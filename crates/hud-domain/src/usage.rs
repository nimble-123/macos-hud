//! AI usage: tokens, periods, prices and cost estimates.
//!
//! Money is integer micro-USD (ADR 0009). Prices are micro-USD per million
//! tokens, so a cost is `tokens × price / 1 000 000` and stays exact until the
//! final division.

use std::collections::BTreeMap;
use std::fmt;
use std::ops::AddAssign;

use jiff::civil::Date;
use jiff::{Timestamp, ToSpan, Zoned};

use crate::DomainError;

/// Who served the requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Provider {
    /// Claude Code (CLI and desktop).
    Claude,
    /// OpenAI Codex.
    Codex,
}

/// Model id as the provider reports it, e.g. `claude-opus-5-5`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelId(String);

impl ModelId {
    /// The id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ModelId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || value.chars().any(char::is_whitespace) {
            return Err(DomainError::InvalidModelId(value));
        }
        Ok(Self(value))
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Tokens of one or many requests, split the way they are priced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TokenUsage {
    /// Uncached input.
    pub input: u64,
    /// Output, including thinking.
    pub output: u64,
    /// Input written to the 5-minute cache.
    pub cache_write_5m: u64,
    /// Input written to the 1-hour cache.
    pub cache_write_1h: u64,
    /// Input read from the cache.
    pub cache_read: u64,
    /// Reasoning tokens, where the provider reports them separately (Codex).
    pub reasoning: Option<u64>,
}

impl TokenUsage {
    /// Everything the model read: uncached input, cache writes and reads.
    #[must_use]
    pub fn prompt(&self) -> u64 {
        self.input + self.cache_write_5m + self.cache_write_1h + self.cache_read
    }

    /// All tokens together.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.prompt() + self.output + self.reasoning.unwrap_or(0)
    }
}

impl AddAssign for TokenUsage {
    fn add_assign(&mut self, other: Self) {
        self.input += other.input;
        self.output += other.output;
        self.cache_write_5m += other.cache_write_5m;
        self.cache_write_1h += other.cache_write_1h;
        self.cache_read += other.cache_read;
        self.reasoning = match (self.reasoning, other.reasoning) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        };
    }
}

/// Speed tier of a request; fast mode costs more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Speed {
    /// Normal.
    #[default]
    Standard,
    /// Fast mode.
    Fast,
}

/// Where a request was served; US-only inference costs 10 % more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Geo {
    /// Global routing (default).
    #[default]
    Global,
    /// US-only inference.
    Us,
}

/// Usage of one model response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageRecord {
    /// Provider.
    pub provider: Provider,
    /// Identifies the response across log lines, for de-duplication.
    pub key: String,
    /// Session the response belongs to.
    pub session: String,
    /// Model.
    pub model: ModelId,
    /// When it was answered.
    pub at: Timestamp,
    /// Tokens.
    pub tokens: TokenUsage,
    /// Speed tier.
    pub speed: Speed,
    /// Inference geography.
    pub geo: Geo,
    /// Project the session ran in, e.g. the repository folder name.
    pub project: Option<String>,
    /// Answered by a subagent rather than the main conversation.
    pub subagent: bool,
    /// Part of the output spent on thinking.
    pub thinking: u64,
    /// Context window the provider reported with the response, if any.
    pub context_window: Option<u64>,
}

/// One tool call, counted once by its id however often a log repeats it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    /// Provider.
    pub provider: Provider,
    /// The call's id, e.g. `toolu_…`.
    pub id: String,
    /// When the answer that made it was written.
    pub at: Timestamp,
}

impl TokenUsage {
    /// Tokens that cost full price or more: everything except cache reads.
    #[must_use]
    pub fn fresh(&self) -> u64 {
        self.input
            + self.cache_write_5m
            + self.cache_write_1h
            + self.output
            + self.reasoning.unwrap_or(0)
    }
}

