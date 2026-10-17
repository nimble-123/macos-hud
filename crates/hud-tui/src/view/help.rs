//! Help overlay: the keys of the current tab, then the global ones.

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::i18n::t;
use crate::{Model, Tab};

const KEY_WIDTH: usize = 18;

/// Keys and what they do, as message ids, for `tab`.
fn tab_keys(tab: Tab) -> &'static [(&'static str, &'static str)] {
    match tab {
        Tab::Overview => &[("key-r", "help-sync")],
        Tab::Workspaces => &[
            ("key-arrows", "help-select"),
            ("key-f", "help-fast-forward"),
            ("key-r", "help-sync"),
        ],
        Tab::Updates => &[
            ("key-arrows", "help-select"),
            ("help-key-install", "help-install"),
            ("key-r", "help-check"),
        ],
        Tab::Ai => &[
            ("help-key-period", "help-period"),
            ("key-t", "help-telemetry"),
            ("key-r", "help-sync"),
        ],
        Tab::Log => &[
            ("key-arrows", "help-select"),
            ("key-slash", "help-search"),
            ("key-f", "help-filter"),
            ("key-esc", "help-clear-search"),
            ("key-r", "help-reload-log"),
        ],
        Tab::Settings => &[
            ("key-arrows", "help-select"),
            ("help-key-change", "help-change"),
            ("key-a", "help-add-workspace"),
            ("key-d", "help-remove-workspace"),
            ("key-r", "help-reload-config"),
        ],
    }
}

const GLOBAL_KEYS: [(&str, &str); 4] = [
    ("help-key-tabs", "help-tabs"),
    ("help-key-help", "help-help"),
    ("key-quit", "help-quit"),
    ("help-key-ctrl-c", "help-quit-now"),
];

pub(super) fn render(model: &Model, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let key_style = Style::new().fg(theme.focus).add_modifier(Modifier::BOLD);
    let row = |key: &str, label: &str| {
        Line::from(vec![
            Span::styled(format!("  {:<KEY_WIDTH$}", t(language, key)), key_style),
            Span::styled(t(language, label), Style::new().fg(theme.ink)),
        ])
    };
    let heading = |key: &str| {
        Line::styled(
            format!("  {}", t(language, key)),
            Style::new().fg(theme.soft).add_modifier(Modifier::BOLD),
        )
    };
    let keys = tab_keys(model.tab());
    let mut lines = vec![Line::raw(""), heading(super::tab_message(model.tab()))];
    lines.extend(keys.iter().map(|(key, label)| row(key, label)));
    lines.push(Line::raw(""));
    lines.push(heading("help-everywhere"));
    lines.extend(GLOBAL_KEYS.iter().map(|(key, label)| row(key, label)));
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!("  {}", t(language, "help-close")),
        Style::new().fg(theme.soft),
    ));

    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let [dialog] = Layout::horizontal([Constraint::Length(88)])
        .flex(Flex::Center)
        .areas(area);
    let [dialog] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(dialog);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.focus))
        .title(Span::styled(
            format!(" {} ", t(language, "help-title")),
            key_style,
        ));
    frame.render_widget(Clear, dialog);
    frame.render_widget(Paragraph::new(lines).block(block), dialog);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tab_documents_at_least_one_key() {
        for tab in Tab::ALL {
            assert!(!tab_keys(tab).is_empty(), "{tab:?}");
        }
    }
}
