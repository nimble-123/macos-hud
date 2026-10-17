---
status: akzeptiert
datum: 2026-10-17
---

# Untracked Dateien blockieren den Fast-Forward nicht

Nur geänderte oder gestagte getrackte Dateien verhindern den automatischen Fast-Forward, untracked Dateien nicht. Git bricht `merge --ff-only` von selbst ab, wenn der Upstream eine gleichnamige Datei mitbringt, sodass nichts überschrieben wird; eine strengere Regel würde viele Repos mit lokalen Notizen oder Build-Resten dauerhaft überspringen.

## Betrachtete Optionen

- Jede untracked Datei blockiert: sicher, aber häufige Skips ohne echten Schutzgewinn.
