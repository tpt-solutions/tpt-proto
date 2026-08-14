//! Observability: metrics, tracing, and structured logging for gRPC calls.
//!
//! The types here are transport-agnostic and pluggable. The server and client
//! runtime (and the `tpt-grpc` debug CLI) record call lifecycles through an
//! [`Observability`] bundle, which forwards to user-supplied [`MetricsRecorder`],
//! [`Tracer`], and [`Logger`] implementations. `Noop*` defaults are provided so
//! the stack works with zero configuration; [`InMemoryMetricsRecorder`] is
//! useful for tests and in-process aggregation.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::status::Code;

/// The streaming shape of a call, used as a metrics/logging label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamingType {
    /// Single request, single response.
    Unary,
    /// Single request, streaming response.
    ServerStreaming,
    /// Streaming request, single response.
    ClientStreaming,
    /// Streaming request, streaming response.
    BidiStreaming,
}

impl StreamingType {
    /// Classify from the request-side and response-side streaming flags.
    pub fn from_flags(client_streaming: bool, server_streaming: bool) -> StreamingType {
        match (client_streaming, server_streaming) {
            (false, false) => StreamingType::Unary,
            (false, true) => StreamingType::ServerStreaming,
            (true, false) => StreamingType::ClientStreaming,
            (true, true) => StreamingType::BidiStreaming,
        }
    }

    /// A stable short label for metrics/logs.
    pub fn as_str(self) -> &'static str {
        match self {
            StreamingType::Unary => "unary",
            StreamingType::ServerStreaming => "server_streaming",
            StreamingType::ClientStreaming => "client_streaming",
            StreamingType::BidiStreaming => "bidi_streaming",
        }
    }
}

/// Labels identifying a call, recorded with every metric and log line.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallLabels {
    /// The service fully-qualified name (e.g. `example.UserService`).
    pub service: String,
    /// The method name (e.g. `GetUser`).
    pub method: String,
    /// The terminal gRPC status code.
    pub status: Code,
    /// The streaming shape of the call.
    pub streaming_type: StreamingType,
}

impl CallLabels {
    /// Construct labels from their parts.
    pub fn new(
        service: impl Into<String>,
        method: impl Into<String>,
        status: Code,
        streaming_type: StreamingType,
    ) -> Self {
        CallLabels {
            service: service.into(),
            method: method.into(),
            status,
            streaming_type,
        }
    }
}

/// The terminal outcome of a call, recorded by the metrics layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallOutcome {
    /// Completed successfully (`grpc-status` 0).
    Success,
    /// Terminated with an error status.
    Error(Code),
    /// The peer cancelled the call.
    Cancelled,
    /// The deadline expired before completion.
    DeadlineExceeded,
}

impl CallOutcome {
    /// Classify from a terminal status, distinguishing cancellation and
    /// deadline-exceeded for dedicated counters.
    pub fn from_status(status: Code) -> CallOutcome {
        match status {
            Code::Ok => CallOutcome::Success,
            Code::Cancelled => CallOutcome::Cancelled,
            Code::DeadlineExceeded => CallOutcome::DeadlineExceeded,
            other => CallOutcome::Error(other),
        }
    }

    /// The label used for the `outcome` dimension.
    pub fn as_str(self) -> &'static str {
        match self {
            CallOutcome::Success => "success",
            CallOutcome::Cancelled => "cancelled",
            CallOutcome::DeadlineExceeded => "deadline_exceeded",
            CallOutcome::Error(_) => "error",
        }
    }
}

/// A sink for per-call metrics. All counters are monotonic.
pub trait MetricsRecorder: Send + Sync {
    /// A call was accepted/started.
    fn record_call_started(&self, labels: &CallLabels);
    /// A call completed (after start).
    fn record_call_completed(&self, labels: &CallLabels, duration: Duration, outcome: CallOutcome);
    /// A stream (server- or client-streaming call) was opened.
    fn record_stream_started(&self, labels: &CallLabels);
    /// A stream was closed.
    fn record_stream_closed(&self, labels: &CallLabels);
    /// A message was sent (wire bytes already framed).
    fn record_message_sent(&self, labels: &CallLabels, count: u64);
    /// A message was received.
    fn record_message_recv(&self, labels: &CallLabels, count: u64);
    /// Bytes were sent on the wire.
    fn record_bytes_sent(&self, labels: &CallLabels, bytes: u64);
    /// Bytes were received on the wire.
    fn record_bytes_recv(&self, labels: &CallLabels, bytes: u64);
    /// The call was cancelled by the peer or a downstream task.
    fn record_cancelled(&self, labels: &CallLabels);
    /// The call's deadline expired.
    fn record_deadline_exceeded(&self, labels: &CallLabels);
    /// A transport-level connection error occurred.
    fn record_connection_error(&self, labels: &CallLabels);
    /// A stream was reset abnormally.
    fn record_stream_reset(&self, labels: &CallLabels);
}

