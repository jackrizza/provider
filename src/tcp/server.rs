/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use axum::Router;
use axum::routing::{get, post};
use log::{error, info, warn};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::{
    io::{self, BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
};

use serde_json::{self, Value};

use crate::providers::Providers;
use crate::query::{EntityInProvider, QueryEnvelope, QueryEnvelopePayload};

// Bring the unified response into scope
use crate::tcp::response::{ResponseEnvelope, ResponseError, ResponseKind};

use crate::http::{AppState, http_list_providers, http_load_plugin, http_ping_provider};
use crate::pyadapter::add_dirs_to_syspath;

/// Simple TCP server: one thread per connection (blocking).
pub struct ProviderServer {
    address: String,
    http_address: String,
    pub providers: Arc<Mutex<Providers>>,
    pub db_path: String,
}

impl ProviderServer {
    /// `project_base_dir` should be the parent of `provider/` or `providers/`
    /// e.g. "/Users/augustusrizza/Code/rust/provider"
    pub fn new(address: String, http_address: String, db_path: String) -> Self {
        // 1) Prime Python sys.path once, at process start.
        //    This adds <base>, <base>/provider, and <base>/providers if they exist.
        if let Err(e) = add_dirs_to_syspath(".") {
            // Don't crash; just log—HTTP loader can still pass a base later
            error!("Failed to add python paths: {e}");
        } else {
            info!("Python sys.path primed for base_dir={}", ".");
        }

        // 2) Build providers registry and register built-in Rust providers
        let mut providers = Providers::new();
        providers.add_provider(
            "yahoo_finance".to_string(),
            Box::new(crate::providers::yahoo_finance::YahooFinanceProvider::new(
                &db_path,
            )),
        );

        Self {
            address,
            http_address,
            providers: Arc::new(Mutex::new(providers)),
            db_path,
        }
    }

    /// Start HTTP (Axum) in its own Tokio runtime thread, then run TCP accept loop.
    pub fn listen(&mut self) {
        let state = AppState {
            providers: Arc::clone(&self.providers),
        };
        let http_addr: SocketAddr = self
            .http_address
            .parse()
            .expect(format!("invalid http_address (e.g., {})", self.http_address).as_str());

        // Spawn HTTP server in a dedicated Tokio runtime thread
        let _http_thr = {
            let state_clone = state.clone();
            thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");

                rt.block_on(async move {
                    let app = Router::new()
                        .route("/providers", get(http_list_providers))
                        .route("/providers/:name/ping", get(http_ping_provider))
                        .route("/plugins/load", post(http_load_plugin))
                        .with_state(state_clone);

                    info!("HTTP listening on {}", http_addr);

                    let listener = tokio::net::TcpListener::bind(http_addr).await.unwrap();
                    axum::serve(listener, app).await.unwrap();
                });
            })
        };

        // Start blocking TCP listener on current thread
        let listener = match TcpListener::bind(&self.address) {
            Ok(l) => l,
            Err(e) => {
                error!("Bind failed on {}: {}", self.address, e);
                return;
            }
        };
        info!("TCP listening on {}", self.address);

        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let peer = stream.peer_addr().ok();
                    info!("New connection: {:?}", peer);

                    // clone Arc for per-conn handler
                    let providers = Arc::clone(&self.providers);
                    thread::spawn(move || {
                        if let Err(e) = handle_connection(providers, &mut stream) {
                            error!("connection error ({:?}): {}", peer, e);
                        }
                    });
                }
                Err(e) => error!("Accept failed: {}", e),
            }
        }
    }
}

