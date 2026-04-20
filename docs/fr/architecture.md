# 📥 Architecture de l'Ingest : Passerelle de Télémétrie Haute Performance

Le **Worker Ingest Aegis AI** est la couche d'ingestion haute performance de la plateforme. Écrit en **Rust** pour gérer une concurrence massive avec une efficacité zéro copie, il sert de point d'entrée pour toute la télémétrie transmise par les Agents externes.

---

## 🏗️ Principes de Conception de Base

Le Worker Ingest est conçu pour la **fiabilité**, le **débit** et l'**intégrité des données** :

1. **Sécurité de la Mémoire** : Exploitation de Rust pour prévenir les dépassements de tampon et les conditions de concurrence dans le chemin de données à haute intensité.
2. **Ingestion à Deux Étapes** :
   - **Tampon Chaud (Redis)** : Les événements entrants sont immédiatement poussés vers Redis pour l'alerte en temps réel et la déduplication.
   - **Écriture par Lots (ClickHouse)** : Les événements sont consolidés dans des lots optimisés (~10 000 événements) pour une performance d'ingestion OLAP massive dans ClickHouse.
3. **Terminaison mTLS** : Sert de terminus sécurisé pour les flux gRPC bidirectionnels provenant de milliers d'Agents.

---

## 🔐 Communication Sécurisée (mTLS)

La couche Ingest impose un **Mutual TLS (mTLS)** strict pour toutes les connexions :

- **Validation des Certificats** : Le serveur Ingest vérifie le certificat client de l'Agent par rapport à la racine CA Interne d'Aegis.
- **Confiance Bidirectionnelle** : La communication ne se poursuit que si l'Agent et le serveur Ingest prouvent leur identité cryptographique.
- **Application de l'Identité** : Le worker valide le `Common Name` ou le `SAN` de l'Agent pour s'assurer qu'il appartient à un espace de travail client autorisé.

---

## 🌊 Mise à l'Échelle Dynamique (KEDA)

Le pool Ingest est géré par **KEDA** (Kubernetes Event-Driven Autoscaling) pour gérer les pics d'ingestion :

- **Conscience de la File d'Attente** : KEDA interroge la longueur de la file d'attente d'ingestion dans Redis.
- **Mise à l'Échelle Rapide** : Lorsque la taille de la file dépasse le seuil de sécurité, KEDA lance davantage de workers Ingest pour traiter le retard.
- **Efficacité des Coûts** : Se réduit à une base minimale lorsque le trafic est faible.

```mermaid
graph LR
    Agents[Agents Aegis] -- "gRPC / mTLS" --> Nginx[Nginx Ingress]
    Nginx -- "Sortie" --> Ingest[Ingest Worker (Rust)]
    Ingest -- "Poussée" --> Redis[(Tampon Chaud Redis)]
    Ingest -- "Écriture par lots" --> ClickHouse[(OLAP ClickHouse)]
    Redis -- "Longueur File" --> KEDA[Opérateur KEDA]
    KEDA -- "Mise à l'échelle" --> Ingest
```

---

## ⚙️ Spécifications Techniques

- **Capacité de Débit** : > 10 000 Événements Par Seconde (EPS) par cœur de processeur.
- **Sérialisation** : Protobuf-v3 (via Tonic/Prost).
- **Persistance** : ClickHouse (via `clickhouse-rs`) et Redis (via `redis-rs`).

---

*Ingénierie Télémétrie et Données Aegis AI — 2026*