/// A no-op metrics recorder.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopMetricsRecorder;

impl MetricsRecorder for NoopMetricsRecorder {
    fn record_call_started(&self, _: &CallLabels) {}
    fn record_call_completed(&self, _: &CallLabels, _: Duration, _: CallOutcome) {}
    fn record_stream_started(&self, _: &CallLabels) {}
    fn record_stream_closed(&self, _: &CallLabels) {}
    fn record_message_sent(&self, _: &CallLabels, _: u64) {}
    fn record_message_recv(&self, _: &CallLabels, _: u64) {}
    fn record_bytes_sent(&self, _: &CallLabels, _: u64) {}
    fn record_bytes_recv(&self, _: &CallLabels, _: u64) {}
    fn record_cancelled(&self, _: &CallLabels) {}
    fn record_deadline_exceeded(&self, _: &CallLabels) {}
    fn record_connection_error(&self, _: &CallLabels) {}
    fn record_stream_reset(&self, _: &CallLabels) {}
}

/// A single point-in-time metric sample, for in-memory aggregation.
#[derive(Debug, Clone, Default)]
struct MetricSample {
    call_started: u64,
    call_completed: u64,
    stream_started: u64,
    stream_closed: u64,
    message_sent: u64,
    message_recv: u64,
    bytes_sent: u64,
    bytes_recv: u64,
    cancelled: u64,
    deadline_exceeded: u64,
    connection_error: u64,
    stream_reset: u64,
    completed_duration_micros: u64,
}

/// An in-memory [`MetricsRecorder`] that aggregates counts per label set.
///
/// Useful for tests, local aggregation, and exposing metrics to a higher-level
/// monitoring system.
#[derive(Debug, Clone, Default)]
pub struct InMemoryMetricsRecorder {
    inner: Arc<Mutex<std::collections::HashMap<CallLabels, MetricSample>>>,
}

impl InMemoryMetricsRecorder {
    /// Create an empty recorder.
    pub fn new() -> Self {
        InMemoryMetricsRecorder::default()
    }

    fn with_sample(&self, labels: &CallLabels, f: impl FnOnce(&mut MetricSample)) {
        let mut map = self.inner.lock().unwrap();
        f(map.entry(labels.clone()).or_default());
    }

    /// Snapshot the aggregated counts keyed by [`CallLabels`].
    pub fn snapshot(&self) -> std::collections::HashMap<CallLabels, MetricSampleView> {
        let map = self.inner.lock().unwrap();
        map.iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    MetricSampleView {
                        call_started: v.call_started,
                        call_completed: v.call_completed,
                        stream_started: v.stream_started,
                        stream_closed: v.stream_closed,
                        message_sent: v.message_sent,
                        message_recv: v.message_recv,
                        bytes_sent: v.bytes_sent,
                        bytes_recv: v.bytes_recv,
                        cancelled: v.cancelled,
                        deadline_exceeded: v.deadline_exceeded,
                        connection_error: v.connection_error,
                        stream_reset: v.stream_reset,
                        total_duration: Duration::from_micros(v.completed_duration_micros),
                    },
                )
            })
            .collect()
    }

    /// Total calls started across all label sets.
    pub fn total_calls_started(&self) -> u64 {
        self.inner
            .lock()
            .unwrap()
            .values()
            .map(|v| v.call_started)
            .sum()
    }
}

