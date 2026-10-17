//! Rendering.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

mod workspaces;

use crate::i18n::{t, t_args};
use crate::settings::Language;
use crate::theme::Theme;
use crate::{Model, Tab};

/// Renders the model into a frame. Pure: reads the model, writes the frame.
pub fn view(model: &Model, frame: &mut Frame) {
    let [tabs, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(2),
    ])
    .horizontal_margin(2)
    .areas(frame.area());

    let language = model.settings.language;
    render_tabs(model.tab(), language, &model.theme, frame, tabs);
    let body = body.inner(Margin::new(0, 1));
    match model.tab() {
        Tab::Workspaces => workspaces::render(model, frame, body),
        tab => render_placeholder(tab, language, &model.theme, frame, body),
    }
    render_footer(model, frame, footer);
}

fn tab_message(tab: Tab) -> &'static str {
    match tab {
        Tab::Overview => "tab-overview",
        Tab::Workspaces => "tab-workspaces",
        Tab::Updates => "tab-updates",
        Tab::Ai => "tab-ai",
        Tab::Log => "tab-log",
        Tab::Settings => "tab-settings",
    }
}

fn render_tabs(active: Tab, language: Language, theme: &Theme, frame: &mut Frame, area: Rect) {
    let mut spans = Vec::with_capacity(Tab::ALL.len() * 2);
    for tab in Tab::ALL {
        let style = if tab == active {
            Style::new()
                .fg(theme.focus)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::new().fg(theme.muted)
        };
        spans.push(Span::styled(t(language, tab_message(tab)), style));
        spans.push(Span::raw("    "));
    }
    let block = Block::new()
        .borders(Borders::BOTTOM)
        .border_style(Style::new().fg(theme.rule));
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn render_placeholder(
    active: Tab,
    language: Language,
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
) {
    let title = t(language, tab_message(active));
    let text = t_args(language, "tab-placeholder", &[("tab", &title)]);
    frame.render_widget(
        Paragraph::new(Line::styled(text, Style::new().fg(theme.soft))),
        area,
    );
}

fn render_footer(model: &Model, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let mut hints = Vec::with_capacity(5);
    if model.tab() == Tab::Workspaces {
        hints.extend([
            ("key-arrows", "hint-select"),
            ("key-f", "hint-fast-forward"),
        ]);
    }
    hints.extend([
        ("key-r", "hint-refresh"),
        ("key-tab", "hint-switch"),
        ("key-quit", "hint-quit"),
    ]);
    let mut spans = Vec::with_capacity(hints.len() * 2 + 1);
    for (key, label) in hints {
        spans.push(Span::styled(
            t(language, key),
            Style::new().fg(theme.focus).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {}   ", t(language, label)),
            Style::new().fg(theme.soft),
        ));
    }
    if model.syncing {
        spans.push(Span::styled(
            format!("   {}", t(language, "syncing")),
            Style::new().fg(theme.status(crate::icons::Status::Advisory)),
        ));
    }
    let block = Block::new()
        .borders(Borders::TOP)
        .border_style(Style::new().fg(theme.rule));
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::settings::UiSettings;
    use crate::{Msg, update};

    fn render(model: &Model) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).expect("BUG: test backend");
        terminal
            .draw(|frame| view(model, frame))
            .expect("BUG: test draw");
        terminal
    }

    fn german() -> Model {
        Model::new(UiSettings {
            language: Language::De,
            ..UiSettings::default()
        })
    }

    #[test]
    fn overview_is_active_on_start() {
        insta::assert_snapshot!(render(&Model::default()).backend());
    }

    #[test]
    fn tab_key_activates_workspaces() {
        let mut model = Model::default();
        update(
            &mut model,
            Msg::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        );
        insta::assert_snapshot!(render(&model).backend());
    }

    #[test]
    fn german_overview_uses_german_texts() {
        insta::assert_snapshot!(render(&german()).backend());
    }
}
