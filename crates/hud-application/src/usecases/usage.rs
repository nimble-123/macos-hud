//! Reading AI usage from local logs and summarising it per period.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use hud_domain::usage::{
    Accuracy, CostEstimate, Measured, ModelId, PRICES_AS_OF, Period, PriceTable, Provider,
    TelemetryTotals, TokenUsage, UsageRecord, context_window,
};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use tokio::sync::{mpsc, watch};
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::event::AppEvent;
use crate::ports::store::Clock;
use crate::ports::telemetry::TelemetryStore;
use crate::ports::usage::{UsageError, UsageSource, UsageStore};
use crate::settings::Settings;

/// Usage of one model within a period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelUsage {
    /// Provider.
    pub provider: Provider,
    /// Model.
    pub model: ModelId,
    /// Responses.
    pub responses: u64,
    /// Sessions that used this model.
    pub sessions: u64,
    /// Tokens.
    pub tokens: TokenUsage,
    /// Estimated cost; `None` when the model has no known price.
    pub cost: Option<CostEstimate>,
}

/// Usage within one period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeriodUsage {
    /// Period.
    pub period: Period,
    /// Per model, most expensive first.
    pub models: Vec<ModelUsage>,
    /// Distinct sessions.
    pub sessions: Measured<u64>,
    /// All tokens.
    pub tokens: Measured<TokenUsage>,
    /// Estimated cost of everything with a known price.
    pub cost: Measured<CostEstimate>,
    /// What Claude Code's telemetry reported; always partial, because it
    /// only arrives while hud runs (ADR 0011).
    pub telemetry: Measured<TelemetryTotals>,
    /// Shares and activity within the period.
    pub kpis: PeriodKpis,
}

/// State of one usage source after the last read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceState {
    /// Provider.
    pub provider: Provider,
    /// Error of the last read, if any.
    pub error: Option<UsageError>,
    /// Lines that looked like usage but could not be parsed.
    pub unreadable: u64,
    /// Log files known after the read.
    pub files: u64,
    /// How long the read took, milliseconds.
    pub read_ms: u64,
}

/// The last answer of the main conversation and how full its context was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextUse {
    /// Model.
    pub model: ModelId,
    /// Project.
    pub project: Option<String>,
    /// Tokens the model read for it.
    pub prompt: u64,
    /// Context window of the model.
    pub window: u64,
    /// When it was answered.
    pub at: Timestamp,
    /// Time from the answer to the report.
    pub idle: Duration,
}

/// Shares and activity of one period. Shares are in per mille; `None`
/// means there was nothing to divide by.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PeriodKpis {
    /// Cache reads among all prompt tokens.
    pub cache_hit: Option<u16>,
    /// Models by share of fresh tokens (all but cache reads), largest first.
    pub models: Vec<(ModelId, u16)>,
    /// Subagents' share of fresh tokens.
    pub subagents: Option<u16>,
    /// Thinking's share of output.
    pub thinking: Option<u16>,
    /// Responses per bucket, oldest first: hours for today, days otherwise.
    pub activity: Vec<u64>,
    /// Responses.
    pub responses: u64,
    /// Tool calls.
    pub tool_calls: u64,
    /// Projects by share of fresh tokens, largest first.
    pub projects: Vec<(String, u16)>,
}

