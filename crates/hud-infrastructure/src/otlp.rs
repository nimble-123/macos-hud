//! OTLP/HTTP+JSON receiver for Claude Code metrics on `127.0.0.1` (ADR 0011).
//!
//! Only metric name, value, time, `session.id`, `model` and `type` survive
//! parsing; every other attribute, including `user.email`, is dropped here.

use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};

use http_body_util::{BodyExt, Full, Limited};
use hud_domain::usage::{ModelId, TelemetryMetric, TelemetryPoint};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use jiff::Timestamp;
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Requests above this size are rejected.
const BODY_LIMIT: usize = 4 * 1024 * 1024;

/// A bound, not yet running receiver.
#[derive(Debug)]
pub struct OtlpReceiver {
    listener: TcpListener,
}

impl OtlpReceiver {
    /// Opens `127.0.0.1:port`; only local processes can connect.
    ///
    /// # Errors
    ///
    /// Fails when the port is taken or not allowed.
    pub async fn bind(port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await?;
        Ok(Self { listener })
    }

    /// The port actually bound; differs from the requested one only for port 0.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.listener.local_addr().map_or(0, |addr| addr.port())
    }

    /// Serves until cancelled, sending parsed points to `sink`.
    pub async fn run(self, sink: mpsc::Sender<Vec<TelemetryPoint>>, cancel: CancellationToken) {
        loop {
            let accepted = tokio::select! {
                () = cancel.cancelled() => return,
                accepted = self.listener.accept() => accepted,
            };
            let stream = match accepted {
                Ok((stream, _)) => stream,
                Err(error) => {
                    tracing::debug!(%error, "telemetry connection failed");
                    continue;
                }
            };
            let sink = sink.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                let service = service_fn(move |request| handle(request, sink.clone()));
                let connection =
                    http1::Builder::new().serve_connection(TokioIo::new(stream), service);
                tokio::select! {
                    () = cancel.cancelled() => {}
                    result = connection => {
                        if let Err(error) = result {
                            tracing::debug!(%error, "telemetry connection closed");
                        }
                    }
                }
            });
        }
    }
}

