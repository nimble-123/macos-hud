//! Settings tab: presentation, workspaces and opt-ins, plus the remove dialog.

use hud_application::settings::{
    AutoInstallKind, IconPreference, LanguagePreference, OverviewInsight, Settings, ThemePreference,
};
use hud_domain::WorkspacePath;
use hud_domain::usage::Provider;
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::Model;
use crate::i18n::{t, t_args};
use crate::icons::{Status, status_glyph};
use crate::settings::{Appearance, Language};
use crate::settings_tab::{Notice, Row, rows};

/// The label column shrinks in narrow windows down to this width.
const MIN_LABEL_WIDTH: usize = 20;
const MAX_LABEL_WIDTH: usize = 40;

/// Label column for an area `width` wide; the values need about 30 columns.
fn label_width(width: u16) -> usize {
    usize::from(width)
        .saturating_sub(34)
        .clamp(MIN_LABEL_WIDTH, MAX_LABEL_WIDTH)
}

pub(super) fn render(model: &Model, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let Some(preferences) = &model.preferences else {
        frame.render_widget(
            Paragraph::new(Line::styled(
                t(language, "settings-loading"),
                Style::new().fg(theme.soft),
            )),
            area,
        );
        return;
    };
    let tab = &model.settings_tab;
    let (lines, map) = build(model, preferences, area.width);
    let offset = super::follow(area, selected_line(&map, tab.selected));
    frame.render_widget(Paragraph::new(lines).scroll((offset, 0)), area);
    if let Some(path) = &tab.remove {
        render_remove(model, path, frame, area);
    }
}

/// Settings row under line `y` of the tab's area.
pub(super) fn row_at(model: &Model, area: Rect, y: u16) -> Option<usize> {
    let preferences = model.preferences.as_ref()?;
    let map = build(model, preferences, area.width).1;
    let offset = super::follow(area, selected_line(&map, model.settings_tab.selected));
    map.get(usize::from(y + offset)).copied().flatten()
}

/// Line that shows row `selected`; the view keeps it on screen.
fn selected_line(map: &[Option<usize>], selected: usize) -> usize {
    map.iter()
        .position(|row| *row == Some(selected))
        .unwrap_or(0)
}

/// Lines and, per line, the settings row it shows.
fn build<'a>(
    model: &Model,
    preferences: &Settings,
    width: u16,
) -> (Vec<Line<'a>>, Vec<Option<usize>>) {
    let labels = label_width(width);
    let tab = &model.settings_tab;
    let mut lines = Vec::new();
    let mut map = Vec::new();
    let mut section = None;
    for (index, row) in rows(preferences).iter().enumerate() {
        let current = match row {
            Row::Language(_) | Row::Theme(_) | Row::Icons(_) => "settings-display",
            Row::Workspace { .. } | Row::AddWorkspace => "settings-workspaces",
            Row::AutoInstall { .. } => "settings-auto-install",
            Row::AiSource { .. } => "settings-ai-sources",
            Row::OverviewInsight { .. } => "settings-overview",
        };
        if section != Some(current) {
            if section.is_some() {
                lines.push(Line::raw(""));
                map.push(None);
            }
            let column = (current == "settings-workspaces").then_some("settings-auto-ff");
            lines.push(heading(model, current, column, labels));
            map.push(None);
            section = Some(current);
        }
        if *row == Row::AddWorkspace
            && let Some(input) = &tab.input
        {
            lines.push(input_line(model, input));
        } else {
            lines.push(row_line(model, row, index == tab.selected, labels));
        }
        map.push(Some(index));
    }
    if let Some(notice) = &tab.notice {
        lines.push(Line::raw(""));
        lines.push(notice_line(model, notice));
    }
    (lines, map)
}

fn heading<'a>(model: &Model, title: &str, column: Option<&str>, labels: usize) -> Line<'a> {
    let language = model.settings.language;
    let theme = &model.theme;
    let mut spans = vec![Span::styled(
        fit(&t(language, title), labels + 2),
        Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
    )];
    if let Some(column) = column {
        spans.push(Span::styled(
            t(language, column),
            Style::new().fg(theme.soft),
        ));
    }
    Line::from(spans)
}

fn row_line<'a>(model: &Model, row: &Row, selected: bool, labels: usize) -> Line<'a> {
    let language = model.settings.language;
    let theme = &model.theme;
    let label_style = if selected {
        Style::new().fg(theme.focus).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme.ink)
    };
    let (label, value) = match row {
        Row::Language(preference) => (
            t(language, "settings-language"),
            language_value(model, *preference),
        ),
        Row::Theme(preference) => (
            t(language, "settings-theme"),
            theme_value(model, *preference),
        ),
        Row::Icons(preference) => (
            t(language, "settings-icons"),
            icons_value(model, *preference),
        ),
        Row::Workspace {
            path,
            auto_fast_forward,
        } => (path.to_string(), switch(model, *auto_fast_forward)),
        Row::AddWorkspace => (
            format!("+ {}", t(language, "settings-add-workspace")),
            Vec::new(),
        ),
        Row::AutoInstall { kind, enabled } => (
            t(
                language,
                match kind {
                    AutoInstallKind::BrewFormulae => "opt-brew-formulae",
                    AutoInstallKind::BrewCasks => "opt-brew-casks",
                    AutoInstallKind::Npm => "opt-npm",
                },
            ),
            switch(model, *enabled),
        ),
        Row::AiSource { provider, enabled } => (
            t(
                language,
                match provider {
                    Provider::Claude => "ai-claude",
                    Provider::Codex => "ai-codex",
                },
            ),
            switch(model, *enabled),
        ),
        Row::OverviewInsight { insight, enabled } => (
            t(
                language,
                match insight {
                    OverviewInsight::Unpushed => "insight-unpushed",
                    OverviewInsight::UpdateAttention => "insight-update-attention",
                    OverviewInsight::Problems => "insight-problems",
                    OverviewInsight::PlanLimits => "insight-plan-limits",
                },
            ),
            switch(model, *enabled),
        ),
    };
    let mut spans = vec![
        Span::styled(
            if selected { "▌ " } else { "  " },
            Style::new().fg(theme.focus),
        ),
        Span::styled(fit(&label, labels), label_style),
    ];
    spans.extend(value);
    Line::from(spans)
}