/// Computes shares and activity for the records in `[from, to)`.
#[must_use]
pub fn period_kpis(
    records: &[UsageRecord],
    (from, to): (Timestamp, Timestamp),
    buckets: &[(Timestamp, Timestamp)],
    tool_calls: u64,
) -> PeriodKpis {
    let inside: Vec<&UsageRecord> = records
        .iter()
        .filter(|r| r.at >= from && r.at < to)
        .collect();
    let mut totals = TokenUsage::default();
    let (mut subagent, mut thinking) = (0u64, 0u64);
    let mut models: BTreeMap<ModelId, u64> = BTreeMap::new();
    let mut projects: BTreeMap<String, u64> = BTreeMap::new();
    for record in &inside {
        totals += record.tokens;
        thinking += record.thinking;
        let fresh = record.tokens.fresh();
        if record.subagent {
            subagent += fresh;
        }
        *models.entry(record.model.clone()).or_default() += fresh;
        if let Some(project) = &record.project {
            *projects.entry(project.clone()).or_default() += fresh;
        }
    }
    let fresh = totals.fresh();
    PeriodKpis {
        cache_hit: share(totals.cache_read, totals.prompt()),
        models: ranked(models, fresh),
        subagents: share(subagent, fresh),
        thinking: share(thinking, totals.output),
        activity: buckets
            .iter()
            .map(|&(start, end)| {
                count(
                    inside
                        .iter()
                        .filter(|r| r.at >= start && r.at < end)
                        .count(),
                )
            })
            .collect(),
        responses: count(inside.len()),
        tool_calls,
        projects: ranked(projects, fresh),
    }
}

/// The latest answer of a main conversation before `now`.
#[must_use]
pub fn latest_context(records: &[UsageRecord], now: Timestamp) -> Option<ContextUse> {
    records
        .iter()
        .filter(|r| !r.subagent && r.at <= now)
        .max_by_key(|r| r.at)
        .map(|r| ContextUse {
            model: r.model.clone(),
            project: r.project.clone(),
            prompt: r.tokens.prompt(),
            window: context_window(&r.model),
            at: r.at,
            idle: now.duration_since(r.at).unsigned_abs(),
        })
}

/// `part / total` in per mille, rounded.
fn share(part: u64, total: u64) -> Option<u16> {
    (total > 0).then(|| {
        let permille = (u128::from(part) * 1000 + u128::from(total) / 2) / u128::from(total);
        u16::try_from(permille.min(1000)).unwrap_or(1000)
    })
}

fn ranked<K>(amounts: BTreeMap<K, u64>, total: u64) -> Vec<(K, u16)> {
    let mut ranked: Vec<(K, u64)> = amounts
        .into_iter()
        .filter(|(_, amount)| *amount > 0)
        .collect();
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    ranked
        .into_iter()
        .map(|(key, amount)| (key, share(amount, total).unwrap_or(0)))
        .collect()
}

/// Answers per day over many weeks, for a contribution-style calendar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityCalendar {
    /// First day shown; always a Monday, so columns are whole weeks.
    pub first_day: jiff::civil::Date,
    /// Answers per day from `first_day` to today.
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

/// Counts `times` per local day from `calendar_start` to `today`.
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

/// Everything the AI tab shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageReport {
    /// One entry per period, in [`Period::ALL`] order.
    pub periods: Vec<PeriodUsage>,
    /// Sources in configured order.
    pub sources: Vec<SourceState>,
    /// Context of the latest main answer; independent of the period.
    pub context: Option<ContextUse>,
    /// Answers per day over the last weeks.
    pub calendar: ActivityCalendar,
}