/// A read-only view of an aggregated metric sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricSampleView {
    /// Calls started.
    pub call_started: u64,
    /// Calls completed.
    pub call_completed: u64,
    /// Streams opened.
    pub stream_started: u64,
    /// Streams closed.
    pub stream_closed: u64,
    /// Messages sent.
    pub message_sent: u64,
    /// Messages received.
    pub message_recv: u64,
    /// Bytes sent.
    pub bytes_sent: u64,
    /// Bytes received.
    pub bytes_recv: u64,
    /// Cancellations.
    pub cancelled: u64,
    /// Deadline-exceeded events.
    pub deadline_exceeded: u64,
    /// Connection errors.
    pub connection_error: u64,
    /// Stream resets.
    pub stream_reset: u64,
    /// Sum of completed-call durations.
    pub total_duration: Duration,
}

impl MetricsRecorder for InMemoryMetricsRecorder {
    fn record_call_started(&self, labels: &CallLabels) {
        self.with_sample(labels, |s| s.call_started += 1);
    }
    fn record_call_completed(&self, labels: &CallLabels, duration: Duration, _outcome: CallOutcome) {
        self.with_sample(labels, |s| {
            s.call_completed += 1;
            s.completed_duration_micros += duration.as_micros() as u64;
        });
    }
    fn record_stream_started(&self, labels: &CallLabels) {
        self.with_sample(labels, |s| s.stream_started += 1);
    }
    fn record_stream_closed(&self, labels: &CallLabels) {
        self.with_sample(labels, |s| s.stream_closed += 1);
    }
    fn record_message_sent(&self, labels: &CallLabels, count: u64) {
        self.with_sample(labels, |s| s.message_sent += count);
    }
    fn record_message_recv(&self, labels: &CallLabels, count: u64) {
        self.with_sample(labels, |s| s.message_recv += count);
    }
    fn record_bytes_sent(&self, labels: &CallLabels, bytes: u64) {
        self.with_sample(labels, |s| s.bytes_sent += bytes);
    }
    fn record_bytes_recv(&self, labels: &CallLabels, bytes: u64) {
        self.with_sample(labels, |s| s.bytes_recv += bytes);
    }
    fn record_cancelled(&self, labels: &CallLabels) {
        self.with_sample(labels, |s| s.cancelled += 1);
    }
    fn record_deadline_exceeded(&self, labels: &CallLabels) {
        self.with_sample(labels, |s| s.deadline_exceeded += 1);
    }
    fn record_connection_error(&self, labels: &CallLabels) {
        self.with_sample(labels, |s| s.connection_error += 1);
    }
    fn record_stream_reset(&self, labels: &CallLabels) {
        self.with_sample(labels, |s| s.stream_reset += 1);
    }
}

/// A span identifier carried through a call for tracing.
#[derive(Debug, Clone)]
pub struct SpanContext {
    /// A stable span id (e.g. hex). Optional.
    pub span_id: Option<String>,
    /// The correlation/request id propagated across hops.
    pub request_id: Option<String>,
    /// `rpc.system`, always `grpc`.
    pub rpc_system: &'static str,
    /// `rpc.service`.
    pub service: String,
    /// `rpc.method`.
    pub method: String,
    /// `rpc.grpc.status_code` (numeric; 0 = ok).
    pub status_code: i32,
    /// Peer address, when known.
    pub peer: Option<String>,
}

impl SpanContext {
    /// Build a span context for a call.
    pub fn new(service: impl Into<String>, method: impl Into<String>) -> Self {
        SpanContext {
            span_id: None,
            request_id: None,
            rpc_system: "grpc",
            service: service.into(),
            method: method.into(),
            status_code: 0,
            peer: None,
        }
    }

    /// Set the terminal status code on the span.
    pub fn with_status(mut self, code: Code) -> Self {
        self.status_code = code.as_i32();
        self
    }

    /// Set the propagated request id.
    pub fn with_request_id(mut self, id: impl Into<String>) -> Self {
        self.request_id = Some(id.into());
        self
    }
}

/// A sink for span lifecycle events.
pub trait Tracer: Send + Sync {
    /// A span was opened.
    fn start_span(&self, span: &SpanContext);
    /// A span was closed.
    fn end_span(&self, span: &SpanContext);
}

/// A no-op tracer.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopTracer;

impl Tracer for NoopTracer {
    fn start_span(&self, _: &SpanContext) {}
    fn end_span(&self, _: &SpanContext) {}
}

