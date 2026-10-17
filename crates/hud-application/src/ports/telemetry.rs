//! Telemetry from Claude Code, received while hud runs (ADR 0011).

use std::future::Future;
use std::sync::Arc;

use hud_domain::usage::{TelemetryPoint, TelemetryTotals};
use jiff::Timestamp;

use crate::ports::store::StoreError;

/// Keeps telemetry data points.
pub trait TelemetryStore: Send + Sync {
    /// Stores received points.
    fn save_points(
        &self,
        points: Vec<TelemetryPoint>,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// Sums of the points in `[from, to)`.
    fn totals(
        &self,
        from: Timestamp,
        to: Timestamp,
    ) -> impl Future<Output = Result<TelemetryTotals, StoreError>> + Send;
}

impl<T: TelemetryStore> TelemetryStore for Arc<T> {
    fn save_points(
        &self,
        points: Vec<TelemetryPoint>,
    ) -> impl Future<Output = Result<(), StoreError>> + Send {
        (**self).save_points(points)
    }

    fn totals(
        &self,
        from: Timestamp,
        to: Timestamp,
    ) -> impl Future<Output = Result<TelemetryTotals, StoreError>> + Send {
        (**self).totals(from, to)
    }
}

/// Whether the receiver could start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiverState {
    /// Telemetry is switched off in the config.
    Off,
    /// Listening on `127.0.0.1` at this port.
    Listening(u16),
    /// The port could not be opened, e.g. because it is taken.
    Unavailable {
        /// Port.
        port: u16,
        /// Why.
        reason: String,
    },
}

/// Whether Claude Code sends its metrics to hud.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeTelemetry {
    /// Claude Code is set up to send metrics to this receiver.
    Enabled,
    /// Not set up; hud can add the entries.
    Disabled,
    /// Existing entries point elsewhere; hud does not overwrite them.
    Conflict(Vec<String>),
    /// The settings file could not be read or parsed.
    Unreadable(String),
}

/// Environment entries that make Claude Code send its metrics to hud on `port`.
///
/// Never includes `OTEL_LOG_USER_PROMPTS` or anything that exports content.
#[must_use]
pub fn telemetry_entries(port: u16) -> Vec<(&'static str, String)> {
    vec![
        ("CLAUDE_CODE_ENABLE_TELEMETRY", "1".to_owned()),
        ("OTEL_METRICS_EXPORTER", "otlp".to_owned()),
        (
            "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL",
            "http/json".to_owned(),
        ),
        (
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
            format!("http://127.0.0.1:{port}/v1/metrics"),
        ),
    ]
}

/// Variables that would send metrics elsewhere or in a form hud ignores.
const CONFLICTING: [&str; 2] = [
    "OTEL_EXPORTER_OTLP_ENDPOINT",
    "OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE",
];

/// Compares the `env` entries of Claude Code's settings with what hud needs.
#[must_use]
pub fn classify(env: &[(String, String)], port: u16) -> ClaudeTelemetry {
    let get = |key: &str| env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let mut conflicts = Vec::new();
    let mut missing = false;
    for (key, wanted) in telemetry_entries(port) {
        match get(key) {
            None => missing = true,
            Some(value) if value == wanted => {}
            // Several exporters are allowed, as long as OTLP is one of them.
            Some(value)
                if key == "OTEL_METRICS_EXPORTER"
                    && value.split(',').any(|e| e.trim() == "otlp") => {}
            Some(value) => conflicts.push(format!("{key}={value}")),
        }
    }
    for key in CONFLICTING {
        if let Some(value) = get(key)
            && !(key.ends_with("PREFERENCE") && value == "delta")
        {
            conflicts.push(format!("{key}={value}"));
        }
    }
    match (conflicts.is_empty(), missing) {
        (false, _) => ClaudeTelemetry::Conflict(conflicts),
        (true, true) => ClaudeTelemetry::Disabled,
        (true, false) => ClaudeTelemetry::Enabled,
    }
}

/// Claude Code's user settings file (`~/.claude/settings.json`).
pub trait ClaudeSettings: Send + Sync {
    /// The `env` entries as they are now; a missing file has none.
    fn env(&self) -> impl Future<Output = Result<Vec<(String, String)>, String>> + Send;

    /// Adds `entries` to `env`, keeping every other key, its order and value;
    /// writes atomically and only after reading the file again.
    fn add_env(
        &self,
        entries: Vec<(&'static str, String)>,
    ) -> impl Future<Output = Result<(), String>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn all(port: u16) -> Vec<(String, String)> {
        telemetry_entries(port)
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect()
    }

    #[test]
    fn empty_env_is_disabled() {
        assert_eq!(classify(&[], 4318), ClaudeTelemetry::Disabled);
    }

    #[test]
    fn all_entries_present_is_enabled() {
        assert_eq!(classify(&all(4318), 4318), ClaudeTelemetry::Enabled);
    }

    #[test]
    fn another_port_is_a_conflict() {
        assert!(matches!(
            classify(&all(4318), 9999),
            ClaudeTelemetry::Conflict(_)
        ));
    }

    #[test]
    fn an_own_collector_is_not_overwritten() {
        let state = classify(
            &env(&[("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otel.example.com")]),
            4318,
        );
        assert!(
            matches!(state, ClaudeTelemetry::Conflict(keys) if keys[0].starts_with("OTEL_EXPORTER_OTLP_ENDPOINT"))
        );
    }

    #[test]
    fn an_extra_exporter_next_to_otlp_is_fine() {
        let mut entries = all(4318);
        entries[1].1 = "otlp,prometheus".into();
        assert_eq!(classify(&entries, 4318), ClaudeTelemetry::Enabled);
    }

    #[test]
    fn entries_never_export_content() {
        assert!(
            telemetry_entries(4318)
                .iter()
                .all(|(k, _)| !k.starts_with("OTEL_LOG_"))
        );
    }
}
