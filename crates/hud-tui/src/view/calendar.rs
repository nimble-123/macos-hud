//! AI activity as a contribution-style calendar: one column per week.

use hud_domain::activity::ActivityCalendar;
use jiff::ToSpan;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::Model;
use crate::i18n::{t, t_args};
use crate::settings::Language;
use crate::theme::Activity;

/// Weekday labels take this many columns.
const LABEL_WIDTH: usize = 5;
/// The labels repeat on the right, after the gap of the last week.
const RIGHT_LABEL_WIDTH: usize = 3;
/// Each week is a square and a gap.
const CELL_WIDTH: usize = 2;

/// The calendar for `width` columns, or nothing when there was no activity.
pub(super) fn lines<'a>(
    model: &Model,
    activity: Activity,
    calendar: &ActivityCalendar,
    width: usize,
) -> Vec<Line<'a>> {
    let weeks_total = calendar.days.len().div_ceil(7);
    let weeks = weeks_total.min(width.saturating_sub(LABEL_WIDTH + RIGHT_LABEL_WIDTH) / CELL_WIDTH);
    let skip = weeks_total - weeks;
    let shown = &calendar.days[(skip * 7).min(calendar.days.len())..];
    if weeks == 0 || shown.iter().all(|&day| day == 0) {
        return Vec::new();
    }
    let language = model.settings.language;
    let theme = &model.theme;
    let levels = Levels::new(shown);
    let first = calendar
        .first_day
        .checked_add((7 * skip_i64(skip)).days())
        .unwrap_or(calendar.first_day);

    let mut lines = vec![Line::from(vec![
        Span::styled(
            t(language, title_key(activity)),
            Style::new().fg(theme.ink).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "   {}",
                t_args(language, "calendar-weeks", &[("count", &weeks.to_string())])
            ),
            Style::new().fg(theme.soft),
        ),
    ])];
    lines.push(Line::styled(
        format!(
            "{}{}",
            " ".repeat(LABEL_WIDTH),
            month_row(first, weeks, language)
        ),
        Style::new().fg(theme.soft),
    ));
    for weekday in 0..7 {
        let label = match weekday {
            0 => t(language, "calendar-mon"),
            2 => t(language, "calendar-wed"),
            4 => t(language, "calendar-fri"),
            _ => String::new(),
        };
        let mut spans = vec![Span::styled(
            fit(&label, LABEL_WIDTH),
            Style::new().fg(theme.soft),
        )];
        // The newest weeks are on the right, far from the left labels.
        let right = Span::styled(label, Style::new().fg(theme.soft));
        for week in 0..weeks {
            // Days after today in the current week stay empty.
            let cell = match shown.get(week * 7 + weekday) {
                Some(&count) => {
                    let level = levels.of(count);
                    Span::styled(
                        format!("{} ", glyph(model, level)),
                        Style::new().fg(theme.heat(activity, level)),
                    )
                }
                None => Span::raw("  "),
            };
            spans.push(cell);
        }
        spans.push(right);
        lines.push(Line::from(spans));
    }
    lines.push(summary(model, activity, shown));
    lines
}

fn skip_i64(weeks: usize) -> i64 {
    i64::try_from(weeks).unwrap_or(0)
}

/// Squares in colour; without colour the level shows in the glyph.
fn glyph(model: &Model, level: u8) -> &'static str {
    if model.theme.colorful() {
        "■"
    } else {
        ["·", "░", "▒", "▓", "█"][usize::from(level.min(4))]
    }
}

/// Short month names over the first week of each month.
fn month_row(first: jiff::civil::Date, weeks: usize, language: Language) -> String {
    let mut row = vec![' '; weeks * CELL_WIDTH];
    let mut free_from = 0;
    let mut previous = None;
    for week in 0..weeks {
        let Ok(monday) = first.checked_add((7 * skip_i64(week)).days()) else {
            break;
        };
        // The label belongs to the week in which the month's first day lies.
        let sunday = monday.checked_add(6.days()).unwrap_or(monday);
        let month = sunday.month();
        if previous == Some(month) {
            continue;
        }
        previous = Some(month);
        let at = week * CELL_WIDTH;
        let name = month_name(month, language);
        if at < free_from || at + name.chars().count() > row.len() {
            continue;
        }
        for (offset, c) in name.chars().enumerate() {
            row[at + offset] = c;
        }
        free_from = at + name.chars().count() + 1;
    }
    row.into_iter().collect()
}

