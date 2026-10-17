# Mitwirken an hud

Die verbindlichen Regeln stehen in [AGENTS.md](AGENTS.md). Diese Datei beschreibt den Ablauf.

## Voraussetzungen

- macOS mit Xcode Command Line Tools
- Rust über `rustup`; die Version kommt automatisch aus `rust-toolchain.toml`
- Werkzeuge:

```sh
cargo install --locked cargo-nextest cargo-deny cargo-insta
brew install gitleaks uv
```

`uv` braucht es nur für die Skripte unter `scripts/` (Screenshots, README-Grafiken).

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
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
gitleaks git --redact
```

Die CI führt dieselben Schritte aus (`gitleaks` als eigener Job) und prüft zusätzlich, dass der PR-Titel Conventional Commits folgt (`pr-title.yml`).

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

release-please pflegt auf `main` einen Release-PR mit Versionssprung und `CHANGELOG.md`. Es läuft mit dem Secret `RELEASE_PLEASE_TOKEN`; nur deshalb löst das Release den Build-Workflow aus. Auf diesem PR und auf jedem PR, der `release-build.yml` selbst ändert, baut der Workflow das Universal Binary zur Probe. Wird der PR gemergt, entstehen Tag und GitHub-Release, und derselbe Workflow

1. baut `hud` für Apple Silicon und Intel und führt beide per `lipo` zusammen,
2. hängt `hud-<version>-universal-apple-darwin.tar.gz` mit `.sha256` und Build-Provenance an das Release,
3. prüft `hud --version` und ergänzt den Release-Text um einen Installationshinweis,
4. schreibt `Formula/hud.rb` in `nimble-123/homebrew-tap` neu (Secret `HOMEBREW_TAP_TOKEN`; fehlt es, warnt der Workflow nur, und die Formel zeigt weiter auf das vorige Release).

Für ein bestehendes Tag lässt sich der Workflow manuell mit `workflow_dispatch` neu starten.

## TUI-Screenshots

Für Reviews (z. B. am Smartphone) rendert dieses Skript jeden Tab als PNG nach `target/screenshots/`, im Format eines etwa 1800 × 1800 px großen Terminalfensters mit Hack Nerd Font:

```sh
scripts/tui-screenshots.sh            # 115 × 50 Zellen
scripts/tui-screenshots.sh 160 60     # eigene Größe
```

## README-Grafiken

Die Grafiken in `docs/assets/` werden erzeugt, nicht von Hand bearbeitet:

```sh
uv run scripts/gen-readme-assets.py   # Logo und Wortmarke (hell, dunkel)
scripts/gen-showcase.sh               # Showcase: echte Übersicht mit Beispieldaten, hell und dunkel
```

## UI-Texte

Jeder neue Text braucht einen Eintrag in `crates/hud-tui/locales/de/hud.ftl` und `crates/hud-tui/locales/en/hud.ftl`. Ein Test prüft, dass beide Dateien dieselben IDs haben.
