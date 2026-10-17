//! Reading AI usage from local logs and summarising it per period.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use hud_domain::usage::{
    Accuracy, CostEstimate, Measured, ModelId, PRICES_AS_OF, Period, PriceTable, Provider,
    TelemetryTotals, TokenUsage, UsageRecord,
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
}

/// Everything the AI tab shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageReport {
    /// One entry per period, in [`Period::ALL`] order.
    pub periods: Vec<PeriodUsage>,
    /// Sources in configured order.
    pub sources: Vec<SourceState>,
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
            }
        })
        .collect()
}

fn count(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
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
        for (usage, (_, from, to)) in periods.iter_mut().zip(&ranges) {
            match self.store.totals(*from, *to).await {
                Ok(totals) => usage.telemetry.value = totals,
                Err(error) => tracing::warn!(%error, "could not read telemetry"),
            }
        }
        UsageReport { periods, sources }
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
                };
            }
        };
        let previous: u64 = cursors.values().map(|c| c.unreadable).sum();
        match source.read(cursors).await {
            Ok(batch) => {
                let unreadable = batch.cursors.values().map(|c| c.unreadable).sum();
                let added = batch.records.len();
                if let Err(error) = self.store.save(provider, batch).await {
                    tracing::warn!(%error, "could not store usage");
                    return SourceState {
                        provider,
                        error: Some(UsageError::Io(error.to_string())),
                        unreadable: previous,
                    };
                }
                tracing::debug!(?provider, added, unreadable, "usage read");
                SourceState {
                    provider,
                    error: None,
                    unreadable,
                }
            }
            Err(error) => SourceState {
                provider,
                error: Some(error),
                unreadable: previous,
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
        assert_eq!(report.periods[0].telemetry.accuracy, Accuracy::Partial);
        cancel.cancel();
        task.await.expect("BUG");
    }
}