/// Context window of a Claude `model` in tokens: 1 M from Claude 4.6 on,
/// 200 k before; unknown models are assumed to be current. Codex reports the
/// window with every response instead.
#[must_use]
pub fn context_window(model: &ModelId) -> u64 {
    const WINDOWS: [(&str, u64); 9] = [
        ("claude-opus-4", 200_000),
        ("claude-opus-4-1", 200_000),
        ("claude-opus-4-5", 200_000),
        ("claude-opus-4-6", 1_000_000),
        ("claude-opus-4-7", 1_000_000),
        ("claude-opus-4-8", 1_000_000),
        ("claude-sonnet-4", 200_000),
        ("claude-sonnet-4-5", 200_000),
        ("claude-haiku-4-5", 200_000),
    ];
    longest_match(WINDOWS.iter().copied(), model.as_str()).unwrap_or(1_000_000)
}

/// How far a value can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Accuracy {
    /// Counted from the source.
    Exact,
    /// Derived, e.g. a cost from a price table.
    Estimated,
    /// Some input was missing or unreadable.
    Partial,
    /// The source does not provide it.
    Unavailable,
}

/// A value together with how far it can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measured<T> {
    /// The value.
    pub value: T,
    /// Its accuracy.
    pub accuracy: Accuracy,
}

/// One rate-limit window of a subscription, as the provider last reported it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanWindow {
    /// Length of the window, e.g. 300 for five hours.
    pub minutes: Option<u32>,
    /// How much of the limit is used, in per mille.
    pub used_permille: u16,
    /// When the window starts over.
    pub resets_at: Timestamp,
}

impl PlanWindow {
    /// The value still describes the running window; after `resets_at` it is
    /// only known that the limit started over.
    #[must_use]
    pub fn is_current(&self, now: Timestamp) -> bool {
        self.resets_at > now
    }
}

/// Subscription usage as the provider last reported it (Codex).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanUsage {
    /// When it was reported.
    pub at: Timestamp,
    /// Plan name, e.g. `plus`.
    pub plan: Option<String>,
    /// The main window.
    pub primary: Option<PlanWindow>,
    /// A second, usually longer window.
    pub secondary: Option<PlanWindow>,
}

/// The periods the AI dashboard offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Period {
    /// Since midnight.
    #[default]
    Today,
    /// Today and the six days before.
    Last7Days,
    /// Today and the 29 days before.
    Last30Days,
    /// Since the first of this month.
    ThisMonth,
    /// The previous calendar month.
    LastMonth,
}

impl Period {
    /// All periods in display order.
    pub const ALL: [Self; 5] = [
        Self::Today,
        Self::Last7Days,
        Self::Last30Days,
        Self::ThisMonth,
        Self::LastMonth,
    ];

    /// Start (inclusive) and end (exclusive) in the time zone of `now`.
    /// Days and months follow the calendar there, so DST changes are handled.
    ///
    /// # Errors
    ///
    /// Fails only for dates outside the range `jiff` supports.
    pub fn range(self, now: &Zoned) -> Result<(Timestamp, Timestamp), jiff::Error> {
        let today = now.start_of_day()?;
        let month = today.first_of_month()?;
        let end = now.timestamp();
        Ok(match self {
            Self::Today => (today.timestamp(), end),
            Self::Last7Days => (today.checked_sub(6.days())?.timestamp(), end),
            Self::Last30Days => (today.checked_sub(29.days())?.timestamp(), end),
            Self::ThisMonth => (month.timestamp(), end),
            Self::LastMonth => (month.checked_sub(1.month())?.timestamp(), month.timestamp()),
        })
    }
}

/// Prices of one model in micro-USD per million tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Price {
    /// Uncached input.
    pub input: u64,
    /// Output.
    pub output: u64,
    /// 5-minute cache write.
    pub cache_write_5m: u64,
    /// 1-hour cache write.
    pub cache_write_1h: u64,
    /// Cache read.
    pub cache_read: u64,
}

impl Price {
    /// Cost of `tokens` in micro-USD × 1 000 000, so sums stay exact.
    fn cost_scaled(&self, tokens: &TokenUsage) -> u128 {
        let part = |count: u64, price: u64| u128::from(count) * u128::from(price);
        part(tokens.input, self.input)
            + part(tokens.output + tokens.reasoning.unwrap_or(0), self.output)
            + part(tokens.cache_write_5m, self.cache_write_5m)
            + part(tokens.cache_write_1h, self.cache_write_1h)
            + part(tokens.cache_read, self.cache_read)
    }
}

