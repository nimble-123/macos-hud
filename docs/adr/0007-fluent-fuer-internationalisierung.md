---
status: akzeptiert
datum: 2026-10-17
---

# Fluent für die Zweisprachigkeit

Alle UI-Texte liegen als Fluent-Ressourcen (`crates/hud-tui/locales/{de,en}/hud.ftl`) und werden über `fluent-templates` geladen; Domain und Application liefern nur Enums. Fluent kann Plural- und Grammatikregeln ausdrücken, die bei Zählern wie „1 Repo / 3 Repos“ ständig vorkommen.

## Betrachtete Optionen

- `rust-i18n` (YAML): einfacher, aber ohne ausdrucksstarke Plural- und Auswahlregeln.
