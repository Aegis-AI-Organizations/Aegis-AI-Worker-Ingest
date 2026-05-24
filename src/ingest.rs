use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

pub fn startup_message() -> &'static str {
    "Aegis AI Worker Ingest started."
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SystemEvent {
    Log { source: String, message: String },
    Metric { name: String, value: f64 },
}

#[derive(Debug)]
pub struct EventEnvelope {
    pub event: SystemEvent,
    ack: Option<oneshot::Sender<()>>,
}

impl EventEnvelope {
    pub fn new(event: SystemEvent) -> (Self, oneshot::Receiver<()>) {
        let (ack_tx, ack_rx) = oneshot::channel();
        (
            Self {
                event,
                ack: Some(ack_tx),
            },
            ack_rx,
        )
    }

    fn ack(mut self) {
        if let Some(ack) = self.ack.take() {
            let _ = ack.send(());
        }
    }
}

pub trait EventProcessor: Send + Sync + 'static {
    fn process(&self, event: SystemEvent) -> impl std::future::Future<Output = Result<()>> + Send;
}

pub struct BatchedEvent {
    pub event: SystemEvent,
    pub ack: oneshot::Sender<Result<()>>,
}

#[derive(clickhouse::Row, serde::Serialize)]
pub struct ClickHouseEventRow {
    pub event_type: String,
    pub source: String,
    pub message: String,
    pub value: f64,
    pub timestamp: u32,
}

#[derive(Debug, Default)]
pub struct ClickHouseEventProcessor {
    sender: Option<mpsc::Sender<BatchedEvent>>,
}

impl ClickHouseEventProcessor {
    pub fn new(sender: mpsc::Sender<BatchedEvent>) -> Self {
        Self {
            sender: Some(sender),
        }
    }
}

impl EventProcessor for ClickHouseEventProcessor {
    async fn process(&self, event: SystemEvent) -> Result<()> {
        if let Some(ref sender) = self.sender {
            let (tx, rx) = oneshot::channel();
            sender
                .send(BatchedEvent { event, ack: tx })
                .await
                .map_err(|e| anyhow::anyhow!("Failed to send event to batching loop: {}", e))?;
            rx.await
                .map_err(|e| anyhow::anyhow!("Batch ack channel closed: {}", e))?
        } else {
            Ok(())
        }
    }
}