/// Summarises `records` for every period. `unreadable` lines make token and
/// cost values partial, as does a model without a price.
#[must_use]
pub fn summarize(
    records: &[UsageRecord],
    ranges: &[(Period, Timestamp, Timestamp)],
    prices: &PriceTable,
    unreadable: u64,
) -> Vec<PeriodUsage> {
    let base = if unreadable > 0 {
        Accuracy::Partial
    } else {
        Accuracy::Exact
    };
    ranges
        .iter()
        .map(|&(period, from, to)| {
            let mut models: BTreeMap<(Provider, ModelId), (ModelUsage, u128, HashSet<&str>)> =
                BTreeMap::new();
            let mut sessions = HashSet::new();
            let mut tokens = TokenUsage::default();
            let mut scaled = 0u128;
            let mut unpriced = false;
            for record in records.iter().filter(|r| r.at >= from && r.at < to) {
                sessions.insert(record.session.as_str());
                tokens += record.tokens;
                let cost = prices.cost_scaled(record);
                scaled += cost.unwrap_or(0);
                unpriced |= cost.is_none();
                let entry = models
                    .entry((record.provider, record.model.clone()))
                    .or_insert_with(|| {
                        (
                            ModelUsage {
                                provider: record.provider,
                                model: record.model.clone(),
                                responses: 0,
                                sessions: 0,
                                tokens: TokenUsage::default(),
                                cost: Some(CostEstimate::from_scaled(0, PRICES_AS_OF)),
                            },
                            0,
                            HashSet::new(),
                        )
                    });
                entry.0.responses += 1;
                entry.0.tokens += record.tokens;
                entry.2.insert(record.session.as_str());
                match cost {
                    Some(cost) => entry.1 += cost,
                    None => entry.0.cost = None,
                }
            }
            let mut models: Vec<ModelUsage> = models
                .into_values()
                .map(|(mut usage, scaled, sessions)| {
                    usage.sessions = count(sessions.len());
                    if usage.cost.is_some() {
                        usage.cost = Some(CostEstimate::from_scaled(scaled, PRICES_AS_OF));
                    }
                    usage
                })
                .collect();
            models.sort_by(|a, b| {
                let cost = |m: &ModelUsage| m.cost.map_or(0, |c| c.micro_usd);
                cost(b)
                    .cmp(&cost(a))
                    .then(b.tokens.total().cmp(&a.tokens.total()))
            });
            PeriodUsage {
                period,
                models,
                sessions: Measured {
                    value: count(sessions.len()),
                    accuracy: base,
                },
                tokens: Measured {
                    value: tokens,
                    accuracy: base,
                },
                cost: Measured {
                    value: CostEstimate::from_scaled(scaled, PRICES_AS_OF),
                    accuracy: if unpriced || unreadable > 0 {
                        Accuracy::Partial
                    } else {
                        Accuracy::Estimated
                    },
                },
                telemetry: Measured {
                    value: TelemetryTotals::default(),
                    accuracy: Accuracy::Partial,
                },
                kpis: PeriodKpis::default(),
            }
        })
        .collect()
}

fn count(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}

/// Activity buckets of a period: the 24 hours of today, else its days.
fn buckets(
    period: Period,
    from: Timestamp,
    to: Timestamp,
    zone: &TimeZone,
) -> Vec<(Timestamp, Timestamp)> {
    use jiff::ToSpan;
    let step = if period == Period::Today {
        1.hour()
    } else {
        1.day()
    };
    let start = from.to_zoned(zone.clone());
    // Today shows the whole day, so the hours ahead stay empty but visible.
    let end = match period {
        Period::Today => start.checked_add(1.day()).map_or(to, |end| end.timestamp()),
        _ => to,
    };
    let mut buckets = Vec::new();
    let mut current = start;
    while current.timestamp() < end && buckets.len() < 48 {
        let Ok(next) = current.checked_add(step) else {
            break;
        };
        buckets.push((current.timestamp(), next.timestamp().min(end)));
        current = next;
    }
    buckets
}

/// Reads all sources into the store, then reports usage per period.
pub struct UsageAgent<S, C> {
    sources: Vec<Arc<dyn UsageSource>>,
    store: S,
    clock: C,
    time_zone: TimeZone,
    settings: watch::Receiver<Settings>,
    interval: Duration,
}

impl<S: UsageStore + TelemetryStore, C: Clock> UsageAgent<S, C> {
    /// Agent over `sources`; prices follow the price overrides in `settings`.
    pub fn new(
        sources: Vec<Arc<dyn UsageSource>>,
        store: S,
        clock: C,
        time_zone: TimeZone,
        settings: watch::Receiver<Settings>,
        interval: Duration,
    ) -> Self {
        Self {
            sources,
            store,
            clock,
            time_zone,
            settings,
            interval,
        }
    }