/// A tracer that writes span lifecycle events to stderr with no external
/// logging dependency.
#[derive(Debug, Default, Clone, Copy)]
pub struct LoggingTracer;

impl Tracer for LoggingTracer {
    fn start_span(&self, span: &SpanContext) {
        eprintln!(
            "[span:start] system={} service={} method={} span_id={:?} request_id={:?}",
            span.rpc_system, span.service, span.method, span.span_id, span.request_id
        );
    }
    fn end_span(&self, span: &SpanContext) {
        eprintln!(
            "[span:end] system={} service={} method={} status_code={} span_id={:?}",
            span.rpc_system, span.service, span.method, span.status_code, span.span_id
        );
    }
}

/// Severity of a structured log record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Debug-level diagnostics.
    Debug,
    /// Informational.
    Info,
    /// Warning.
    Warn,
    /// Error.
    Error,
}

/// A structured log record emitted for a call.
#[derive(Debug, Clone)]
pub struct LogRecord {
    /// Severity.
    pub level: Level,
    /// Free-form human message.
    pub message: String,
    /// Correlation/request id.
    pub request_id: Option<String>,
    /// `rpc.service`.
    pub service: String,
    /// `rpc.method`.
    pub method: String,
    /// Terminal status code name (e.g. `ok`, `not_found`).
    pub status: String,
    /// Call deadline, when applicable.
    pub deadline: Option<SystemTime>,
    /// Peer address, when known.
    pub peer: Option<String>,
    /// Reason the call was cancelled, when applicable.
    pub cancellation_reason: Option<String>,
    /// Span id, when known.
    pub span_id: Option<String>,
}

/// A sink for structured log records.
pub trait Logger: Send + Sync {
    /// Emit a log record.
    fn log(&self, record: &LogRecord);
}

/// A no-op logger.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopLogger;

impl Logger for NoopLogger {
    fn log(&self, _: &LogRecord) {}
}

/// A logger that emits one JSON object per record on stderr.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonLogger;

impl Logger for JsonLogger {
    fn log(&self, r: &LogRecord) {
        let mut s = String::new();
        s.push_str("{\"tpt_grpc_log\":true,\"level\":");
        s.push_str(match r.level {
            Level::Debug => "\"debug\"",
            Level::Info => "\"info\"",
            Level::Warn => "\"warn\"",
            Level::Error => "\"error\"",
        });
        s.push_str(",\"message\":");
        push_json_string(&mut s, &r.message);
        if let Some(id) = &r.request_id {
            s.push_str(",\"request_id\":");
            push_json_string(&mut s, id);
        }
        s.push_str(",\"service\":");
        push_json_string(&mut s, &r.service);
        s.push_str(",\"method\":");
        push_json_string(&mut s, &r.method);
        s.push_str(",\"status\":");
        push_json_string(&mut s, &r.status);
        if let Some(d) = r.deadline {
            let secs = d
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            s.push_str(&format!(",\"deadline_unix_secs\":{secs}"));
        }
        if let Some(p) = &r.peer {
            s.push_str(",\"peer\":");
            push_json_string(&mut s, p);
        }
        if let Some(c) = &r.cancellation_reason {
            s.push_str(",\"cancellation_reason\":");
            push_json_string(&mut s, c);
        }
        if let Some(sp) = &r.span_id {
            s.push_str(",\"span_id\":");
            push_json_string(&mut s, sp);
        }
        s.push('}');
        eprintln!("{s}");
    }
}

fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A bundle of observability sinks used by the runtime and CLI.
///
/// Each sink has a sensible default (`Noop*`), so a call site can construct a
/// bundle with only the sinks it cares about.
#[derive(Clone)]
pub struct Observability {
    /// Metrics sink.
    pub metrics: Arc<dyn MetricsRecorder>,
    /// Tracing sink.
    pub tracer: Arc<dyn Tracer>,
    /// Logging sink.
    pub logger: Arc<dyn Logger>,
}

impl std::fmt::Debug for Observability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Observability").finish_non_exhaustive()
    }
}

impl Default for Observability {
    fn default() -> Self {
        Observability {
            metrics: Arc::new(NoopMetricsRecorder),
            tracer: Arc::new(NoopTracer),
            logger: Arc::new(NoopLogger),
        }
    }
}

