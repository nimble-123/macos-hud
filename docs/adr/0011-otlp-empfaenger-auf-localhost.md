---
status: akzeptiert
datum: 2026-10-17
---

# OTLP-Empfänger auf localhost für Claude-Code-Telemetrie

Claude Code liefert per OpenTelemetry Werte, die in den JSONL-Logs fehlen (aktive Zeit, Codezeilen, Commits, PRs, Kosten laut Claude). hud nimmt diese Metriken selbst entgegen: ein OTLP-HTTP/JSON-Empfänger, der nur auf `127.0.0.1` lauscht und nur läuft, solange hud läuft. Er ist der erste eigene Netzwerkzugang von hud; die JSONL-Logs bleiben die vollständige Historie.

## Betrachtete Optionen

- Externer OTel Collector mit Datei-Export, den hud liest: kein Netzwerkzugang in hud, aber ein zusätzlicher Dienst, den Nutzer installieren und betreiben müssen; widerspricht dem Ziel „ein Binary, nichts weiter“.
- Hintergrunddienst, der immer empfängt: keine Lücken, aber ADR 0008 (kein Hintergrunddienst) wäre aufgehoben.
- OTLP über gRPC: Protobuf und HTTP/2 als zusätzliche Abhängigkeiten ohne Nutzen für ein lokales Ziel.

## Konsequenzen

- Was Claude Code sendet, während hud nicht läuft, geht verloren; der KI-Tab kennzeichnet Telemetriewerte deshalb als unvollständig, wenn hud nicht im ganzen Zeitraum lief.
- Gespeichert werden nur Metrikname, Wert, Zeit, `session.id`, `model` und `type`. Alle anderen Attribute (u. a. `user.email`, Konto- und Organisations-IDs) werden beim Parsen verworfen; Log-Events werden nicht ausgewertet.
- Nur Delta-Temporality (Standard von Claude Code); kumulative Datenpunkte werden ignoriert, damit nichts doppelt zählt.
- Anfragen über 4 MiB werden abgewiesen; ein belegter Port ist ein Zustand der Quelle, kein Absturz.
