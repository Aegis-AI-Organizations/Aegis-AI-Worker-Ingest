# 📥 Quickstart : Worker Ingest

Le Worker Ingest est un service Rust qui termine les flux de télémétrie mTLS des
Agents, tamponne les événements dans Redis et les écrit par lots dans ClickHouse.

---

## Prérequis

- Rust 1.85+ (Tokio).
- Redis (tampon chaud) et ClickHouse (magasin OLAP) joignables.
- Matériel mTLS : CA interne plus un certificat/clé serveur pour l'endpoint
  Ingest.

## Développement local

```bash
cargo build
cargo test
cargo run
```

## Build du conteneur

```bash
docker build -t aegis-worker-ingest .
```

## Checklist de configuration

- Les endpoints Redis et ClickHouse sont joignables.
- Le certificat mTLS, la clé et la CA sont montés ; les certificats client sont
  validés contre la CA interne.
- Le contexte tenant est dérivé de **métadonnées de confiance** (identité client
  validée), jamais du contenu de payload non signé.
- Les limites de taille et de schéma des payloads sont configurées.
- La taille de lot (~10k événements) et l'intervalle de flush sont réglés pour la
  charge cible.
- Le comportement de retry est idempotent — les soumissions en double ne doivent
  pas produire de double écriture.

---

*Ingénierie Télémétrie et Données Aegis AI — 2026*
