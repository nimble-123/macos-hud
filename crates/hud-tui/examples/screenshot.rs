//! Renders every tab into an SVG that mimics a terminal window.
//!
//! Used by `scripts/tui-screenshots.sh` to produce PNGs for reviews. The grid
//! defaults to 115 × 50 cells, roughly a 1800 × 1800 px Warp window on Retina.
//!
//! Usage: `cargo run -p hud-tui --example screenshot -- <out-dir> [cols] [rows]`
#![expect(
    clippy::expect_used,
    reason = "hard-coded sample data must fail loudly when it becomes invalid"
)]

use std::fmt::Write as _;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;
use std::time::Instant;

use hud_application::usecases::workspaces::{RepoReport, RepoState, WorkspaceReport};
use hud_application::{AppEvent, ReportSource};
use hud_domain::WorkspacePath;
use hud_domain::git::{
    BranchName, CommitId, GitOperation, Head, RepoPath, RepoStatus, SkipReason, Upstream,
};
use hud_tui::{Appearance, Language, Model, Msg, Tab, UiSettings, update, view};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

const CELL_W: f64 = 15.65;
const CELL_H: f64 = 33.8;
const FONT_SIZE: f64 = 26.0;
const PAD: f64 = 24.0;

/// Terminal background and default foreground per appearance.
fn colors(appearance: Appearance) -> (&'static str, &'static str) {
    match appearance {
        Appearance::Dark => ("#161B22", "#E6E8E3"),
        Appearance::Light => ("#F2F3F1", "#1C2128"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().ok_or("missing <out-dir>")?);
    let cols: u16 = args.next().map_or(Ok(115), |v| v.parse())?;
    let rows: u16 = args.next().map_or(Ok(50), |v| v.parse())?;
    std::fs::create_dir_all(&out)?;

    // Every tab in English on dark, plus the overview in the other variants.
    let variants = [
        ("en-dark", Language::En, Appearance::Dark, Tab::ALL.len()),
        ("de-dark", Language::De, Appearance::Dark, 2),
        ("en-light", Language::En, Appearance::Light, 2),
    ];
    for (suffix, language, appearance, tabs) in variants {
        let settings = UiSettings {
            language,
            appearance,
            ..UiSettings::default()
        };
        let mut model = Model::new(settings);
        update(&mut model, Msg::Tick(Instant::now()));
        update(&mut model, Msg::App(sample_workspaces()));
        for tab in Tab::ALL.into_iter().take(tabs) {
            let mut terminal = Terminal::new(TestBackend::new(cols, rows))?;
            terminal.draw(|frame| view(&model, frame))?;
            let name = format!(
                "{:02}-{}-{suffix}.svg",
                tab.index() + 1,
                format!("{tab:?}").to_lowercase()
            );
            std::fs::write(
                out.join(&name),
                to_svg(terminal.backend().buffer(), appearance),
            )?;
            println!("{}", out.join(name).display());
            update(
                &mut model,
                Msg::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            );
        }
    }
    Ok(())
}

/// Realistic sample data so reviews show every state at once.
#[expect(
    clippy::too_many_lines,
    reason = "one literal list of sample repositories"
)]
fn sample_workspaces() -> AppEvent {
    let ws = |path: &str| WorkspacePath::try_from(PathBuf::from(path)).expect("BUG: absolute");
    let report = |path: &str, branch: &str, ahead: u32, behind: u32, state: RepoState| RepoReport {
        repo: RepoPath::try_from(PathBuf::from(path)).expect("BUG: absolute"),
        status: Some(RepoStatus {
            head: if branch.is_empty() {
                Head::Detached
            } else {
                Head::Branch(BranchName::try_from(branch.to_owned()).expect("BUG: name"))
            },
            head_oid: Some(CommitId::try_from("a".repeat(40).as_str()).expect("BUG: id")),
            upstream: Some(Upstream {
                name: format!("origin/{branch}"),
                oid: CommitId::try_from("b".repeat(40).as_str()).expect("BUG: id"),
            }),
            ahead,
            behind,
            tracked_changes: 0,
            untracked: 0,
            operation: None,
        }),
        state,
    };
    AppEvent::Workspaces {
        reports: vec![
            WorkspaceReport {
                workspace: ws("/Users/dev/workspace/projects"),
                auto_fast_forward: true,
                repos: Some(vec![
                    report(
                        "/Users/dev/workspace/projects/api",
                        "main",
                        0,
                        0,
                        RepoState::FastForwarded(3),
                    ),
                    report(
                        "/Users/dev/workspace/projects/billing",
                        "main",
                        0,
                        0,
                        RepoState::UpToDate,
                    ),
                    report(
                        "/Users/dev/workspace/projects/docs-site",
                        "main",
                        0,
                        2,
                        RepoState::Behind(2),
                    ),
                    report(
                        "/Users/dev/workspace/projects/infra-modules",
                        "main",
                        0,
                        0,
                        RepoState::UpToDate,
                    ),
                    report(
                        "/Users/dev/workspace/projects/legacy-portal",
                        "main",
                        2,
                        5,
                        RepoState::Skipped(SkipReason::Diverged {
                            ahead: 2,
                            behind: 5,
                        }),
                    ),
                    report(
                        "/Users/dev/workspace/projects/mobile-app",
                        "feature/login",
                        1,
                        0,
                        RepoState::Skipped(SkipReason::Ahead(1)),
                    ),
                    report(
                        "/Users/dev/workspace/projects/notes",
                        "",
                        0,
                        0,
                        RepoState::Skipped(SkipReason::DetachedHead),
                    ),
                    report(
                        "/Users/dev/workspace/projects/web-shop",
                        "main",
                        0,
                        4,
                        RepoState::Skipped(SkipReason::TrackedChanges),
                    ),
                    report(
                        "/Users/dev/workspace/projects/website",
                        "main",
                        0,
                        0,
                        RepoState::Skipped(SkipReason::OperationInProgress(GitOperation::Rebase)),
                    ),
                ]),
                unavailable: None,
            },
            WorkspaceReport {
                workspace: ws("/Users/dev/oss"),
                auto_fast_forward: false,
                repos: Some(vec![
                    report(
                        "/Users/dev/oss/ratatui",
                        "main",
                        0,
                        12,
                        RepoState::Skipped(SkipReason::OptedOut),
                    ),
                    report("/Users/dev/oss/tokio", "master", 0, 0, RepoState::UpToDate),
                ]),
                unavailable: None,
            },
            WorkspaceReport {
                workspace: ws("/Volumes/Archive/projects"),
                auto_fast_forward: true,
                repos: None,
                unavailable: Some("unavailable".into()),
            },
        ],
        source: ReportSource::Sync,
    }
}