impl Observability {
    /// Create a bundle from explicit sinks.
    pub fn new(
        metrics: Arc<dyn MetricsRecorder>,
        tracer: Arc<dyn Tracer>,
        logger: Arc<dyn Logger>,
    ) -> Self {
        Observability {
            metrics,
            tracer,
            logger,
        }
    }

    /// Replace the metrics sink.
    pub fn with_metrics(mut self, metrics: Arc<dyn MetricsRecorder>) -> Self {
        self.metrics = metrics;
        self
    }

    /// Replace the tracer sink.
    pub fn with_tracer(mut self, tracer: Arc<dyn Tracer>) -> Self {
        self.tracer = tracer;
        self
    }

    /// Replace the logger sink.
    pub fn with_logger(mut self, logger: Arc<dyn Logger>) -> Self {
        self.logger = logger;
        self
    }
}

/// An instrumentor that records the lifecycle of a single call.
///
/// Construct with [`CallInstrumentor::start`], then call
/// [`finish`](CallInstrumentor::finish) (or [`finish_with_status`]) once. The
/// start time, duration, and outcome are derived automatically.
pub struct CallInstrumentor {
    labels: CallLabels,
    started: Instant,
    obs: Observability,
    span: Option<SpanContext>,
    finished: bool,
}

impl CallInstrumentor {
    /// Begin instrumenting a call with the given labels and observability.
    pub fn start(labels: CallLabels, obs: Observability) -> Self {
        obs.metrics.record_call_started(&labels);
        if labels.streaming_type != StreamingType::Unary {
            obs.metrics.record_stream_started(&labels);
        }
        let span = SpanContext::new(labels.service.clone(), labels.method.clone());
        obs.tracer.start_span(&span);
        CallInstrumentor {
            labels,
            started: Instant::now(),
            obs,
            span: Some(span),
            finished: false,
        }
    }

    /// Record that a single message was sent.
    pub fn message_sent(&self, bytes: u64) {
        self.obs.metrics.record_message_sent(&self.labels, 1);
        self.obs.metrics.record_bytes_sent(&self.labels, bytes);
    }

    /// Record that a single message was received.
    pub fn message_recv(&self, bytes: u64) {
        self.obs.metrics.record_message_recv(&self.labels, 1);
        self.obs
            .metrics
            .record_bytes_recv(&self.labels, bytes);
    }

    /// Record peer-initiated cancellation.
    pub fn cancelled(&self) {
        self.obs.metrics.record_cancelled(&self.labels);
    }

    /// Record deadline expiry.
    pub fn deadline_exceeded(&self) {
        self.obs.metrics.record_deadline_exceeded(&self.labels);
    }

    /// Record an abnormal stream reset.
    pub fn stream_reset(&self) {
        self.obs.metrics.record_stream_reset(&self.labels);
    }

    /// Record a connection error.
    pub fn connection_error(&self) {
        self.obs.metrics.record_connection_error(&self.labels);
    }

    /// Finish the call, recording duration and outcome from `status`.
    pub fn finish_with_status(&mut self, status: Code) {
        if self.finished {
            return;
        }
        self.finished = true;
        let duration = self.started.elapsed();
        let outcome = CallOutcome::from_status(status);
        self.labels.status = status;
        self.obs
            .metrics
            .record_call_completed(&self.labels, duration, outcome);
        if self.labels.streaming_type != StreamingType::Unary {
            self.obs.metrics.record_stream_closed(&self.labels);
        }
        if let Some(mut span) = self.span.take() {
            span = span.with_status(status);
            self.obs.tracer.end_span(&span);
        }
    }

    /// Finish the call as a success.
    pub fn finish(&mut self) {
        self.finish_with_status(Code::Ok);
    }
}

impl Drop for CallInstrumentor {
    fn drop(&mut self) {
        if !self.finished {
            self.finish();
        }
    }
}

/// A bounded rolling buffer of recent request ids for debugging.
#[derive(Debug, Clone, Default)]
pub struct RequestIdLog {
    inner: Arc<Mutex<VecDeque<String>>>,
    cap: usize,
}

impl RequestIdLog {
    /// Create a log retaining up to `cap` ids.
    pub fn new(cap: usize) -> Self {
        RequestIdLog {
            inner: Arc::new(Mutex::new(VecDeque::new())),
            cap: cap.max(1),
        }
    }