pub async fn flush_batch(batch: &mut Vec<BatchedEvent>, client: &clickhouse::Client) {
    if batch.is_empty() {
        return;
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32;

    let rows: Vec<ClickHouseEventRow> = batch
        .iter()
        .map(|item| {
            let (event_type, source, message, value) = match &item.event {
                SystemEvent::Log { source, message } => {
                    ("Log".to_string(), source.clone(), message.clone(), 0.0)
                }
                SystemEvent::Metric { name, value } => {
                    ("Metric".to_string(), name.clone(), "".to_string(), *value)
                }
            };
            ClickHouseEventRow {
                event_type,
                source,
                message,
                value,
                timestamp,
            }
        })
        .collect();

    let result = async {
        let mut insert = client.insert("system_events")?;
        for row in rows {
            insert.write(&row).await?;
        }
        insert.end().await?;
        Ok::<(), clickhouse::error::Error>(())
    }
    .await;

    match result {
        Ok(()) => {
            for item in batch.drain(..) {
                let _ = item.ack.send(Ok(()));
            }
        }
        Err(err) => {
            let err_str = err.to_string();
            eprintln!("Failed to flush batch to ClickHouse: {}", err_str);
            for item in batch.drain(..) {
                let _ = item
                    .ack
                    .send(Err(anyhow::anyhow!("ClickHouse error: {}", err_str)));
            }
        }
    }
}

pub async fn run_batching_loop(
    mut receiver: mpsc::Receiver<BatchedEvent>,
    client: clickhouse::Client,
) {
    let mut batch = Vec::with_capacity(1000);
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
    // Skip the first tick
    interval.tick().await;

    loop {
        tokio::select! {
            maybe_event = receiver.recv() => {
                match maybe_event {
                    Some(event) => {
                        batch.push(event);
                        if batch.len() >= 1000 {
                            flush_batch(&mut batch, &client).await;
                        }
                    }
                    None => {
                        if !batch.is_empty() {
                            flush_batch(&mut batch, &client).await;
                        }
                        break;
                    }
                }
            }
            _ = interval.tick() => {
                if !batch.is_empty() {
                    flush_batch(&mut batch, &client).await;
                }
            }
        }
    }
}

pub fn spawn_ingest_loop<P>(
    mut receiver: mpsc::Receiver<EventEnvelope>,
    processor: Arc<P>,
) -> JoinHandle<()>
where
    P: EventProcessor,
{
    tokio::spawn(async move {
        while let Some(envelope) = receiver.recv().await {
            let processor = Arc::clone(&processor);

            tokio::spawn(async move {
                let event = envelope.event.clone();
                if processor.process(event).await.is_ok() {
                    envelope.ack();
                }
            });
        }
    })
}

pub fn spawn_default_ingest_loop(receiver: mpsc::Receiver<EventEnvelope>) -> JoinHandle<()> {
    spawn_ingest_loop(receiver, Arc::new(ClickHouseEventProcessor::default()))
}

#[cfg(test)]
mod tests {
    use super::{
        ClickHouseEventProcessor, EventEnvelope, EventProcessor, SystemEvent, spawn_ingest_loop,
    };
    use anyhow::{Result, anyhow};
    use std::collections::VecDeque;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::sync::{Mutex, Notify, mpsc};
    use tokio::time::{Duration, timeout};

    #[derive(Debug)]
    struct RecordingProcessor {
        calls: Arc<AtomicUsize>,
        seen: Arc<Mutex<Vec<SystemEvent>>>,
        gate: Arc<Notify>,
        fail_sources: VecDeque<String>,
    }

    impl RecordingProcessor {
        fn new() -> Self {
            Self {
                calls: Arc::new(AtomicUsize::new(0)),
                seen: Arc::new(Mutex::new(Vec::new())),
                gate: Arc::new(Notify::new()),
                fail_sources: VecDeque::new(),
            }
        }

        fn with_fail_source(source: &str) -> Self {
            let mut processor = Self::new();
            processor.fail_sources.push_back(source.to_string());
            processor
        }
    }

    impl EventProcessor for RecordingProcessor {
        async fn process(&self, event: SystemEvent) -> Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.seen.lock().await.push(event.clone());

            if let SystemEvent::Log { source, .. } = &event {
                if self.fail_sources.iter().any(|item| item == source) {
                    return Err(anyhow!("simulated failure"));
                }
                if source == "slow" {
                    self.gate.notified().await;
                }
            }

            Ok(())
        }
    }

    #[test]
    fn startup_message_matches_expected_banner() {
        assert_eq!(super::startup_message(), "Aegis AI Worker Ingest started.");
    }

    #[tokio::test]
    async fn acks_only_after_successful_processing() {
        let (tx, rx) = mpsc::channel(4);
        let processor = Arc::new(RecordingProcessor::new());
        let gate = Arc::clone(&processor.gate);

        let loop_handle = spawn_ingest_loop(rx, Arc::clone(&processor));
        let (envelope, slow_ack) = EventEnvelope::new(SystemEvent::Log {
            source: "slow".to_string(),
            message: "hello".to_string(),
        });

        tx.send(envelope).await.unwrap();

        assert!(timeout(Duration::from_millis(50), slow_ack).await.is_err());
        gate.notify_waiters();
        let (envelope, fast_ack) = EventEnvelope::new(SystemEvent::Metric {
            name: "cpu_usage".to_string(),
            value: 12.0,
        });
        tx.send(envelope).await.unwrap();
        timeout(Duration::from_millis(100), fast_ack)
            .await
            .unwrap()
            .unwrap();

        loop_handle.abort();
    }

    #[tokio::test]
    async fn does_not_ack_failed_processing() {
        let (tx, rx) = mpsc::channel(4);
        let processor = Arc::new(RecordingProcessor::with_fail_source("bad-source"));

        let loop_handle = spawn_ingest_loop(rx, processor);
        let (envelope, ack_rx) = EventEnvelope::new(SystemEvent::Log {
            source: "bad-source".to_string(),
            message: "broken".to_string(),
        });

        tx.send(envelope).await.unwrap();

        let res = timeout(Duration::from_millis(100), ack_rx).await;
        assert!(matches!(res, Ok(Err(_)) | Err(_)));
        loop_handle.abort();
    }

    #[tokio::test]
    async fn processes_events_without_blocking_the_receive_loop() {
        let (tx, rx) = mpsc::channel(4);
        let processor = Arc::new(RecordingProcessor::new());
        let seen = Arc::clone(&processor.seen);
        let gate = Arc::clone(&processor.gate);

        let loop_handle = spawn_ingest_loop(rx, Arc::clone(&processor));

        let (slow_envelope, slow_ack) = EventEnvelope::new(SystemEvent::Log {
            source: "slow".to_string(),
            message: "first".to_string(),
        });
        let (fast_envelope, fast_ack) = EventEnvelope::new(SystemEvent::Log {
            source: "fast".to_string(),
            message: "second".to_string(),
        });

        tx.send(slow_envelope).await.unwrap();
        tx.send(fast_envelope).await.unwrap();

        timeout(Duration::from_millis(200), async {
            loop {
                if seen.lock().await.len() >= 2 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        assert!(timeout(Duration::from_millis(50), fast_ack).await.is_ok());
        assert!(timeout(Duration::from_millis(50), slow_ack).await.is_err());

        gate.notify_waiters();
        loop_handle.abort();
    }

    #[tokio::test]
    async fn default_processor_accepts_events() {
        let (tx, rx) = mpsc::channel(2);
        let loop_handle = spawn_ingest_loop(rx, Arc::new(ClickHouseEventProcessor::default()));
        let (envelope, ack_rx) = EventEnvelope::new(SystemEvent::Metric {
            name: "cpu_usage".to_string(),
            value: 42.0,
        });

        tx.send(envelope).await.unwrap();
        timeout(Duration::from_millis(100), ack_rx)
            .await
            .unwrap()
            .unwrap();

        loop_handle.abort();
    }
}
