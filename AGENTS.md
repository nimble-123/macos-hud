# AGENTS.md

Verbindliche Arbeitsregeln für KI-Agenten und Menschen in diesem Repository. `CLAUDE.md` importiert diese Datei.

## Projekt

`hud` ist ein Head-up-Display für die Entwicklermaschine im macOS-Terminal (Rust, Ratatui, Tokio).

| Dokument | Inhalt |
|---|---|
| [docs/PRD.md](docs/PRD.md) | Anforderungen, Abnahmekriterien |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Architektur nach arc42, ADRs |
| [docs/DESIGN.md](docs/DESIGN.md) | Product Identity, Farben, TUI-Layout |
| GitHub Issues & Milestones | alles Geplante und Offene |

Bei Widerspruch gilt: PRD > ARCHITECTURE > DESIGN > Code-Kommentare. Widersprüche nicht still auflösen, sondern als Issue melden.

## Dokumentation ist AS-IS

- `docs/PRD.md`, `docs/ARCHITECTURE.md` und `docs/DESIGN.md` beschreiben **ausschließlich den aktuellen Zustand** des Produkts.
- Keine Historie in diesen Dokumenten: keine „früher/neu/geändert“-Hinweise, keine Änderungslisten, keine Versionsvergleiche, keine durchgestrichenen oder abgelösten Inhalte. Die Historie steht in Git, im `CHANGELOG.md` (release-please) und in Issues/PRs.
- Ändert ein PR Verhalten, Architektur oder Design, passt **derselbe PR** das betroffene Dokument an, sodass es danach den neuen Ist-Zustand beschreibt.
- Diagramme (`docs/diagrams/`) gehören zur AS-IS-Doku und werden im selben PR aktualisiert.

## Architekturentscheidungen (ADRs)

- ADRs liegen einzeln in `docs/adr/NNNN-titel.md`, Vorlage `docs/adr/TEMPLATE.md`, fortlaufend nummeriert.
- Ein ADR nur dann, wenn die Entscheidung **schwer umkehrbar**, **ohne Kontext überraschend** und das Ergebnis einer **echten Abwägung** ist. Sonst gehört sie in die AS-IS-Doku oder nirgendwohin.
- ADRs sind Entscheidungsprotokolle und werden nach `akzeptiert` inhaltlich nicht mehr geändert. Eine neue Entscheidung bekommt ein neues ADR; das alte erhält den Status `abgelöst durch ADR-NNNN`.
- Der Index in `ARCHITECTURE.md` §9 wird im selben PR aktualisiert.

## Planung über GitHub Issues

- Geplante Änderungen, neue Features, offene Punkte, Fragen, Bugs und technische Schulden werden **ausschließlich als GitHub Issues** geführt (Templates in `.github/ISSUE_TEMPLATE/`), gruppiert über Milestones.
- Keine TODO-Listen, Roadmaps oder „offene Punkte“-Abschnitte in den Doku-Dateien.
- `TODO`/`FIXME` im Code nur mit Issue-Nummer: `// TODO(#42): …`.
- Jeder PR verweist auf sein Issue (`Closes #42`).
- Wer während der Arbeit etwas Offenes entdeckt, legt ein Issue an, statt es nebenbei zu lösen oder in die Doku zu schreiben.

## Skill-Nutzung

Agenten mit Skill-Unterstützung (z. B. Claude Code) **müssen** vor der jeweiligen Tätigkeit den passenden Skill laden und befolgen. Agenten ohne Skill-Unterstützung wenden die Regeln dieser Datei sinngemäß an.

| Tätigkeit | Skills |
|---|---|
| Rust-Code schreiben, ändern oder reviewen | `rust-skills` und `rust-best-practices` |
| Diagramme erstellen oder ändern | `diagram-design` mit Profil `hud` (Marker `.diagram-design`) |
| TUI-Gestaltung, Theme, README, Grafiken | `frontend-design` und `make-interfaces-feel-better`, jeweils im Rahmen von `docs/DESIGN.md` |
| Begriffe, Glossar, ADRs | `domain-modeling` |
| App starten und Änderung im echten Terminal prüfen | `run` |
| Review vor dem PR | `code-review`; bei Prozessaufrufen, Dateizugriffen oder Logs zusätzlich `security-review` |