    /// Record a request id as seen.
    pub fn push(&self, id: impl Into<String>) {
        let mut q = self.inner.lock().unwrap();
        q.push_back(id.into());
        while q.len() > self.cap {
            q.pop_front();
        }
    }

    /// Most recent ids, oldest first.
    pub fn recent(&self) -> Vec<String> {
        self.inner.lock().unwrap().iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_type_classify() {
        assert_eq!(
            StreamingType::from_flags(false, false),
            StreamingType::Unary
        );
        assert_eq!(
            StreamingType::from_flags(false, true),
            StreamingType::ServerStreaming
        );
        assert_eq!(
            StreamingType::from_flags(true, false),
            StreamingType::ClientStreaming
        );
        assert_eq!(
            StreamingType::from_flags(true, true),
            StreamingType::BidiStreaming
        );
    }

    #[test]
    fn outcome_from_status() {
        assert_eq!(CallOutcome::from_status(Code::Ok), CallOutcome::Success);
        assert_eq!(
            CallOutcome::from_status(Code::Cancelled),
            CallOutcome::Cancelled
        );
        assert_eq!(
            CallOutcome::from_status(Code::DeadlineExceeded),
            CallOutcome::DeadlineExceeded
        );
        assert_eq!(
            CallOutcome::from_status(Code::NotFound),
            CallOutcome::Error(Code::NotFound)
        );
    }

    #[test]
    fn instrumentor_records_completion() {
        let metrics = InMemoryMetricsRecorder::new();
        let obs = Observability::default().with_metrics(Arc::new(metrics.clone()));
        let labels = CallLabels::new("ex.Svc", "Do", Code::Ok, StreamingType::Unary);
        {
            let mut inst = CallInstrumentor::start(labels.clone(), obs);
            inst.message_sent(10);
            inst.finish_with_status(Code::NotFound);
        }
        let snap = metrics.snapshot();
        let s = snap.get(&labels).expect("sample present");
        assert_eq!(s.call_started, 1);
        assert_eq!(s.call_completed, 1);
        assert_eq!(s.message_sent, 1);
        assert_eq!(s.bytes_sent, 10);
        assert_eq!(s.cancelled, 0);
        assert_eq!(s.deadline_exceeded, 0);
    }

    #[test]
    fn instrumentor_records_cancellation_and_deadline() {
        let metrics = InMemoryMetricsRecorder::new();
        let obs = Observability::default().with_metrics(Arc::new(metrics.clone()));
        let labels = CallLabels::new("ex.Svc", "Do", Code::Ok, StreamingType::Unary);
        let mut inst = CallInstrumentor::start(labels.clone(), obs);
        inst.cancelled();
        inst.deadline_exceeded();
        inst.finish_with_status(Code::Cancelled);
        let s = metrics.snapshot().get(&labels).unwrap().clone();
        assert_eq!(s.cancelled, 1);
        assert_eq!(s.deadline_exceeded, 1);
        assert_eq!(s.call_completed, 1);
    }

    #[test]
    fn drop_finishes_call() {
        let metrics = InMemoryMetricsRecorder::new();
        let obs = Observability::default().with_metrics(Arc::new(metrics.clone()));
        let labels = CallLabels::new("ex.Svc", "Do", Code::Ok, StreamingType::Unary);
        {
            let _inst = CallInstrumentor::start(labels.clone(), obs);
        }
        assert_eq!(metrics.total_calls_started(), 1);
        assert_eq!(metrics.snapshot().get(&labels).unwrap().call_completed, 1);
    }

    #[test]
    fn json_logger_emits_valid_shape() {
        let logger = JsonLogger;
        let rec = LogRecord {
            level: Level::Info,
            message: "done",
            request_id: Some("abc".into()),
            service: "ex.Svc".into(),
            method: "Do".into(),
            status: "ok".into(),
            deadline: None,
            peer: Some("127.0.0.1:50051".into()),
            cancellation_reason: None,
            span_id: None,
        };
        logger.log(&rec);
    }

    #[test]
    fn request_id_log_bounded() {
        let log = RequestIdLog::new(3);
        for i in 0..5 {
            log.push(format!("id{i}"));
        }
        let recent = log.recent();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0], "id2");
        assert_eq!(recent[2], "id4");
    }
}