/// How one model is priced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelPricing {
    /// Normal prices.
    pub standard: Price,
    /// Fast-mode prices, for models that offer it.
    pub fast: Option<Price>,
    /// Higher prices for requests whose prompt exceeds the threshold.
    pub long_prompt: Option<(u64, Price)>,
}

/// `dollars` plus `ten_thousandths` of a dollar, in micro-USD: `usd(0, 2500)` is $0.25.
const fn usd(dollars: u64, ten_thousandths: u64) -> u64 {
    dollars * 1_000_000 + ten_thousandths * 100
}

/// Standard pricing: cache writes 1.25× and 2×, reads as given.
const fn standard(input: u64, output: u64, cache_read: u64) -> ModelPricing {
    ModelPricing {
        standard: Price {
            input,
            output,
            cache_write_5m: input / 4 * 5,
            cache_write_1h: input * 2,
            cache_read,
        },
        fast: None,
        long_prompt: None,
    }
}

const fn with_fast(
    mut pricing: ModelPricing,
    input: u64,
    output: u64,
    read_ratio_x1000: u64,
) -> ModelPricing {
    pricing.fast = Some(Price {
        input,
        output,
        cache_write_5m: input / 4 * 5,
        cache_write_1h: input * 2,
        cache_read: input * read_ratio_x1000 / 1000,
    });
    pricing
}

/// Date of the built-in prices: <https://platform.claude.com/docs/en/about-claude/pricing>
/// and <https://developers.openai.com/api/docs/pricing>.
pub const PRICES_AS_OF: Date = jiff::civil::date(2026, 10, 17);

/// OpenAI pricing: a cache write costs `write`, or the input price when the
/// model lists none. With `long_context`, prompts above 272 k tokens cost
/// twice as much for input and cache, and 1.5× for output.
const fn openai(
    input: u64,
    cached: u64,
    write: u64,
    output: u64,
    long_context: bool,
) -> ModelPricing {
    let write = if write == 0 { input } else { write };
    ModelPricing {
        standard: Price {
            input,
            output,
            cache_write_5m: write,
            cache_write_1h: write,
            cache_read: cached,
        },
        fast: None,
        long_prompt: if long_context {
            Some((
                272_000,
                Price {
                    input: input * 2,
                    output: output / 2 * 3,
                    cache_write_5m: write * 2,
                    cache_write_1h: write * 2,
                    cache_read: cached * 2,
                },
            ))
        } else {
            None
        },
    }
}

