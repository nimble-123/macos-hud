---
status: akzeptiert
datum: 2026-10-17
---

# Git über die CLI statt über eine Bibliothek

hud ruft `git` als Prozess auf (`status --porcelain=v2`, `fetch`, `merge --ff-only`). So verhält sich hud exakt wie Git im Terminal der Nutzer:in, inklusive Konfiguration, Credential-Helper und Hooks.

## Betrachtete Optionen

- `git2` (libgit2): C-Abhängigkeit, Abweichungen bei Konfiguration und Authentifizierung.
- `gix`: Fast-Forward-Merge und Fetch-Authentifizierung nicht in gleichem Umfang wie die CLI.
