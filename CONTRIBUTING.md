# Mitwirken an hud

Die verbindlichen Regeln stehen in [AGENTS.md](AGENTS.md). Diese Datei beschreibt den Ablauf.

## Voraussetzungen

- macOS mit Xcode Command Line Tools
- Rust über `rustup`; die Version kommt automatisch aus `rust-toolchain.toml`
- Werkzeuge:

```sh
cargo install --locked cargo-nextest cargo-deny cargo-insta
brew install gitleaks
```

## Bauen und starten

```sh
cargo run -p hud
```

Logs: `~/Library/Logs/hud/`. Ausführlicher mit `HUD_LOG=debug cargo run -p hud`.

## Ablauf

1. Issue anlegen oder ein bestehendes wählen. Alles Geplante und Offene lebt in Issues, nicht in der Doku.
2. Branch von `main`: `feat/<kurz>`, `fix/<kurz>`, `docs/<kurz>`.
3. Von innen nach außen bauen: Domain-Typen und Regel-Tests → Port und Use Case → Adapter → TUI.
4. Qualitäts-Gates lokal ausführen (siehe unten).
5. PR öffnen. Der Titel folgt Conventional Commits, z. B. `feat(git): auto fast-forward for clean repos`.
6. Squash-Merge nach grüner CI.

## Dokumentation

`docs/PRD.md`, `docs/ARCHITECTURE.md` und `docs/DESIGN.md` beschreiben immer den aktuellen Zustand, ohne Historie. Wer Verhalten, Architektur oder Design ändert, aktualisiert das betroffene Dokument und seine Diagramme im selben PR. Details: [AGENTS.md](AGENTS.md#dokumentation-ist-as-is).

## Qualitäts-Gates

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo nextest run --workspace --locked
cargo deny check
gitleaks git --redact
```

Snapshot-Tests prüfen und annehmen: `cargo insta review`.

## Commit-Nachrichten

[Conventional Commits](https://www.conventionalcommits.org/) auf **Englisch**: `type(scope): description`, z. B. `fix(npm): treat exit code 1 of npm outdated as success`.

| Typ | Wirkung auf die Version |
|---|---|
| `feat` | Minor |
| `fix`, `perf` | Patch |
| `feat!` / `BREAKING CHANGE:` | Major (vor 1.0: Minor) |
| `docs`, `test`, `refactor`, `chore`, `ci` | keine |

## Releases

release-please pflegt auf `main` einen Release-PR mit Versionssprung und `CHANGELOG.md`. Wird er gemergt, entstehen Tag und GitHub-Release.

## TUI-Screenshots

Für Reviews (z. B. am Smartphone) rendert dieses Skript jeden Tab als PNG nach `target/screenshots/`, im Format eines etwa 1800 × 1800 px großen Terminalfensters mit Hack Nerd Font:

```sh
scripts/tui-screenshots.sh            # 115 × 50 Zellen
scripts/tui-screenshots.sh 160 60     # eigene Größe
```

## README-Grafiken

Logo, Wortmarke und Showcase in `docs/assets/` werden erzeugt, nicht von Hand bearbeitet:

```sh
uv run scripts/gen-readme-assets.py
```

## UI-Texte

Jeder neue Text braucht einen Eintrag in `crates/hud-tui/locales/de/hud.ftl` und `crates/hud-tui/locales/en/hud.ftl`. Ein Test prüft, dass beide Dateien dieselben IDs haben.
