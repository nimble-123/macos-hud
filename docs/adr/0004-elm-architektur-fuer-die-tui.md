---
status: akzeptiert
datum: 2026-10-17
---

# Elm-Architektur für die TUI

Die Oberfläche besteht aus einem `Model`, reinen Funktionen `update(model, msg) -> Vec<Command>` und `view(model, frame)` und einer Schleife, die nur rendert und Nachrichten verteilt. Alle I/O läuft in Tokio-Tasks und kommt als `AppEvent` zurück; dadurch blockiert die UI nie und ist vollständig ohne Terminal testbar.

## Betrachtete Optionen

- Komponenten mit eigenem Zustand und eigener I/O: schwer testbar, Gefahr blockierender Aufrufe im Render-Pfad.
