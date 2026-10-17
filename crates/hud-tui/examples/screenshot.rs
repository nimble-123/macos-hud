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

use hud_application::ports::updates::{Availability, SourceCheck};
use hud_application::settings::{AutoInstallSettings, Settings, WorkspaceSettings};
use hud_application::usecases::updates::{JobId, JobState, JobUpdate, SourceReport};
use hud_application::usecases::workspaces::{RepoReport, RepoState, WorkspaceReport};
use hud_application::{AppEvent, ReportSource};
use hud_domain::WorkspacePath;
use hud_domain::action::{ActionId, ActionKind, ActionRecord, Outcome, Trigger};
use hud_domain::git::{
    BranchName, CommitId, GitOperation, Head, RepoPath, RepoStatus, SkipReason, Upstream,
};
use hud_domain::update::{
    InstallTarget, PackageKind, PackageName, SourceId, UpdateCandidate, Version,
};
use hud_tui::{
    Appearance, Language, Model, Msg, SetupModel, Tab, UiSettings, update, update_setup, view,
    view_setup,
};
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

    // First-run setup, both steps, German on dark.
    let settings = UiSettings {
        language: Language::De,
        ..UiSettings::default()
    };
    let mut setup = SetupModel::new(
        settings,
        vec![
            "~/workspace".into(),
            "~/workspace/projects".into(),
            "~/Developer".into(),
        ],
    );
    let press = |code| Msg::Key(KeyEvent::new(code, KeyModifiers::NONE));
    update_setup(&mut setup, &press(KeyCode::Down));
    update_setup(&mut setup, &press(KeyCode::Char(' ')));
    for (name, advance) in [
        ("00-setup-1-de-dark.svg", true),
        ("00-setup-2-de-dark.svg", false),
    ] {
        let mut terminal = Terminal::new(TestBackend::new(cols, rows))?;
        terminal.draw(|frame| view_setup(&setup, frame))?;
        std::fs::write(
            out.join(name),
            to_svg(terminal.backend().buffer(), Appearance::Dark),
        )?;
        println!("{}", out.join(name).display());
        if advance {
            update_setup(&mut setup, &press(KeyCode::Enter));
        }
    }

    render_scanning(&out, cols, rows)?;
    render_confirm(&out, cols, rows)?;

    // Every tab in English on dark, plus the overview in the other variants.
    let variants = [
        ("en-dark", Language::En, Appearance::Dark, Tab::ALL.len()),
        ("de-dark", Language::De, Appearance::Dark, Tab::ALL.len()),
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
        update(
            &mut model,
            Msg::App(AppEvent::Settings(Box::new(sample_settings()))),
        );
        update(&mut model, Msg::App(AppEvent::Actions(sample_actions())));
        for sample in sample_load() {
            update(&mut model, Msg::App(AppEvent::System(sample)));
        }
        update(
            &mut model,
            Msg::App(AppEvent::Usage(Box::new(sample_usage()))),
        );
        update(&mut model, Msg::App(sample_workspaces()));
        for event in sample_updates() {
            update(&mut model, Msg::App(event));
        }
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

/// Two minutes of load with a short build peak.
fn sample_load() -> Vec<hud_application::ports::system::SystemSample> {
    (0..60u64)
        .map(|step| hud_application::ports::system::SystemSample {
            cpu_percent: u8::try_from(match step {
                30..=38 => 70 + step % 5 * 5,
                _ => 12 + step * 7 % 15,
            })
            .expect("BUG: percent"),
            memory_used: 10_400_000_000 + step * 15_000_000,
            memory_total: 17_179_869_184,
            processes: 612,
        })
        .collect()
}

/// A busy day with Opus and a few Haiku subagent calls.
fn sample_usage() -> hud_application::usecases::usage::UsageReport {
    use hud_application::usecases::usage::{ModelUsage, PeriodUsage, SourceState, UsageReport};
    use hud_domain::usage::{
        Accuracy, CostEstimate, Measured, ModelId, PRICES_AS_OF, Period, Provider, TokenUsage,
    };
    let tokens = |input, output, write, read| TokenUsage {
        input,
        output,
        cache_write_5m: 0,
        cache_write_1h: write,
        cache_read: read,
        reasoning: None,
    };
    let cost = |micro_usd| CostEstimate {
        micro_usd,
        prices_as_of: PRICES_AS_OF,
    };
    let model = |id: &str, responses, sessions, tokens, micro: Option<u64>| ModelUsage {
        provider: Provider::Claude,
        model: ModelId::try_from(id.to_owned()).expect("BUG: model id"),
        responses,
        sessions,
        tokens,
        cost: micro.map(cost),
    };
    let periods = Period::ALL
        .iter()
        .map(|&period| PeriodUsage {
            period,
            models: vec![
                model(
                    "claude-opus-5-5",
                    1_537,
                    4,
                    tokens(3_120, 212_400, 12_400_000, 180_200_000),
                    Some(137_480_000),
                ),
                model(
                    "claude-haiku-4-5-20251001",
                    214,
                    3,
                    tokens(96_000, 18_300, 0, 0),
                    Some(187_500),
                ),
            ],
            sessions: Measured {
                value: 5,
                accuracy: Accuracy::Exact,
            },
            tokens: Measured {
                value: tokens(99_120, 230_700, 12_400_000, 180_200_000),
                accuracy: Accuracy::Exact,
            },
            cost: Measured {
                value: cost(137_667_500),
                accuracy: Accuracy::Estimated,
            },
            telemetry: Measured {
                value: hud_domain::usage::TelemetryTotals {
                    cost_micro_usd: 131_920_000,
                    active_user_ms: 34 * 60_000,
                    active_cli_ms: 100 * 60_000,
                    lines_added: 1_240,
                    lines_removed: 310,
                    commits: 3,
                    pull_requests: 1,
                    sessions: 5,
                },
                accuracy: Accuracy::Partial,
            },
        })
        .collect();
    UsageReport {
        periods,
        sources: vec![SourceState {
            provider: Provider::Claude,
            error: None,
            unreadable: 0,
        }],
    }
}

/// A failed manual installation on top, then automatic work.
fn sample_actions() -> Vec<ActionRecord> {
    let at = |minute: u8| {
        format!("2026-10-17T12:{minute:02}:00Z")
            .parse::<jiff::Timestamp>()
            .expect("BUG: timestamp")
    };
    let record = |id, minute, kind, target: &str, trigger, outcome, summary: Option<&str>| {
        let started = at(minute);
        ActionRecord {
            id: ActionId(id),
            started_at: started,
            finished_at: Some(started + jiff::SignedDuration::from_secs(14)),
            kind,
            target: target.to_owned(),
            trigger,
            outcome: Some(outcome),
            summary: summary.map(str::to_owned),
        }
    };
    vec![
        record(6, 41, ActionKind::Install, "Npm wrangler 4.81.0 → 4.151.1", Trigger::Manual,
            Outcome::Failed("npm failed: npm error code EACCES\nnpm error syscall rename\nnpm error path /usr/local/lib/node_modules/wrangler".into()), None),
        record(5, 30, ActionKind::Install, "Homebrew wget 1.21.4 → 1.24.5", Trigger::Auto, Outcome::Succeeded, None),
        record(4, 30, ActionKind::Install, "Homebrew firefox 118.0 → 119.0", Trigger::Auto,
            Outcome::Skipped("needs a password".into()), None),
        record(3, 15, ActionKind::FastForward, "/Users/nimble/workspace/projects/hud", Trigger::Auto,
            Outcome::Succeeded, Some("3 commits to 1a2b3c4d5e6f")),
        record(2, 15, ActionKind::Fetch, "/Users/nimble/workspace/projects/legacy", Trigger::Auto,
            Outcome::Failed("git failed: Permission denied (publickey)".into()), None),
        record(1, 2, ActionKind::FastForward, "/Volumes/1TB EXT/workspace/oss/abap-ai-documentation", Trigger::Manual,
            Outcome::Succeeded, Some("1 commit to 9f8e7d6c5b4a")),
    ]
}

/// Two workspaces, one opted out of fast-forward, formulae installed automatically.
fn sample_settings() -> Settings {
    let workspace = |path: &str, auto_fast_forward| WorkspaceSettings {
        path: WorkspacePath::try_from(PathBuf::from(path)).expect("BUG: absolute"),
        auto_fast_forward,
    };
    Settings {
        workspaces: vec![
            workspace("/Users/nimble/workspace/projects", true),
            workspace("/Volumes/1TB EXT/workspace", false),
        ],
        auto_install: AutoInstallSettings {
            brew_formulae: true,
            ..AutoInstallSettings::default()
        },
        ..Settings::default()
    }
}

/// A first scan in progress: four of 79 repositories are done.
fn render_scanning(
    out: &std::path::Path,
    cols: u16,
    rows: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let settings = UiSettings {
        language: Language::De,
        ..UiSettings::default()
    };
    let mut model = Model::new(settings);
    update(&mut model, Msg::Tick(Instant::now()));
    update(
        &mut model,
        Msg::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
    );
    let AppEvent::Workspaces { reports, .. } = sample_workspaces() else {
        unreachable!()
    };
    let mut empty = reports.clone();
    for workspace in &mut empty {
        if workspace.repos.is_some() {
            workspace.repos = Some(Vec::new());
        }
    }
    update(
        &mut model,
        Msg::App(AppEvent::Workspaces {
            reports: empty,
            source: ReportSource::Cache,
        }),
    );
    update(
        &mut model,
        Msg::App(AppEvent::ScanStarted {
            total: 79,
            fetch: false,
        }),
    );
    let first = &reports[0];
    for report in first.repos.iter().flatten().take(4) {
        update(
            &mut model,
            Msg::App(AppEvent::RepoScanned {
                workspace: first.workspace.clone(),
                report: report.clone(),
            }),
        );
    }
    let mut terminal = Terminal::new(TestBackend::new(cols, rows))?;
    terminal.draw(|frame| view(&model, frame))?;
    let name = "02-workspaces-scanning-de-dark.svg";
    std::fs::write(
        out.join(name),
        to_svg(terminal.backend().buffer(), Appearance::Dark),
    )?;
    println!("{}", out.join(name).display());
    Ok(())
}

fn package(
    source: SourceId,
    kind: PackageKind,
    name: &str,
    current: &str,
    latest: &str,
) -> UpdateCandidate {
    UpdateCandidate {
        source,
        kind,
        name: PackageName::try_from(name.to_owned()).expect("BUG: name"),
        current: Version::new(current),
        latest: Version::new(latest),
        pinned: false,
    }
}

/// Update sources with a running npm installation.
fn sample_updates() -> Vec<AppEvent> {
    let check = |candidates| {
        Some(Ok(SourceCheck {
            candidates,
            installed: Vec::new(),
            updates_known: true,
        }))
    };
    let mut pinned = package(
        SourceId::Homebrew,
        PackageKind::Formula,
        "node@20",
        "20.11.0",
        "20.18.1",
    );
    pinned.pinned = true;
    let wrangler = package(
        SourceId::Npm,
        PackageKind::NpmGlobal,
        "wrangler",
        "4.81.0",
        "4.151.1",
    );
    let skills: Vec<String> = (0..72).map(|i| format!("skill-{i}")).collect();
    vec![
        AppEvent::Updates(vec![
            SourceReport {
                source: SourceId::Homebrew,
                availability: Availability::Available("7.0.9".into()),
                check: check(vec![
                    package(
                        SourceId::Homebrew,
                        PackageKind::Formula,
                        "wget",
                        "1.21.4",
                        "1.24.5",
                    ),
                    package(
                        SourceId::Homebrew,
                        PackageKind::Cask,
                        "firefox",
                        "118.0",
                        "119.0",
                    ),
                    pinned,
                ]),
            },
            SourceReport {
                source: SourceId::Npm,
                availability: Availability::Available("11.6.2".into()),
                check: check(vec![
                    package(
                        SourceId::Npm,
                        PackageKind::NpmGlobal,
                        "@sap/cds-dk",
                        "9.7.2",
                        "10.1.0",
                    ),
                    package(
                        SourceId::Npm,
                        PackageKind::NpmGlobal,
                        "@ui5/cli",
                        "4.0.49",
                        "4.0.71",
                    ),
                    wrangler.clone(),
                ]),
            },
            SourceReport {
                source: SourceId::Skills,
                availability: Availability::Available("1.7.2".into()),
                check: Some(Ok(SourceCheck {
                    candidates: Vec::new(),
                    installed: skills,
                    updates_known: false,
                })),
            },
        ]),
        AppEvent::Job(JobUpdate {
            id: JobId(1),
            target: InstallTarget::Package(wrangler),
            state: JobState::Running,
        }),
    ]
}

/// Updates tab with the confirmation dialog for a major npm update.
fn render_confirm(
    out: &std::path::Path,
    cols: u16,
    rows: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let settings = UiSettings {
        language: Language::De,
        ..UiSettings::default()
    };
    let mut model = Model::new(settings);
    update(&mut model, Msg::Tick(Instant::now()));
    for event in sample_updates() {
        update(&mut model, Msg::App(event));
    }
    let press = |code| Msg::Key(KeyEvent::new(code, KeyModifiers::NONE));
    for code in [
        KeyCode::Tab,
        KeyCode::Tab,
        KeyCode::Down,
        KeyCode::Down,
        KeyCode::Down,
        KeyCode::Char('u'),
    ] {
        update(&mut model, press(code));
    }
    let mut terminal = Terminal::new(TestBackend::new(cols, rows))?;
    terminal.draw(|frame| view(&model, frame))?;
    let name = "03-updates-confirm-de-dark.svg";
    std::fs::write(
        out.join(name),
        to_svg(terminal.backend().buffer(), Appearance::Dark),
    )?;
    println!("{}", out.join(name).display());
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