Weicht ein Skill von dieser Datei ab, gilt diese Datei; die Abweichung wird als Issue gemeldet.

## Architekturgrenzen

```
hud (Binary, Composition Root)
 ├─ hud-tui            → hud-application, hud-domain
 ├─ hud-infrastructure → hud-application, hud-domain
 └─ hud-application    → hud-domain
```

- `hud-domain`: keine I/O, kein Tokio, kein Serde-Format, kein Ratatui. Regeln sind reine Funktionen.
- `hud-application`: Use Cases und Ports (Traits). Keine Prozesse, keine Dateien, kein SQLite.
- `hud-infrastructure`: Adapter. Kein Ratatui.
- `hud-tui`: kennt `hud-infrastructure` nicht. Enthält alle UI-Texte (i18n).
- Neue Abhängigkeiten nur über `[workspace.dependencies]`.

## Sicherheitsregeln (nicht verhandelbar)

- Fast-Forward und Paket-Installation nur über `FastForwardPermit` bzw. `InstallPermit`. Keine Abkürzung, kein zweiter Weg.
- Keine Funktion darf lokale Änderungen in Repositories überschreiben.
- Fremdwerkzeuge nur über den `ProcessRunner`, Argumente als Liste, nie über `sh -c`.
- Keine Prompt-Inhalte, Tokens oder Zugangsdaten loggen oder speichern. `~/.codex/auth.json` nie öffnen.
- Tests führen nie echtes `brew upgrade`, `npm install -g` oder `npx` aus.

## Rust-Konventionen

- Edition 2024, Toolchain aus `rust-toolchain.toml`.
- Fehler: `thiserror` in Bibliotheks-Crates, `anyhow` nur im Binary. Meldungen klein, ohne Punkt am Ende.
- Kein `unwrap`. `expect` nur für echte Invarianten mit Text `"BUG: …"` und `#[expect(clippy::expect_used, reason = "…")]`.
- Lint-Ausnahmen mit `#[expect(…, reason = "…")]`, nie `#[allow]`.
- Statische Ports: `fn … -> impl Future<Output = …> + Send`. Dyn-Ports: `BoxFuture`. Kein `async-trait`.
- Newtypes statt `String`/`PathBuf` an API-Grenzen; Parsen an der Grenze.
- Entscheidungen als Enums mit `#[must_use]`; `match` ohne `_`-Zweig auf eigenen Enums.
- Keine Sperre über `.await` halten (Ausnahme: Ressourcen-Sperren pro Paketmanager/Repo).
- Begrenzte Channels. Blockierendes in `spawn_blocking` oder im DB-Thread.
- `tracing` statt `println!`; strukturierte Felder.
- Kommentare erklären das *Warum*. Öffentliche Items in Domain und Application mit `///`, fehlbare Funktionen mit `# Errors`.

## Tests

- Testnamen beschreiben Verhalten: `sync_decision_skips_when_tracked_changes`.
- Domain-Regeln: Unit-Tests plus `proptest`. Use Cases: gegen Fakes, `mockall` nur für Aufruf-Verifikation. Adapter: Fixtures und temporäre Git-Repos. TUI: `insta`-Snapshots in `de` und `en`.

## Qualitäts-Gates (vor jedem Commit)

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo nextest run --workspace --locked
cargo deny check
```

## Commits und PRs

- Conventional Commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`, `ci:`, `perf:`; Scope optional, z. B. `feat(git): …`. Breaking Changes mit `!`.
- Ein PR = ein Thema. Squash-Merge; der PR-Titel wird zur Commit-Nachricht und steuert release-please.
- Neue UI-Texte immer in beiden Sprachen (`crates/hud-tui/locales/{de,en}/hud.ftl`).
- AS-IS-Doku im selben PR anpassen, wenn sich Verhalten, Architektur oder Design ändert (siehe oben).

## Sprache

- Doku, Issues, PRs: Deutsch.
- Ausnahme: `README.md` ist Englisch.
- Code, Bezeichner, Kommentare, Log-Meldungen, Commit-Nachrichten und PR-Titel: Englisch.