/// Built-in prices by model id prefix. A model id matches an entry when it
/// equals the key or continues with `-`, e.g. a date suffix; the longest key wins.
const BUILT_IN: [(&str, ModelPricing); 38] = [
    (
        "claude-fable-5-1",
        standard(usd(10, 0), usd(50, 0), usd(0, 2500)),
    ),
    (
        "claude-mythos-5-1",
        standard(usd(10, 0), usd(50, 0), usd(0, 2500)),
    ),
    (
        "claude-fable-5",
        standard(usd(10, 0), usd(50, 0), usd(1, 0)),
    ),
    (
        "claude-mythos-5",
        standard(usd(10, 0), usd(50, 0), usd(1, 0)),
    ),
    (
        "claude-opus-5-5",
        with_fast(
            standard(usd(4, 0), usd(20, 0), usd(0, 2000)),
            usd(8, 0),
            usd(40, 0),
            50,
        ),
    ),
    (
        "claude-opus-5",
        with_fast(
            standard(usd(5, 0), usd(25, 0), usd(0, 5000)),
            usd(10, 0),
            usd(50, 0),
            100,
        ),
    ),
    (
        "claude-opus-4-8",
        with_fast(
            standard(usd(5, 0), usd(25, 0), usd(0, 5000)),
            usd(10, 0),
            usd(50, 0),
            100,
        ),
    ),
    (
        "claude-opus-4-7",
        standard(usd(5, 0), usd(25, 0), usd(0, 5000)),
    ),
    (
        "claude-opus-4-6",
        standard(usd(5, 0), usd(25, 0), usd(0, 5000)),
    ),
    (
        "claude-opus-4-5",
        standard(usd(5, 0), usd(25, 0), usd(0, 5000)),
    ),
    (
        "claude-opus-4-1",
        standard(usd(15, 0), usd(75, 0), usd(1, 5000)),
    ),
    (
        "claude-opus-4",
        standard(usd(15, 0), usd(75, 0), usd(1, 5000)),
    ),
    (
        "claude-sonnet-5-5",
        standard(usd(2, 0), usd(10, 0), usd(0, 1000)),
    ),
    (
        "claude-sonnet-5",
        standard(usd(2, 0), usd(10, 0), usd(0, 2000)),
    ),
    (
        "claude-sonnet-4-6",
        standard(usd(3, 0), usd(15, 0), usd(0, 3000)),
    ),
    (
        "claude-sonnet-4-5",
        standard(usd(3, 0), usd(15, 0), usd(0, 3000)),
    ),
    (
        "claude-sonnet-4",
        standard(usd(3, 0), usd(15, 0), usd(0, 3000)),
    ),
    (
        "claude-haiku-5-5",
        ModelPricing {
            long_prompt: Some((
                100_000,
                Price {
                    input: usd(0, 5000),
                    output: usd(2, 5000),
                    cache_write_5m: usd(0, 6250),
                    cache_write_1h: usd(1, 0),
                    cache_read: usd(0, 500),
                },
            )),
            ..standard(usd(0, 1000), usd(0, 5000), usd(0, 100))
        },
    ),
    (
        "claude-haiku-4-5",
        standard(usd(1, 0), usd(5, 0), usd(0, 1000)),
    ),
    (
        "gpt-6-astra",
        openai(usd(10, 0), usd(1, 0), usd(12, 5000), usd(50, 0), true),
    ),
    (
        "gpt-6.1-sol",
        openai(usd(2, 0), usd(0, 1000), usd(2, 5000), usd(10, 0), true),
    ),
    (
        "gpt-6-sol",
        openai(usd(2, 0), usd(0, 2000), usd(2, 5000), usd(10, 0), true),
    ),
    (
        "gpt-6-luna",
        openai(usd(0, 1000), usd(0, 100), usd(0, 1250), usd(0, 5000), true),
    ),
    (
        "gpt-5.6-sol",
        openai(usd(4, 0), usd(0, 4000), usd(5, 0), usd(20, 0), true),
    ),
    (
        "gpt-5.6-terra",
        openai(usd(2, 0), usd(0, 2000), usd(2, 5000), usd(12, 0), true),
    ),
    (
        "gpt-5.6-luna",
        openai(usd(0, 2000), usd(0, 200), usd(0, 2500), usd(1, 2000), true),
    ),
    (
        "gpt-5.5",
        openai(usd(5, 0), usd(0, 5000), 0, usd(30, 0), true),
    ),
    (
        "gpt-5.4",
        openai(usd(2, 5000), usd(0, 2500), 0, usd(15, 0), true),
    ),
    (
        "gpt-5.4-mini",
        openai(usd(0, 7500), usd(0, 750), 0, usd(4, 5000), false),
    ),
    (
        "gpt-5.3-codex",
        openai(usd(1, 7500), usd(0, 1750), 0, usd(14, 0), false),
    ),
    (
        "gpt-5.2",
        openai(usd(1, 7500), usd(0, 1750), 0, usd(14, 0), false),
    ),
    (
        "gpt-5.2-codex",
        openai(usd(1, 7500), usd(0, 1750), 0, usd(14, 0), false),
    ),
    (
        "gpt-5.1",
        openai(usd(1, 2500), usd(0, 1250), 0, usd(10, 0), false),
    ),
    (
        "gpt-5.1-codex",
        openai(usd(1, 2500), usd(0, 1250), 0, usd(10, 0), false),
    ),
    (
        "gpt-5.1-codex-mini",
        openai(usd(0, 2500), usd(0, 250), 0, usd(2, 0), false),
    ),
    (
        "gpt-5",
        openai(usd(1, 2500), usd(0, 1250), 0, usd(10, 0), false),
    ),
    (
        "gpt-5-codex",
        openai(usd(1, 2500), usd(0, 1250), 0, usd(10, 0), false),
    ),
    (
        "gpt-5-mini",
        openai(usd(0, 2500), usd(0, 250), 0, usd(2, 0), false),
    ),
];

/// Prices hud knows, plus the user's overrides.
#[derive(Debug, Clone, Default)]
pub struct PriceTable {
    overrides: BTreeMap<String, Price>,
}