    /// Reads and reports at once, then on every interval and settings change.
    pub async fn run(mut self, events: mpsc::Sender<AppEvent>, cancel: CancellationToken) {
        let mut timer = tokio::time::interval(self.interval);
        timer.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = cancel.cancelled() => return,
                _ = timer.tick() => {}
                Ok(()) = self.settings.changed() => {}
            }
            let report = self.cycle().await;
            if events
                .send(AppEvent::Usage(Box::new(report)))
                .await
                .is_err()
            {
                return;
            }
        }
    }

    async fn cycle(&mut self) -> UsageReport {
        let mut sources = Vec::with_capacity(self.sources.len());
        for source in &self.sources {
            sources.push(self.ingest(source.as_ref()).await);
        }
        let now = self.clock.now().to_zoned(self.time_zone.clone());
        let ranges: Vec<_> = Period::ALL
            .iter()
            .filter_map(|&period| period.range(&now).ok().map(|(from, to)| (period, from, to)))
            .collect();
        let from = ranges.iter().map(|r| r.1).min().unwrap_or(now.timestamp());
        let records = match self.store.records(from, now.timestamp()).await {
            Ok(records) => records,
            Err(error) => {
                tracing::warn!(%error, "could not read usage records");
                Vec::new()
            }
        };
        let prices = PriceTable::new(self.settings.borrow_and_update().prices.clone());
        let unreadable = sources.iter().map(|s| s.unreadable).sum();
        let mut periods = summarize(&records, &ranges, &prices, unreadable);
        for (usage, &(period, from, to)) in periods.iter_mut().zip(&ranges) {
            match self.store.totals(from, to).await {
                Ok(totals) => usage.telemetry.value = totals,
                Err(error) => tracing::warn!(%error, "could not read telemetry"),
            }
            let tool_calls = self
                .store
                .tool_calls(from, to)
                .await
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "could not count tool calls");
                    0
                });
            let buckets = buckets(period, from, to, &self.time_zone);
            usage.kpis = period_kpis(&records, (from, to), &buckets, tool_calls);
        }
        let today = now.date();
        let first = calendar_start(today)
            .to_zoned(self.time_zone.clone())
            .map_or(now.timestamp(), |start| start.timestamp());
        let times = self
            .store
            .answer_times(first, now.timestamp())
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not read answer times");
                Vec::new()
            });
        UsageReport {
            context: latest_context(&records, now.timestamp()),
            calendar: calendar(&times, today, &self.time_zone),
            periods,
            sources,
        }
    }

    async fn ingest(&self, source: &dyn UsageSource) -> SourceState {
        let provider = source.provider();
        let cursors = match self.store.cursors(provider).await {
            Ok(cursors) => cursors,
            Err(error) => {
                tracing::warn!(%error, "could not read usage cursors");
                return SourceState {
                    provider,
                    error: Some(UsageError::Io(error.to_string())),
                    unreadable: 0,
                    files: 0,
                    read_ms: 0,
                };
            }
        };
        let previous: u64 = cursors.values().map(|c| c.unreadable).sum();
        let known = count(cursors.len());
        let started = std::time::Instant::now();
        let result = source.read(cursors).await;
        let read_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        match result {
            Ok(batch) => {
                let unreadable = batch.cursors.values().map(|c| c.unreadable).sum();
                let files = count(batch.cursors.len());
                let added = batch.records.len();
                if let Err(error) = self.store.save(provider, batch).await {
                    tracing::warn!(%error, "could not store usage");
                    return SourceState {
                        provider,
                        error: Some(UsageError::Io(error.to_string())),
                        unreadable: previous,
                        files: known,
                        read_ms,
                    };
                }
                tracing::debug!(?provider, added, unreadable, "usage read");
                SourceState {
                    provider,
                    error: None,
                    unreadable,
                    files,
                    read_ms,
                }
            }
            Err(error) => SourceState {
                provider,
                error: Some(error),
                unreadable: previous,
                files: known,
                read_ms,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures::future::BoxFuture;
    use hud_domain::usage::{Geo, Speed};

    use super::*;
    use crate::ports::store::StoreError;
    use crate::ports::usage::{Cursors, FileCursor, UsageBatch};
    use crate::testing::FakeClock;

    fn at(text: &str) -> Timestamp {
        text.parse().expect("BUG: timestamp")
    }

    fn record(key: &str, session: &str, model: &str, when: &str, output: u64) -> UsageRecord {
        UsageRecord {
            provider: Provider::Claude,
            key: key.into(),
            session: session.into(),
            model: ModelId::try_from(model.to_owned()).expect("BUG"),
            at: at(when),
            tokens: TokenUsage {
                output,
                ..TokenUsage::default()
            },
            speed: Speed::Standard,
            geo: Geo::Global,
            project: Some("hud".into()),
            subagent: false,
            thinking: 0,
        }
    }

    fn today() -> Vec<(Period, Timestamp, Timestamp)> {
        vec![(
            Period::Today,
            at("2026-10-17T00:00:00Z"),
            at("2026-10-17T12:00:00Z"),
        )]
    }

    #[test]
    fn summary_counts_sessions_tokens_and_cost_per_model() {
        let records = [
            record(
                "a",
                "s1",
                "claude-opus-5-5",
                "2026-10-17T08:00:00Z",
                1_000_000,
            ),
            record(
                "b",
                "s1",
                "claude-opus-5-5",
                "2026-10-17T09:00:00Z",
                1_000_000,
            ),
            record(
                "c",
                "s2",
                "claude-haiku-4-5",
                "2026-10-17T10:00:00Z",
                1_000_000,
            ),
            record(
                "d",
                "s3",
                "claude-opus-5-5",
                "2026-10-16T23:59:59Z",
                1_000_000,
            ),
        ];
        let summary = &summarize(&records, &today(), &PriceTable::default(), 0)[0];
        assert_eq!(summary.sessions.value, 2);
        assert_eq!(summary.tokens.value.output, 3_000_000);
        assert_eq!(summary.cost.value.micro_usd, 45_000_000, "2 × $20 + $5");
        assert_eq!(summary.cost.accuracy, Accuracy::Estimated);
        assert_eq!(summary.models[0].model.as_str(), "claude-opus-5-5");
        assert_eq!(summary.models[0].responses, 2);
        assert_eq!(summary.models[0].sessions, 1);
    }

    fn hours() -> Vec<(Timestamp, Timestamp)> {
        vec![
            (at("2026-10-17T00:00:00Z"), at("2026-10-17T09:00:00Z")),
            (at("2026-10-17T09:00:00Z"), at("2026-10-17T12:00:00Z")),
        ]
    }

    #[test]
    fn period_kpis_split_fresh_tokens_by_model_project_and_subagent() {
        let mut opus = record("a", "s1", "claude-opus-5-5", "2026-10-17T08:00:00Z", 600);
        opus.tokens.cache_read = 9_000;
        opus.tokens.input = 400;
        opus.thinking = 300;
        let mut haiku = record("b", "s1", "claude-haiku-4-5", "2026-10-17T10:00:00Z", 1_000);
        haiku.subagent = true;
        haiku.project = Some("wiki".into());
        let outside = record("c", "s9", "claude-opus-5-5", "2026-10-16T10:00:00Z", 5_000);
        let range = (at("2026-10-17T00:00:00Z"), at("2026-10-17T12:00:00Z"));
        let kpis = period_kpis(&[opus, haiku, outside], range, &hours(), 3);
        assert_eq!(
            kpis.models[0].1, 500,
            "both models have 1 000 of 2 000 fresh tokens"
        );
        assert_eq!(kpis.subagents, Some(500));
        assert_eq!(kpis.thinking, Some(188), "300 of 1 600 output");
        assert_eq!(kpis.cache_hit, Some(957), "9 000 of 9 400 prompt tokens");
        assert_eq!(kpis.activity, vec![1, 1]);
        assert_eq!(kpis.responses, 2);
        assert_eq!(kpis.tool_calls, 3);
        assert_eq!(kpis.projects.len(), 2);
    }

    #[test]
    fn period_kpis_without_records_have_no_shares() {
        let range = (at("2026-10-17T00:00:00Z"), at("2026-10-17T12:00:00Z"));
        let kpis = period_kpis(&[], range, &hours(), 0);
        assert_eq!(kpis.cache_hit, None);
        assert_eq!(kpis.activity, vec![0, 0]);
    }

    #[test]
    fn the_context_is_the_latest_main_answer() {
        let mut main = record("a", "s1", "claude-opus-5-5", "2026-10-17T08:00:00Z", 1);
        main.tokens.cache_read = 121_000;
        let mut sub = record("b", "s1", "claude-haiku-4-5", "2026-10-17T09:00:00Z", 1);
        sub.subagent = true;
        let context = latest_context(&[main, sub], at("2026-10-17T08:06:00Z")).expect("BUG");
        assert_eq!(context.prompt, 121_000);
        assert_eq!(context.window, 1_000_000);
        assert_eq!(context.idle.as_secs(), 360);
    }

    #[test]
    fn the_calendar_starts_on_a_monday_and_ends_today() {
        let today = jiff::civil::date(2026, 10, 17); // a Saturday
        let times = [
            at("2026-10-17T08:00:00Z"),
            at("2026-10-12T08:00:00Z"),
            at("2025-01-01T00:00:00Z"),
        ];
        let calendar = calendar(&times, today, &TimeZone::UTC);
        assert_eq!(calendar.first_day.weekday(), jiff::civil::Weekday::Monday);
        assert_eq!(
            calendar.days.len(),
            52 * 7 + 6,
            "52 full weeks plus Monday to Saturday"
        );
        assert_eq!(calendar.days.last(), Some(&1));
        assert_eq!(
            calendar.days[calendar.days.len() - 6],
            1,
            "Monday of this week"
        );
        assert_eq!(
            calendar.days.iter().sum::<u64>(),
            2,
            "the old answer is outside"
        );
    }

    #[test]
    fn today_has_24_hourly_buckets_and_other_periods_daily_ones() {
        let today = buckets(
            Period::Today,
            at("2026-10-17T00:00:00Z"),
            at("2026-10-17T12:00:00Z"),
            &TimeZone::UTC,
        );
        assert_eq!(today.len(), 24);
        let week = buckets(
            Period::Last7Days,
            at("2026-10-11T00:00:00Z"),
            at("2026-10-17T12:00:00Z"),
            &TimeZone::UTC,
        );
        assert_eq!(week.len(), 7);
        assert_eq!(
            week[6].1,
            at("2026-10-17T12:00:00Z"),
            "the last day ends now"
        );
    }

    #[test]
    fn an_unpriced_model_makes_the_cost_partial() {
        let records = [record("a", "s", "mystery-1", "2026-10-17T08:00:00Z", 10)];
        let summary = &summarize(&records, &today(), &PriceTable::default(), 0)[0];
        assert_eq!(summary.cost.accuracy, Accuracy::Partial);
        assert_eq!(summary.models[0].cost, None);
        assert_eq!(summary.tokens.accuracy, Accuracy::Exact);
    }

    #[test]
    fn unreadable_lines_make_everything_partial() {
        let summary = &summarize(&[], &today(), &PriceTable::default(), 3)[0];
        assert_eq!(summary.tokens.accuracy, Accuracy::Partial);
        assert_eq!(summary.cost.accuracy, Accuracy::Partial);
    }

    struct FakeSource;

    impl UsageSource for FakeSource {
        fn provider(&self) -> Provider {
            Provider::Claude
        }

        fn read(&self, cursors: Cursors) -> BoxFuture<'_, Result<UsageBatch, UsageError>> {
            Box::pin(async move {
                let mut cursors = cursors;
                cursors.insert(
                    "log.jsonl".into(),
                    FileCursor {
                        size: 10,
                        offset: 10,
                        unreadable: 1,
                    },
                );
                Ok(UsageBatch {
                    records: vec![record(
                        "a",
                        "s",
                        "claude-opus-5-5",
                        "2026-10-17T11:00:00Z",
                        1_000_000,
                    )],
                    tool_calls: Vec::new(),
                    cursors,
                })
            })
        }
    }

    #[derive(Default)]
    struct FakeStore {
        records: Mutex<Vec<UsageRecord>>,
        cursors: Mutex<Cursors>,
    }

    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the fake answers immediately; the trait still needs async signatures"
    )]
    impl UsageStore for FakeStore {
        async fn cursors(&self, _provider: Provider) -> Result<Cursors, StoreError> {
            Ok(self.cursors.lock().expect("BUG").clone())
        }

        async fn save(&self, _provider: Provider, batch: UsageBatch) -> Result<(), StoreError> {
            self.records.lock().expect("BUG").extend(batch.records);
            *self.cursors.lock().expect("BUG") = batch.cursors;
            Ok(())
        }

        async fn answer_times(
            &self,
            from: Timestamp,
            to: Timestamp,
        ) -> Result<Vec<Timestamp>, StoreError> {
            let records = self.records.lock().expect("BUG");
            Ok(records
                .iter()
                .filter(|r| r.at >= from && r.at < to)
                .map(|r| r.at)
                .collect())
        }

        async fn tool_calls(&self, _from: Timestamp, _to: Timestamp) -> Result<u64, StoreError> {
            Ok(7)
        }

        async fn records(
            &self,
            from: Timestamp,
            to: Timestamp,
        ) -> Result<Vec<UsageRecord>, StoreError> {
            let records = self.records.lock().expect("BUG");
            Ok(records
                .iter()
                .filter(|r| r.at >= from && r.at < to)
                .cloned()
                .collect())
        }
    }

    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the fake answers immediately; the trait still needs async signatures"
    )]
    impl TelemetryStore for FakeStore {
        async fn save_points(
            &self,
            _points: Vec<hud_domain::usage::TelemetryPoint>,
        ) -> Result<(), StoreError> {
            Ok(())
        }

        async fn totals(
            &self,
            _from: Timestamp,
            _to: Timestamp,
        ) -> Result<TelemetryTotals, StoreError> {
            Ok(TelemetryTotals {
                commits: 2,
                ..TelemetryTotals::default()
            })
        }
    }

    #[tokio::test]
    async fn the_agent_reads_stores_and_reports_every_period() {
        let store = Arc::new(FakeStore::default());
        let (_settings, watched) = watch::channel(Settings::default());
        let agent = UsageAgent::new(
            vec![Arc::new(FakeSource)],
            Arc::clone(&store),
            FakeClock,
            TimeZone::UTC,
            watched,
            Duration::from_secs(60),
        );
        let (events_tx, mut events) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(agent.run(events_tx, cancel.clone()));

        let Some(AppEvent::Usage(report)) = events.recv().await else {
            panic!("expected a usage report")
        };
        assert_eq!(report.periods.len(), Period::ALL.len());
        assert_eq!(report.periods[0].tokens.value.output, 1_000_000);
        assert_eq!(report.sources[0].unreadable, 1);
        assert_eq!(report.periods[0].tokens.accuracy, Accuracy::Partial);
        assert_eq!(store.cursors.lock().expect("BUG")["log.jsonl"].offset, 10);
        assert_eq!(report.periods[0].telemetry.value.commits, 2);
        assert_eq!(report.periods[0].kpis.tool_calls, 7);
        assert_eq!(report.calendar.days.last(), Some(&1), "today's answer");
        assert_eq!(report.periods[0].kpis.activity.len(), 24);
        assert_eq!(report.sources[0].files, 1);
        assert_eq!(report.periods[0].telemetry.accuracy, Accuracy::Partial);
        cancel.cancel();
        task.await.expect("BUG");
    }
}