fn hex(color: Color, default: &str, fg: &str) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02X}{g:02X}{b:02X}"),
        Color::Reset => default.to_owned(),
        Color::Black => "#1C2128".into(),
        Color::Red => "#F0675B".into(),
        Color::Green => "#5CC98A".into(),
        Color::Yellow => "#E8B23A".into(),
        Color::Blue => "#6CA0DC".into(),
        Color::Magenta => "#E870C6".into(),
        Color::Cyan => "#4CC0D6".into(),
        Color::Gray => "#AEB5BD".into(),
        Color::DarkGray => "#8F98A3".into(),
        Color::White => fg.to_owned(),
        other => format!("{other}"),
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn to_svg(buffer: &Buffer, appearance: Appearance) -> String {
    let (bg, fg) = colors(appearance);
    let area = buffer.area;
    let width = PAD * 2.0 + f64::from(area.width) * CELL_W;
    let height = PAD * 2.0 + f64::from(area.height) * CELL_H;
    let mut svg = String::new();
    let _ = write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.0} {height:.0}"><rect width="100%" height="100%" fill="{bg}"/><g font-family="Hack Nerd Font Mono, Hack, Menlo, monospace" font-size="{FONT_SIZE}">"#
    );
    for y in 0..area.height {
        let base = PAD + f64::from(y) * CELL_H + CELL_H * 0.72;
        for x in 0..area.width {
            let cell = &buffer[(x, y)];
            let left = PAD + f64::from(x) * CELL_W;
            if cell.bg != Color::Reset {
                let _ = write!(
                    svg,
                    r#"<rect x="{left:.2}" y="{:.2}" width="{CELL_W}" height="{CELL_H}" fill="{}"/>"#,
                    PAD + f64::from(y) * CELL_H,
                    hex(cell.bg, bg, fg)
                );
            }
            let symbol = cell.symbol();
            if symbol.trim().is_empty() {
                continue;
            }
            let weight = if cell.modifier.contains(Modifier::BOLD) {
                "700"
            } else {
                "400"
            };
            let _ = write!(
                svg,
                r#"<text x="{left:.2}" y="{base:.2}" fill="{}" font-weight="{weight}">{}</text>"#,
                hex(cell.fg, fg, fg),
                escape(symbol)
            );
            if cell.modifier.contains(Modifier::UNDERLINED) {
                let _ = write!(
                    svg,
                    r#"<rect x="{left:.2}" y="{:.2}" width="{CELL_W}" height="2" fill="{}"/>"#,
                    base + 5.0,
                    hex(cell.fg, fg, fg)
                );
            }
        }
    }
    svg.push_str("</g></svg>\n");
    svg
}
