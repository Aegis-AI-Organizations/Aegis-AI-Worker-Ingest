# 📊 Télémétrie du Worker Ingest

Le Worker Ingest est lui-même un pipeline de télémétrie ; voici les signaux qui
décrivent sa propre santé.

---

## Métriques recommandées

- Payloads reçus.
- Payloads rejetés pour schéma ou taille.
- Latence de traitement (réception → Redis, Redis → lot ClickHouse).
- Taille de lot et taux de flush.
- Nombre de retries.
- Échecs d'écriture Redis et ClickHouse.
- Profondeur de file / retard consommateur.
- Débit (événements par seconde par cœur).

---

## Visibilité du scaling (KEDA)

Le pool Ingest scale sur la longueur de la file Redis. Suivez le retard de file et
le temps de démarrage à froid, pas seulement le nombre de réplicas ; le
scale-to-zero ne s'applique que lorsque le tampon est vide.

---

## Logs

Les logs incluent des identifiants sûrs pour le tenant et des clés d'objets —
jamais le contenu brut des payloads, sauf activation explicite dans un
environnement de debug local. L'identité client mTLS est loguée pour l'audit ;
les certificats et clés ne le sont pas.

---

*Ingénierie Télémétrie et Données Aegis AI — 2026*
