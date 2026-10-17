---
status: akzeptiert
datum: 2026-10-17
---

# Clean Architecture mit einem Crate pro Schicht

hud ist in die Crates `hud-domain`, `hud-application`, `hud-infrastructure`, `hud-tui` und das Binary `hud` geschnitten; Abhängigkeiten zeigen nur nach innen. Wir nehmen den Mehraufwand eigener Crates in Kauf, weil der Compiler so die Schichtgrenzen erzwingt und die Domain ohne I/O in Sekunden baut und testet.

## Betrachtete Optionen

- Ein Crate mit Modulen: Grenzen nur per Konvention und Review, erodieren erfahrungsgemäß.
- Ein Crate pro Adapter (`hud-git`, `hud-brew`, …): rund doppelt so viele Crates ohne zusätzlichen Schutz der Kernregeln.
