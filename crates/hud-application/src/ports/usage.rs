//! AI usage: sources that read logs incrementally, and where usage is kept.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;

use futures::future::BoxFuture;
use hud_domain::usage::{PlanUsage, Provider, ToolCall, UsageRecord};
use jiff::Timestamp;

use crate::ports::store::StoreError;

/// How far one log file has been read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileCursor {
    /// File size when it was last read.
    pub size: u64,
    /// Bytes read so far; always the end of a complete line.
    pub offset: u64,
    /// Lines that looked like usage but could not be parsed.
    pub unreadable: u64,
}

/// Cursors by file path.
pub type Cursors = BTreeMap<String, FileCursor>;

/// What a source read since the given cursors.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsageBatch {
    /// New or updated responses; the same key may appear more than once.
    pub records: Vec<UsageRecord>,
    /// Tool calls in the new lines.
    pub tool_calls: Vec<ToolCall>,
    /// Cursors of every file after reading.
    pub cursors: Cursors,
    /// The newest subscription usage in the new lines, if the source reports it.
    pub plan: Option<PlanUsage>,
}

/// Why a source could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UsageError {
    /// The log folder does not exist, e.g. the tool was never used.
    #[error("no logs at {0}")]
    NoLogs(String),
    /// Reading failed.
    #[error("cannot read usage logs: {0}")]
    Io(String),
}

/// Reads AI usage from local logs, continuing where it stopped.
///
/// Dyn-compatible because all sources are held in one list (ADR 0002).
pub trait UsageSource: Send + Sync {
    /// Which provider the logs belong to.
    fn provider(&self) -> Provider;

    /// Reads what was appended since `cursors`. A file shorter than its
    /// cursor was rewritten and is read again from the start.
    fn read(&self, cursors: Cursors) -> BoxFuture<'_, Result<UsageBatch, UsageError>>;
}

/// Keeps usage records and the read progress.
pub trait UsageStore: Send + Sync {
    /// Read progress of `provider`'s files.
    fn cursors(
        &self,
        provider: Provider,
    ) -> impl Future<Output = Result<Cursors, StoreError>> + Send;

    /// Stores a batch: records are merged by key, keeping the highest count
    /// of each kind of token, the cursors are replaced and the plan usage is
    /// kept if it is newer than the stored one, all at once.
    fn save(
        &self,
        provider: Provider,
        batch: UsageBatch,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// Records answered in `[from, to)`.
    fn records(
        &self,
        from: Timestamp,
        to: Timestamp,
    ) -> impl Future<Output = Result<Vec<UsageRecord>, StoreError>> + Send;

    /// Answer times of `provider` in `[from, to)`, for the activity calendar.
    fn answer_times(
        &self,
        provider: Provider,
        from: Timestamp,
        to: Timestamp,
    ) -> impl Future<Output = Result<Vec<Timestamp>, StoreError>> + Send;

    /// Distinct tool calls of `provider` in `[from, to)`.
    fn tool_calls(
        &self,
        provider: Provider,
        from: Timestamp,
        to: Timestamp,
    ) -> impl Future<Output = Result<u64, StoreError>> + Send;

    /// The newest subscription usage `provider` reported.
    fn plan(
        &self,
        provider: Provider,
    ) -> impl Future<Output = Result<Option<PlanUsage>, StoreError>> + Send;
}

impl<T: UsageStore> UsageStore for Arc<T> {
    fn cursors(
        &self,
        provider: Provider,
    ) -> impl Future<Output = Result<Cursors, StoreError>> + Send {
        (**self).cursors(provider)
    }

    fn save(
        &self,
        provider: Provider,
        batch: UsageBatch,
    ) -> impl Future<Output = Result<(), StoreError>> + Send {
        (**self).save(provider, batch)
    }

    fn records(
        &self,
        from: Timestamp,
        to: Timestamp,
    ) -> impl Future<Output = Result<Vec<UsageRecord>, StoreError>> + Send {
        (**self).records(from, to)
    }

    fn tool_calls(
        &self,
        provider: Provider,
        from: Timestamp,
        to: Timestamp,
    ) -> impl Future<Output = Result<u64, StoreError>> + Send {
        (**self).tool_calls(provider, from, to)
    }

    fn plan(
        &self,
        provider: Provider,
    ) -> impl Future<Output = Result<Option<PlanUsage>, StoreError>> + Send {
        (**self).plan(provider)
    }

    fn answer_times(
        &self,
        provider: Provider,
        from: Timestamp,
        to: Timestamp,
    ) -> impl Future<Output = Result<Vec<Timestamp>, StoreError>> + Send {
        (**self).answer_times(provider, from, to)
    }
}