impl PriceTable {
    /// Built-in prices; an override replaces all prices of its model.
    #[must_use]
    pub fn new(overrides: BTreeMap<String, Price>) -> Self {
        Self { overrides }
    }

    /// Pricing for `model`, if known.
    #[must_use]
    pub fn pricing(&self, model: &ModelId) -> Option<ModelPricing> {
        let id = model.as_str();
        if let Some(price) = longest_match(self.overrides.iter().map(|(k, v)| (k.as_str(), *v)), id)
        {
            return Some(ModelPricing {
                standard: price,
                fast: None,
                long_prompt: None,
            });
        }
        longest_match(BUILT_IN.iter().copied(), id)
    }

    /// Cost of one response in micro-USD × 1 000 000; `None` for unknown models.
    #[must_use]
    pub fn cost_scaled(&self, record: &UsageRecord) -> Option<u128> {
        let pricing = self.pricing(&record.model)?;
        let price = match (record.speed, pricing.fast, pricing.long_prompt) {
            (Speed::Fast, Some(fast), _) => fast,
            (_, _, Some((threshold, long))) if record.tokens.prompt() > threshold => long,
            _ => pricing.standard,
        };
        let cost = price.cost_scaled(&record.tokens);
        Some(match record.geo {
            Geo::Global => cost,
            Geo::Us => cost * 11 / 10,
        })
    }
}

fn longest_match<'a, T: Copy>(entries: impl Iterator<Item = (&'a str, T)>, id: &str) -> Option<T> {
    entries
        .filter(|(key, _)| {
            id == *key
                || id
                    .strip_prefix(key)
                    .is_some_and(|rest| rest.starts_with('-'))
        })
        .max_by_key(|(key, _)| key.len())
        .map(|(_, value)| value)
}

/// What a telemetry data point from Claude Code measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TelemetryMetric {
    /// Cost as Claude Code computes it, in micro-USD.
    Cost,
    /// Tokens; the detail says which kind.
    Tokens,
    /// Active time in milliseconds; the detail says `user` or `cli`.
    ActiveTime,
    /// Lines added by Claude Code.
    LinesAdded,
    /// Lines removed by Claude Code.
    LinesRemoved,
    /// Commits created.
    Commits,
    /// Pull requests created.
    PullRequests,
    /// Sessions started.
    Sessions,
}

/// One delta data point. Only these fields are ever kept (ADR 0011).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryPoint {
    /// End of the interval the delta covers.
    pub at: Timestamp,
    /// What it measures.
    pub metric: TelemetryMetric,
    /// Sub-kind such as `input` tokens or `user` active time.
    pub detail: Option<String>,
    /// Model, where the metric has one.
    pub model: Option<ModelId>,
    /// Session.
    pub session: Option<String>,
    /// Amount in the metric's unit.
    pub value: u64,
}

/// Telemetry summed over a period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TelemetryTotals {
    /// Cost according to Claude Code, micro-USD.
    pub cost_micro_usd: u64,
    /// Active time typing and reading, milliseconds.
    pub active_user_ms: u64,
    /// Active time of tools and responses, milliseconds.
    pub active_cli_ms: u64,
    /// Lines added.
    pub lines_added: u64,
    /// Lines removed.
    pub lines_removed: u64,
    /// Commits.
    pub commits: u64,
    /// Pull requests.
    pub pull_requests: u64,
    /// Sessions started.
    pub sessions: u64,
}

impl TelemetryTotals {
    /// Adds one data point.
    pub fn add(&mut self, point: &TelemetryPoint) {
        let target = match (point.metric, point.detail.as_deref()) {
            (TelemetryMetric::Cost, _) => &mut self.cost_micro_usd,
            (TelemetryMetric::ActiveTime, Some("user")) => &mut self.active_user_ms,
            (TelemetryMetric::ActiveTime, _) => &mut self.active_cli_ms,
            (TelemetryMetric::LinesAdded, _) => &mut self.lines_added,
            (TelemetryMetric::LinesRemoved, _) => &mut self.lines_removed,
            (TelemetryMetric::Commits, _) => &mut self.commits,
            (TelemetryMetric::PullRequests, _) => &mut self.pull_requests,
            (TelemetryMetric::Sessions, _) => &mut self.sessions,
            // Token counts come from the logs, which cover every session.
            (TelemetryMetric::Tokens, _) => return,
        };
        *target += point.value;
    }
}