/// A choice, with what `auto` currently means in brackets.
fn choice<'a>(model: &Model, text: String, detected: Option<String>) -> Vec<Span<'a>> {
    let theme = &model.theme;
    let mut spans = vec![Span::styled(text, Style::new().fg(theme.ink))];
    if let Some(detected) = detected {
        spans.push(Span::styled(
            format!(" ({detected})"),
            Style::new().fg(theme.soft),
        ));
    }
    spans
}

fn language_value<'a>(model: &Model, preference: LanguagePreference) -> Vec<Span<'a>> {
    let language = model.settings.language;
    let name = |value: Language| {
        t(
            language,
            match value {
                Language::De => "lang-de",
                Language::En => "lang-en",
            },
        )
    };
    match preference {
        LanguagePreference::Auto => choice(
            model,
            t(language, "pref-auto"),
            Some(name(model.system.language)),
        ),
        LanguagePreference::De => choice(model, name(Language::De), None),
        LanguagePreference::En => choice(model, name(Language::En), None),
    }
}

fn theme_value<'a>(model: &Model, preference: ThemePreference) -> Vec<Span<'a>> {
    let language = model.settings.language;
    let name = |value: Appearance| {
        t(
            language,
            match value {
                Appearance::Dark => "theme-dark",
                Appearance::Light => "theme-light",
            },
        )
    };
    match preference {
        ThemePreference::Auto => choice(
            model,
            t(language, "pref-auto"),
            Some(name(model.system.appearance)),
        ),
        ThemePreference::Dark => choice(model, name(Appearance::Dark), None),
        ThemePreference::Light => choice(model, name(Appearance::Light), None),
    }
}

fn icons_value<'a>(model: &Model, preference: IconPreference) -> Vec<Span<'a>> {
    let key = match preference {
        IconPreference::Unicode => "setup-icons-unicode",
        IconPreference::Nerd => "setup-icons-nerd",
        IconPreference::Ascii => "setup-icons-ascii",
    };
    choice(model, t(model.settings.language, key), None)
}

fn switch<'a>(model: &Model, on: bool) -> Vec<Span<'a>> {
    let language = model.settings.language;
    let theme = &model.theme;
    let (status, text, color) = if on {
        (Status::Ok, t(language, "toggle-on"), theme.ink)
    } else {
        (Status::Neutral, t(language, "toggle-off"), theme.muted)
    };
    vec![Span::styled(
        format!("{} {text}", status_glyph(model.settings.icons, status)),
        Style::new().fg(color),
    )]
}

fn input_line<'a>(model: &Model, input: &str) -> Line<'a> {
    let theme = &model.theme;
    Line::from(vec![
        Span::styled("▌ ", Style::new().fg(theme.focus)),
        Span::styled(
            format!("{} ", t(model.settings.language, "setup-add-path")),
            Style::new().fg(theme.soft),
        ),
        Span::styled(
            input.to_owned(),
            Style::new().fg(theme.focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("█", Style::new().fg(theme.focus)),
    ])
}

fn notice_line<'a>(model: &Model, notice: &Notice) -> Line<'a> {
    let language = model.settings.language;
    let (status, text) = match notice {
        Notice::InvalidPath => (Status::Caution, t(language, "setup-path-invalid")),
        Notice::Rejected(reason) => (
            Status::Warning,
            t_args(language, "settings-rejected", &[("reason", reason)]),
        ),
    };
    Line::styled(
        format!("  {} {text}", status_glyph(model.settings.icons, status)),
        Style::new().fg(model.theme.status(status)),
    )
}

fn render_remove(model: &Model, path: &WorkspacePath, frame: &mut Frame, area: Rect) {
    let language = model.settings.language;
    let theme = &model.theme;
    let [dialog] = Layout::horizontal([Constraint::Length(72)])
        .flex(Flex::Center)
        .areas(area);
    let [dialog] = Layout::vertical([Constraint::Length(9)])
        .flex(Flex::Center)
        .areas(dialog);
    let key = Style::new().fg(theme.focus).add_modifier(Modifier::BOLD);
    let lines = vec![
        Line::raw(""),
        Line::styled(
            format!("  {path}"),
            Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::styled(
            format!("  {}", t(language, "settings-remove-body")),
            Style::new().fg(theme.soft),
        ),
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!("  {} ", t(language, "key-enter")), key),
            Span::styled(
                format!("{}   ", t(language, "confirm-remove")),
                Style::new().fg(theme.soft),
            ),
            Span::styled(format!("{} ", t(language, "key-esc")), key),
            Span::styled(t(language, "confirm-cancel"), Style::new().fg(theme.soft)),
        ]),
    ];
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.focus))
        .title(Span::styled(
            format!(" {} ", t(language, "settings-remove-title")),
            key,
        ));
    frame.render_widget(Clear, dialog);
    frame.render_widget(Paragraph::new(lines).block(block), dialog);
}

fn fit(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count < width {
        format!("{text}{}", " ".repeat(width - count))
    } else {
        let mut cut: String = text.chars().take(width.saturating_sub(2)).collect();
        cut.push_str("… ");
        cut
    }
}