fn handle_connection(providers: Arc<Mutex<Providers>>, stream: &mut TcpStream) -> io::Result<()> {
    let addr = stream.peer_addr()?;
    info!("TCP STREAM STARTED : {}", addr);

    // Clone for reader; keep `stream` for writing.
    let reader_stream = stream.try_clone()?;
    let mut reader = BufReader::new(reader_stream);

    let mut processed = 0usize;
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            // client closed
            break;
        }

        let msg = line.trim();
        if msg.is_empty() {
            continue;
        }

        info!("Received from {}: {}", addr, msg);

        // Parse the FULL ENVELOPE with new payload variants
        match serde_json::from_str::<QueryEnvelope<QueryEnvelopePayload>>(msg) {
            Ok(env) => {
                processed += 1;

                match &env.query {
                    QueryEnvelopePayload::ProviderList => {
                        let names = providers.lock().unwrap().provider_list();
                        let resp = ResponseEnvelope {
                            ok: true,
                            request_id: Some(env.request_id.clone()),
                            kind: ResponseKind::ProviderList,
                            provider: None,
                            request_kind: None,
                            result: Some(serde_json::to_value(names).unwrap_or(Value::Null)),
                            error: None,
                            ts_ms: now_ms(),
                        };
                        let line = serde_json::to_string(&resp).unwrap();
                        writeln!(stream, "{line}")?;
                        stream.flush()?;
                    }

                    QueryEnvelopePayload::ProviderRequest { provider, request } => {
                        let request_kind = match request {
                            EntityInProvider::GetEntity { .. } => "GetEntity",
                            EntityInProvider::SearchEntities { .. } => "SearchEntities",
                            EntityInProvider::GetEntities { .. } => "GetEntities",
                            EntityInProvider::GetAllEntities { .. } => "GetAllEntities",
                            EntityInProvider::GetReport { .. } => "GetReport",
                        }
                        .to_string();

                        match providers.lock().unwrap().get_provider_mut(provider) {
                            Some(p) => match p.fetch_entities(request.clone()) {
                                Ok(entities) => {
                                    let resp = ResponseEnvelope {
                                        ok: true,
                                        request_id: Some(env.request_id.clone()),
                                        kind: ResponseKind::ProviderRequest,
                                        provider: Some(provider.clone()),
                                        request_kind: Some(request_kind),
                                        result: Some(
                                            serde_json::to_value(&entities).unwrap_or(Value::Null),
                                        ),
                                        error: None,
                                        ts_ms: now_ms(),
                                    };
                                    let line = serde_json::to_string(&resp).unwrap();
                                    writeln!(stream, "{line}")?;
                                    stream.flush()?;
                                }
                                Err(e) => {
                                    let resp = ResponseEnvelope::<Value> {
                                        ok: false,
                                        request_id: Some(env.request_id.clone()),
                                        kind: ResponseKind::ProviderRequest,
                                        provider: Some(provider.clone()),
                                        request_kind: Some(request_kind),
                                        result: None,
                                        error: Some(ResponseError {
                                            code: Some("provider_request_failed".to_string()),
                                            message: e,
                                        }),
                                        ts_ms: now_ms(),
                                    };
                                    let line = serde_json::to_string(&resp).unwrap();
                                    writeln!(stream, "{line}")?;
                                    stream.flush()?;
                                }
                            },
                            None => {
                                let resp = ResponseEnvelope::<Value> {
                                    ok: false,
                                    request_id: Some(env.request_id.clone()),
                                    kind: ResponseKind::ProviderRequest,
                                    provider: Some(provider.clone()),
                                    request_kind: Some(request_kind),
                                    result: None,
                                    error: Some(ResponseError {
                                        code: Some("provider_not_found".to_string()),
                                        message: format!("unknown provider '{provider}'"),
                                    }),
                                    ts_ms: now_ms(),
                                };
                                let line = serde_json::to_string(&resp).unwrap();
                                writeln!(stream, "{line}")?;
                                stream.flush()?;
                            }
                        };
                    }
                }
            }
            Err(e) => {
                warn!("Invalid JSON from {}: {}  (raw: {})", addr, e, msg);
                let resp = ResponseEnvelope::<Value> {
                    ok: false,
                    request_id: None,
                    kind: ResponseKind::InvalidJson,
                    provider: None,
                    request_kind: None,
                    result: None,
                    error: Some(ResponseError {
                        code: Some("invalid_json".to_string()),
                        message: format!("invalid json: {e}"),
                    }),
                    ts_ms: now_ms(),
                };
                let line = serde_json::to_string(&resp).unwrap();
                writeln!(stream, "{line}")?;
                stream.flush()?;
            }
        }
    }

    info!("Client disconnected: {}", addr);
    let _ = writeln!(stream, "Processed {} queries", processed);
    let _ = stream.flush();
    Ok(())
}

// Timestamp helper (or use a shared util)
fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