fn month_name(month: i8, language: Language) -> &'static str {
    const DE: [&str; 12] = [
        "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
    ];
    const EN: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let index = usize::try_from(month - 1).unwrap_or(0).min(11);
    match language {
        Language::De => DE[index],
        Language::En => EN[index],
    }
}

/// GitHub-style levels: 0 for no activity, then the quartiles of active days.
struct Levels {
    bounds: [u64; 3],
}

impl Levels {
    fn new(days: &[u64]) -> Self {
        let mut active: Vec<u64> = days.iter().copied().filter(|&day| day > 0).collect();
        active.sort_unstable();
        let at = |quarter: usize| {
            active
                .get((active.len() * quarter / 4).min(active.len().saturating_sub(1)))
                .copied()
                .unwrap_or(0)
        };
        Self {
            bounds: [at(1), at(2), at(3)],
        }
    }

    fn of(&self, count: u64) -> u8 {
        match count {
            0 => 0,
            c if c <= self.bounds[0] => 1,
            c if c <= self.bounds[1] => 2,
            c if c <= self.bounds[2] => 3,
            _ => 4,
        }
    }
}

/// Totals, active days, longest streak and the legend.
fn summary<'a>(model: &Model, activity: Activity, days: &[u64]) -> Line<'a> {
    let language = model.settings.language;
    let theme = &model.theme;
    let total: u64 = days.iter().sum();
    let active = days.iter().filter(|&&day| day > 0).count();
    let streak = days
        .iter()
        .fold((0usize, 0usize), |(best, current), &day| {
            let current = if day > 0 { current + 1 } else { 0 };
            (best.max(current), current)
        })
        .0;
    let mut spans = vec![
        Span::raw(" ".repeat(LABEL_WIDTH)),
        Span::styled(
            t_args(
                language,
                summary_key(activity),
                &[
                    ("answers", &super::ai::count(total, language)),
                    ("days", &active.to_string()),
                    ("streak", &streak.to_string()),
                ],
            ),
            Style::new().fg(theme.muted),
        ),
        Span::styled(
            format!("      {} ", t(language, "calendar-less")),
            Style::new().fg(theme.soft),
        ),
    ];
    for level in 0..=4 {
        spans.push(Span::styled(
            format!("{} ", glyph(model, level)),
            Style::new().fg(theme.heat(activity, level)),
        ));
    }
    spans.push(Span::styled(
        t(language, "calendar-more"),
        Style::new().fg(theme.soft),
    ));
    Line::from(spans)
}

fn fit(text: &str, width: usize) -> String {
    format!("{text:<width$}")
}

fn title_key(activity: Activity) -> &'static str {
    match activity {
        Activity::Ai => "calendar-title",
        Activity::Git => "calendar-git-title",
    }
}

fn summary_key(activity: Activity) -> &'static str {
    match activity {
        Activity::Ai => "calendar-summary",
        Activity::Git => "calendar-git-summary",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_split_active_days_into_quartiles() {
        let levels = Levels::new(&[0, 1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(levels.of(0), 0);
        assert_eq!(levels.of(1), 1);
        assert_eq!(levels.of(8), 4);
    }

    fn rendered(width: usize) -> Vec<String> {
        let model = Model::default();
        let calendar = ActivityCalendar {
            first_day: hud_domain::activity::calendar_start(jiff::civil::date(2026, 10, 17)),
            days: vec![1; 52 * 7 + 6],
        };
        lines(&model, Activity::Ai, &calendar, width)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn weekdays_are_labelled_on_both_sides() {
        let rows = rendered(115);
        // Heading, months, then Monday.
        assert!(rows[2].starts_with("Mon"), "{:?}", rows[2]);
        assert!(rows[2].ends_with("Mon"), "{:?}", rows[2]);
        assert!(rows[3].trim().ends_with('■'), "Tuesday has no label");
    }

    #[test]
    fn the_right_labels_never_push_past_the_width() {
        for width in [40, 80, 115, 200] {
            // The seven weekday rows follow heading and months.
            let longest = rendered(width)[2..9]
                .iter()
                .map(|row| row.chars().count())
                .max()
                .unwrap_or(0);
            assert!(longest <= width, "{longest} > {width}");
        }
    }

    #[test]
    fn months_are_labelled_at_their_first_week() {
        // 2026-09-28 is a Monday; October begins in that week.
        let row = month_row(jiff::civil::date(2026, 9, 28), 6, Language::De);
        assert!(row.starts_with("Okt"), "{row:?}");
        assert!(row.contains("Nov"), "{row:?}");
    }
}
