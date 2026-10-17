//! Activity per day over many weeks, shown as a contribution-style calendar.

use jiff::Timestamp;
use jiff::tz::TimeZone;

/// Events per day over many weeks, for a contribution-style calendar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityCalendar {
    /// First day shown; always a Monday, so columns are whole weeks.
    pub first_day: jiff::civil::Date,
    /// Events per day from `first_day` to today.
    pub days: Vec<u64>,
}

/// Weeks the calendar covers at most.
pub const CALENDAR_WEEKS: i32 = 53;

/// Monday of the week `CALENDAR_WEEKS - 1` weeks before the week of `today`.
#[must_use]
pub fn calendar_start(today: jiff::civil::Date) -> jiff::civil::Date {
    use jiff::ToSpan;
    let back_to_monday = i32::from(today.weekday().to_monday_zero_offset());
    today
        .checked_sub((back_to_monday + (CALENDAR_WEEKS - 1) * 7).days())
        .unwrap_or(today)
}

/// Counts `times` per day in `zone` from `calendar_start` to `today`.
#[must_use]
pub fn calendar(
    times: &[Timestamp],
    today: jiff::civil::Date,
    zone: &TimeZone,
) -> ActivityCalendar {
    let first_day = calendar_start(today);
    let length =
        usize::try_from(first_day.until(today).map_or(0, |span| span.get_days()) + 1).unwrap_or(0);
    let mut days = vec![0u64; length];
    for time in times {
        let date = time.to_zoned(zone.clone()).date();
        if let Ok(span) = first_day.until(date)
            && let Ok(index) = usize::try_from(span.get_days())
            && let Some(day) = days.get_mut(index)
        {
            *day += 1;
        }
    }
    ActivityCalendar { first_day, days }
}
