//! Renders every tab into an SVG that mimics a terminal window.
//!
//! Used by `scripts/tui-screenshots.sh` to produce PNGs for reviews. The grid
//! defaults to 115 × 50 cells, roughly a 1800 × 1800 px Warp window on Retina.
//!
//! Usage: `cargo run -p hud-tui --example screenshot -- <out-dir> [cols] [rows]`

use std::fmt::Write as _;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use hud_tui::{Model, Msg, Tab, update, view};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

const CELL_W: f64 = 15.65;
const CELL_H: f64 = 33.8;
const FONT_SIZE: f64 = 26.0;
const PAD: f64 = 24.0;
const BG: &str = "#161B22";
const FG: &str = "#E6E8E3";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().ok_or("missing <out-dir>")?);
    let cols: u16 = args.next().map_or(Ok(115), |v| v.parse())?;
    let rows: u16 = args.next().map_or(Ok(50), |v| v.parse())?;
    std::fs::create_dir_all(&out)?;

    let mut model = Model::default();
    for tab in Tab::ALL {
        let mut terminal = Terminal::new(TestBackend::new(cols, rows))?;
        terminal.draw(|frame| view(&model, frame))?;
        let name = format!(
            "{:02}-{}.svg",
            tab.index() + 1,
            format!("{tab:?}").to_lowercase()
        );
        std::fs::write(out.join(&name), to_svg(terminal.backend().buffer()))?;
        println!("{}", out.join(name).display());
        update(
            &mut model,
            Msg::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        );
    }
    Ok(())
}

fn hex(color: Color, default: &str) -> String {
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
        Color::White => FG.into(),
        other => format!("{other}"),
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn to_svg(buffer: &Buffer) -> String {
    let area = buffer.area;
    let width = PAD * 2.0 + f64::from(area.width) * CELL_W;
    let height = PAD * 2.0 + f64::from(area.height) * CELL_H;
    let mut svg = String::new();
    let _ = write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.0} {height:.0}"><rect width="100%" height="100%" fill="{BG}"/><g font-family="Hack Nerd Font Mono, Hack, Menlo, monospace" font-size="{FONT_SIZE}">"#
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
                    hex(cell.bg, BG)
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
                hex(cell.fg, FG),
                escape(symbol)
            );
            if cell.modifier.contains(Modifier::UNDERLINED) {
                let _ = write!(
                    svg,
                    r#"<rect x="{left:.2}" y="{:.2}" width="{CELL_W}" height="2" fill="{}"/>"#,
                    base + 5.0,
                    hex(cell.fg, FG)
                );
            }
        }
    }
    svg.push_str("</g></svg>\n");
    svg
}
