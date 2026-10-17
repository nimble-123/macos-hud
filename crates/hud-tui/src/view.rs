//! Rendering.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::theme;
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

    render_tabs(model.tab(), frame, tabs);
    render_body(model.tab(), frame, body);
    render_footer(frame, footer);
}

fn tab_title(tab: Tab) -> &'static str {
    match tab {
        Tab::Overview => "Overview",
        Tab::Workspaces => "Workspaces",
        Tab::Updates => "Updates",
        Tab::Ai => "AI",
        Tab::Log => "Log",
        Tab::Settings => "Settings",
    }
}

fn render_tabs(active: Tab, frame: &mut Frame, area: Rect) {
    let mut spans = Vec::with_capacity(Tab::ALL.len() * 2);
    for tab in Tab::ALL {
        let style = if tab == active {
            Style::new()
                .fg(theme::FOCUS)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::new().fg(theme::MUTED)
        };
        spans.push(Span::styled(tab_title(tab), style));
        spans.push(Span::raw("    "));
    }
    let block = Block::new()
        .borders(Borders::BOTTOM)
        .border_style(Style::new().fg(theme::RULE));
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn render_body(active: Tab, frame: &mut Frame, area: Rect) {
    let text = format!("{} is not built yet.", tab_title(active));
    let paragraph = Paragraph::new(Line::styled(text, Style::new().fg(theme::SOFT)));
    frame.render_widget(paragraph, area.inner(ratatui::layout::Margin::new(0, 1)));
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let hints = [("tab", "switch"), ("q", "quit")];
    let mut spans = Vec::with_capacity(hints.len() * 3);
    for (key, label) in hints {
        spans.push(Span::styled(
            key,
            Style::new().fg(theme::FOCUS).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {label}   "),
            Style::new().fg(theme::SOFT),
        ));
    }
    let block = Block::new()
        .borders(Borders::TOP)
        .border_style(Style::new().fg(theme::RULE));
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::{Msg, update};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn render(model: &Model) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).expect("BUG: test backend");
        terminal
            .draw(|frame| view(model, frame))
            .expect("BUG: test draw");
        terminal
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
}
