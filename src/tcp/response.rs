use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseEnvelope<T = Value> {
    /// True if the request was handled successfully
    pub ok: bool,
    /// Echo the client's request_id when available (None for invalid JSON, etc.)
    pub request_id: Option<String>,
    /// High-level kind of response (helps clients route)
    pub kind: ResponseKind,
    /// Which provider this pertains to (when applicable)
    pub provider: Option<String>,
    /// Which request variant within provider (e.g., "GetEntity")
    pub request_kind: Option<String>,
    /// Successful result payload; shape depends on `kind`
    pub result: Option<T>,
    /// Error details when `ok == false`
    pub error: Option<ResponseError>,
    /// Server timestamp (ms since epoch)
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseKind {
    ProviderList,
    ProviderRequest,
    InvalidJson,
}

impl<T> ResponseEnvelope<T> {
    pub fn new_ok(
        request_id: Option<String>,
        kind: ResponseKind,
        provider: Option<String>,
        request_kind: Option<String>,
        result: T,
    ) -> Self {
        ResponseEnvelope {
            ok: true,
            request_id,
            kind,
            provider,
            request_kind,
            result: Some(result),
            error: None,
            ts_ms: now_ms(),
        }
    }

    pub fn new_err(
        request_id: Option<String>,
        kind: ResponseKind,
        provider: Option<String>,
        request_kind: Option<String>,
        code: Option<String>,
        message: String,
    ) -> Self {
        ResponseEnvelope {
            ok: false,
            request_id,
            kind,
            provider,
            request_kind,
            result: None,
            error: Some(ResponseError { code, message }),
            ts_ms: now_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseError {
    /// Optional machine-readable code, e.g. "provider_not_found"
    pub code: Option<String>,
    /// Human-readable message
    pub message: String,
}
pub fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
