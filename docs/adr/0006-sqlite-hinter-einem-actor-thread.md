---
status: akzeptiert
datum: 2026-10-17
---

# SQLite hinter einem dedizierten Actor-Thread

Ein eigener Thread besitzt die einzige `rusqlite`-Verbindung; Anfragen kommen per `mpsc`, Antworten per `oneshot`. Das garantiert einen einzigen Schreiber und hält blockierende SQLite-Aufrufe aus der Tokio-Runtime heraus, ohne eine zusätzliche Async-Datenbankbibliothek.

## Betrachtete Optionen

- `tokio-rusqlite`: gleiche Idee, aber zusätzliche Abhängigkeit für wenig Code.
- `sqlx`: schwergewichtig für einen lokalen Cache, Makros mit Build-Zeit-Datenbank.