/// An estimated cost; there is deliberately no type for exact costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostEstimate {
    /// Amount in micro-USD.
    pub micro_usd: u64,
    /// Date of the price table used.
    pub prices_as_of: Date,
}

impl CostEstimate {
    /// Rounds a sum of [`PriceTable::cost_scaled`] values to micro-USD.
    #[must_use]
    pub fn from_scaled(scaled: u128, prices_as_of: Date) -> Self {
        let micro = (scaled + 500_000) / 1_000_000;
        Self {
            micro_usd: u64::try_from(micro).unwrap_or(u64::MAX),
            prices_as_of,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(model: &str, tokens: TokenUsage) -> UsageRecord {
        UsageRecord {
            provider: Provider::Claude,
            key: "k".into(),
            session: "s".into(),
            model: ModelId::try_from(model.to_owned()).expect("BUG: valid"),
            at: Timestamp::UNIX_EPOCH,
            tokens,
            speed: Speed::Standard,
            geo: Geo::Global,
            project: None,
            subagent: false,
            thinking: 0,
            context_window: None,
        }
    }

    fn cost(table: &PriceTable, record: &UsageRecord) -> u64 {
        CostEstimate::from_scaled(table.cost_scaled(record).expect("BUG: known"), PRICES_AS_OF)
            .micro_usd
    }

    #[test]
    fn opus_5_5_costs_match_the_price_page() {
        let tokens = TokenUsage {
            input: 1_000_000,
            output: 1_000_000,
            cache_write_5m: 1_000_000,
            cache_write_1h: 1_000_000,
            cache_read: 1_000_000,
            reasoning: None,
        };
        // $4 + $20 + $5 + $8 + $0.20
        assert_eq!(
            cost(&PriceTable::default(), &record("claude-opus-5-5", tokens)),
            37_200_000
        );
    }

    #[test]
    fn date_suffixed_ids_match_their_model_and_not_a_neighbour() {
        let table = PriceTable::default();
        let pricing = |id: &str| {
            table
                .pricing(&ModelId::try_from(id.to_owned()).expect("BUG"))
                .expect("BUG")
                .standard
                .input
        };
        assert_eq!(pricing("claude-opus-4-20250514"), usd(15, 0));
        assert_eq!(pricing("claude-opus-4-1-20250805"), usd(15, 0));
        assert_eq!(pricing("claude-opus-4-5-20251101"), usd(5, 0));
        assert_eq!(pricing("claude-sonnet-4-5-20250929"), usd(3, 0));
    }

    #[test]
    fn context_windows_follow_the_model_generation() {
        let window = |id: &str| context_window(&ModelId::try_from(id.to_owned()).expect("BUG"));
        assert_eq!(window("claude-opus-4-20250514"), 200_000);
        assert_eq!(window("claude-opus-4-6"), 1_000_000);
        assert_eq!(window("claude-haiku-4-5-20251001"), 200_000);
        assert_eq!(window("claude-sonnet-5-5"), 1_000_000);
    }

    #[test]
    fn unknown_models_have_no_price() {
        let table = PriceTable::default();
        assert_eq!(
            table.cost_scaled(&record("o9-preview", TokenUsage::default())),
            None
        );
    }

    #[test]
    fn codex_models_match_by_version_and_not_a_shorter_neighbour() {
        let table = PriceTable::default();
        let input = |id: &str| {
            table
                .pricing(&ModelId::try_from(id.to_owned()).expect("BUG"))
                .map(|pricing| pricing.standard.input)
        };
        assert_eq!(input("gpt-5-codex"), Some(usd(1, 2500)));
        assert_eq!(input("gpt-5.6-luna"), Some(usd(0, 2000)));
        assert_eq!(input("gpt-5.7-luna"), None, "never priced as gpt-5");
    }

    #[test]
    fn openai_reasoning_is_priced_as_output_and_long_prompts_cost_more() {
        let tokens = TokenUsage {
            input: 100_000,
            cache_read: 100_000,
            output: 400_000,
            reasoning: Some(600_000),
            ..TokenUsage::default()
        };
        // 0.1M × $0.10 + 0.1M × $0.01 + 1M × $0.50
        assert_eq!(
            cost(&PriceTable::default(), &record("gpt-6-luna", tokens)),
            511_000
        );
        let long = TokenUsage {
            input: 300_000,
            ..TokenUsage::default()
        };
        // 0.3M × $0.20 above 272 k prompt tokens
        assert_eq!(
            cost(&PriceTable::default(), &record("gpt-6-luna", long)),
            60_000
        );
    }

    #[test]
    fn a_plan_window_is_current_until_it_resets() {
        let window = PlanWindow {
            minutes: Some(300),
            used_permille: 120,
            resets_at: Timestamp::from_second(100).expect("BUG"),
        };
        assert!(window.is_current(Timestamp::from_second(99).expect("BUG")));
        assert!(!window.is_current(Timestamp::from_second(100).expect("BUG")));
    }

    #[test]
    fn fast_mode_uses_fast_prices_with_cache_multipliers() {
        let mut fast = record(
            "claude-opus-5-5",
            TokenUsage {
                input: 1_000_000,
                output: 1_000_000,
                cache_read: 1_000_000,
                ..TokenUsage::default()
            },
        );
        fast.speed = Speed::Fast;
        // $8 + $40 + 0.05 × $8
        assert_eq!(cost(&PriceTable::default(), &fast), 48_400_000);
    }

    #[test]
    fn haiku_5_5_uses_long_prompt_prices_above_100k_prompt_tokens() {
        let long = record(
            "claude-haiku-5-5",
            TokenUsage {
                input: 50_000,
                cache_read: 60_000,
                output: 1_000_000,
                ..TokenUsage::default()
            },
        );
        // 50k × $0.50 + 60k × $0.05 + 1M × $2.50 per MTok
        assert_eq!(cost(&PriceTable::default(), &long), 2_528_000);
    }

    #[test]
    fn us_inference_costs_ten_percent_more() {
        let mut us = record(
            "claude-sonnet-5-5",
            TokenUsage {
                output: 1_000_000,
                ..TokenUsage::default()
            },
        );
        us.geo = Geo::Us;
        assert_eq!(cost(&PriceTable::default(), &us), 11_000_000);
    }

    #[test]
    fn an_override_replaces_the_built_in_price() {
        let price = Price {
            input: 0,
            output: usd(1, 0),
            cache_write_5m: 0,
            cache_write_1h: 0,
            cache_read: 0,
        };
        let table = PriceTable::new(BTreeMap::from([("claude-opus-5-5".to_owned(), price)]));
        let tokens = TokenUsage {
            output: 2_000_000,
            ..TokenUsage::default()
        };
        assert_eq!(cost(&table, &record("claude-opus-5-5", tokens)), 2_000_000);
    }

    #[test]
    fn periods_follow_the_local_calendar() {
        let now: Zoned = "2026-03-29T15:00:00+02:00[Europe/Berlin]"
            .parse()
            .expect("BUG");
        let local = |period: Period| {
            let (start, end) = period.range(&now).expect("BUG");
            (
                start.to_zoned(now.time_zone().clone()).to_string(),
                end.to_zoned(now.time_zone().clone()).to_string(),
            )
        };
        assert_eq!(
            local(Period::Today).0,
            "2026-03-29T00:00:00+01:00[Europe/Berlin]"
        );
        assert_eq!(
            local(Period::Last7Days).0,
            "2026-03-23T00:00:00+01:00[Europe/Berlin]"
        );
        assert_eq!(
            local(Period::ThisMonth).0,
            "2026-03-01T00:00:00+01:00[Europe/Berlin]"
        );
        assert_eq!(
            local(Period::LastMonth),
            (
                "2026-02-01T00:00:00+01:00[Europe/Berlin]".to_owned(),
                "2026-03-01T00:00:00+01:00[Europe/Berlin]".to_owned()
            )
        );
    }

    #[test]
    fn adding_usage_keeps_reasoning_unknown_unless_reported() {
        let mut sum = TokenUsage::default();
        sum += TokenUsage {
            input: 1,
            ..TokenUsage::default()
        };
        assert_eq!(sum.reasoning, None);
        sum += TokenUsage {
            reasoning: Some(5),
            ..TokenUsage::default()
        };
        assert_eq!(sum.reasoning, Some(5));
    }
}