async fn handle(
    request: Request<Incoming>,
    sink: mpsc::Sender<Vec<TelemetryPoint>>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = request.uri().path().to_owned();
    if request.method() != Method::POST {
        return Ok(reply(StatusCode::METHOD_NOT_ALLOWED));
    }
    let json = request
        .headers()
        .get(hyper::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    let compressed = request
        .headers()
        .contains_key(hyper::header::CONTENT_ENCODING);
    match path.as_str() {
        "/v1/metrics" if json && !compressed => {}
        "/v1/metrics" => return Ok(reply(StatusCode::UNSUPPORTED_MEDIA_TYPE)),
        // Logs and traces are accepted and dropped: hud does not evaluate them.
        "/v1/logs" | "/v1/traces" => return Ok(reply(StatusCode::OK)),
        _ => return Ok(reply(StatusCode::NOT_FOUND)),
    }
    let Ok(body) = Limited::new(request.into_body(), BODY_LIMIT)
        .collect()
        .await
    else {
        return Ok(reply(StatusCode::PAYLOAD_TOO_LARGE));
    };
    match parse_metrics(&body.to_bytes()) {
        Ok(points) => {
            if !points.is_empty() && sink.send(points).await.is_err() {
                return Ok(reply(StatusCode::SERVICE_UNAVAILABLE));
            }
            Ok(reply(StatusCode::OK))
        }
        Err(error) => {
            tracing::debug!(%error, "unreadable telemetry");
            Ok(reply(StatusCode::BAD_REQUEST))
        }
    }
}

fn reply(status: StatusCode) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from_static(b"{}")));
    *response.status_mut() = status;
    response.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("application/json"),
    );
    response
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportRequest {
    #[serde(default)]
    resource_metrics: Vec<ResourceMetrics>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceMetrics {
    #[serde(default)]
    scope_metrics: Vec<ScopeMetrics>,
}

#[derive(Deserialize)]
struct ScopeMetrics {
    #[serde(default)]
    metrics: Vec<Metric>,
}

#[derive(Deserialize)]
struct Metric {
    name: String,
    sum: Option<Sum>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sum {
    #[serde(default)]
    data_points: Vec<DataPoint>,
    /// 1 = delta, 2 = cumulative.
    #[serde(default)]
    aggregation_temporality: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataPoint {
    #[serde(default)]
    attributes: Vec<KeyValue>,
    time_unix_nano: Option<NumberOrString>,
    as_double: Option<f64>,
    as_int: Option<NumberOrString>,
}

#[derive(Deserialize)]
struct KeyValue {
    key: String,
    value: AnyValue,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnyValue {
    string_value: Option<String>,
}

/// Protobuf JSON writes 64-bit integers as strings, some exporters as numbers.
#[derive(Deserialize)]
#[serde(untagged)]
enum NumberOrString {
    Number(u64),
    Text(String),
}

impl NumberOrString {
    fn get(&self) -> Option<u64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Text(text) => text.parse().ok(),
        }
    }
}

const DELTA: u8 = 1;

/// Turns an `ExportMetricsServiceRequest` into data points; unknown metrics
/// and cumulative series are skipped.
///
/// # Errors
///
/// Fails when the body is not OTLP JSON.
pub fn parse_metrics(body: &[u8]) -> Result<Vec<TelemetryPoint>, String> {
    let request: ExportRequest = serde_json::from_slice(body).map_err(|error| error.to_string())?;
    let mut points = Vec::new();
    for metric in request
        .resource_metrics
        .iter()
        .flat_map(|r| &r.scope_metrics)
        .flat_map(|s| &s.metrics)
    {
        let Some(sum) = &metric.sum else { continue };
        if sum.aggregation_temporality != DELTA {
            continue;
        }
        for point in &sum.data_points {
            if let Some(parsed) = data_point(&metric.name, point) {
                points.push(parsed);
            }
        }
    }
    Ok(points)
}

fn data_point(name: &str, point: &DataPoint) -> Option<TelemetryPoint> {
    let attribute = |key: &str| {
        point
            .attributes
            .iter()
            .find(|kv| kv.key == key)
            .and_then(|kv| kv.value.string_value.clone())
    };
    let detail = attribute("type");
    #[expect(
        clippy::cast_precision_loss,
        reason = "counters stay far below 2^52 per export interval"
    )]
    let raw = point.as_double.or_else(|| {
        point
            .as_int
            .as_ref()
            .and_then(NumberOrString::get)
            .map(|v| v as f64)
    })?;
    let (metric, scale) = match (name, detail.as_deref()) {
        ("claude_code.cost.usage", _) => (TelemetryMetric::Cost, 1_000_000.0),
        ("claude_code.token.usage", _) => (TelemetryMetric::Tokens, 1.0),
        ("claude_code.active_time.total", _) => (TelemetryMetric::ActiveTime, 1_000.0),
        ("claude_code.lines_of_code.count", Some("added")) => (TelemetryMetric::LinesAdded, 1.0),
        ("claude_code.lines_of_code.count", Some("removed")) => {
            (TelemetryMetric::LinesRemoved, 1.0)
        }
        ("claude_code.commit.count", _) => (TelemetryMetric::Commits, 1.0),
        ("claude_code.pull_request.count", _) => (TelemetryMetric::PullRequests, 1.0),
        ("claude_code.session.count", _) => (TelemetryMetric::Sessions, 1.0),
        _ => return None,
    };
    if !raw.is_finite() || raw < 0.0 {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "finite and non-negative; deltas are far below u64::MAX"
    )]
    let value = (raw * scale).round() as u64;
    let at = point
        .time_unix_nano
        .as_ref()
        .and_then(NumberOrString::get)
        .and_then(|nanos| Timestamp::from_nanosecond(i128::from(nanos)).ok())
        .unwrap_or_else(Timestamp::now);
    Some(TelemetryPoint {
        at,
        metric,
        detail,
        model: attribute("model").and_then(|model| ModelId::try_from(model).ok()),
        session: attribute("session.id"),
        value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPORT: &str = r#"{
      "resourceMetrics": [{
        "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "claude-code"}}]},
        "scopeMetrics": [{
          "scope": {"name": "com.anthropic.claude_code"},
          "metrics": [
            {"name": "claude_code.cost.usage", "unit": "USD", "sum": {"aggregationTemporality": 1, "isMonotonic": true,
              "dataPoints": [{"attributes": [
                {"key": "user.email", "value": {"stringValue": "someone@example.com"}},
                {"key": "session.id", "value": {"stringValue": "s-1"}},
                {"key": "model", "value": {"stringValue": "claude-opus-5-5"}}],
                "timeUnixNano": "1792238400000000000", "asDouble": 0.1234567}]}},
            {"name": "claude_code.lines_of_code.count", "sum": {"aggregationTemporality": 1,
              "dataPoints": [{"attributes": [{"key": "type", "value": {"stringValue": "added"}}],
                "timeUnixNano": "1792238400000000000", "asInt": "42"}]}},
            {"name": "claude_code.active_time.total", "unit": "s", "sum": {"aggregationTemporality": 1,
              "dataPoints": [{"attributes": [{"key": "type", "value": {"stringValue": "user"}}],
                "timeUnixNano": 1792238400000000000, "asDouble": 12.5}]}},
            {"name": "claude_code.commit.count", "sum": {"aggregationTemporality": 2,
              "dataPoints": [{"timeUnixNano": "1792238400000000000", "asInt": "7"}]}},
            {"name": "some.other.metric", "sum": {"aggregationTemporality": 1,
              "dataPoints": [{"asInt": "1"}]}}
          ]
        }]
      }]
    }"#;

    #[test]
    fn known_delta_metrics_are_converted_and_others_dropped() {
        let points = parse_metrics(EXPORT.as_bytes()).expect("BUG: valid export");
        assert_eq!(
            points.len(),
            3,
            "cumulative commit count and unknown metric skipped"
        );
        let cost = &points[0];
        assert_eq!(cost.metric, TelemetryMetric::Cost);
        assert_eq!(cost.value, 123_457, "USD to micro-USD, rounded");
        assert_eq!(cost.session.as_deref(), Some("s-1"));
        assert_eq!(
            cost.model.as_ref().map(ModelId::as_str),
            Some("claude-opus-5-5")
        );
        assert_eq!(cost.at.as_second(), 1_792_238_400);
        assert_eq!(points[1].metric, TelemetryMetric::LinesAdded);
        assert_eq!(points[1].value, 42);
        assert_eq!(points[2].value, 12_500, "seconds to milliseconds");
    }

    #[test]
    fn no_other_attribute_survives_parsing() {
        let points = parse_metrics(EXPORT.as_bytes()).expect("BUG");
        let debug = format!("{points:?}");
        assert!(!debug.contains("example.com"), "{debug}");
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(parse_metrics(b"not json").is_err());
    }

    #[tokio::test]
    async fn the_receiver_accepts_metrics_over_http() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let receiver = OtlpReceiver::bind(0).await.expect("BUG: free port");
        let port = receiver.port();
        let (sink, mut points) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        let server = tokio::spawn(receiver.run(sink, cancel.clone()));

        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("BUG");
        let request = format!(
            "POST /v1/metrics HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{EXPORT}",
            EXPORT.len()
        );
        stream.write_all(request.as_bytes()).await.expect("BUG");
        let mut response = String::new();
        stream.read_to_string(&mut response).await.expect("BUG");
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert_eq!(points.recv().await.expect("BUG").len(), 3);
        cancel.cancel();
        server.await.expect("BUG");
    }
}
