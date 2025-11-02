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

use crate::auth::AuthService;
use crate::providers::Providers;
use crate::query::{EntityInProvider, QueryEnvelope, QueryEnvelopePayload};

// Bring the unified response into scope
use crate::tcp::response::{ResponseEnvelope, ResponseError, ResponseKind};

use crate::http::{
    AppState,
    files::{http_load_plugin, http_ping_provider},
    interface::{
        account::{http_my_account, http_my_account_refresh},
        init::{http_setup_form, http_setup_submit},
        landing::http_landing,
        login::{http_login_form, http_login_submit, http_signout},
        providers::http_list_providers,
    },
    require_login,
};
use crate::pyadapter::add_dirs_to_syspath;

/// Simple TCP server: one thread per connection (blocking).
pub struct ProviderServer {
    address: String,
    http_address: String,
    pub providers: Arc<Mutex<Providers>>,
    pub db_path: String,
    auth_service: Arc<AuthService>,
}

impl ProviderServer {
    pub fn new(
        address: String,
        http_address: String,
        db_path: String,
        auth_service: AuthService,
    ) -> Self {
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

        let auth_service = Arc::new(auth_service);

        Self {
            address,
            http_address,
            providers: Arc::new(Mutex::new(providers)),
            db_path,
            auth_service,
        }
    }

    /// Start HTTP (Axum) in its own Tokio runtime thread, then run TCP accept loop.
    pub fn listen(&mut self) {
        let state = AppState {
            db_path: self.db_path.clone(),
            providers: Arc::clone(&self.providers),
            auth_service: Arc::clone(&self.auth_service),
        };
        let http_addr: SocketAddr = self
            .http_address
            .parse()
            .expect(format!("invalid http_address (e.g., {})", self.http_address).as_str());

        // HTTP thread
        let _http_thr = {
            let state_clone = state.clone();
            thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");

                rt.block_on(async move {
                    use axum::middleware;

                    let public = Router::new()
                        // setup is public ONLY when no users; handler will deal with that
                        .route("/", get(http_landing))
                        .route("/setup", get(http_setup_form).post(http_setup_submit))
                        .route("/login", get(http_login_form).post(http_login_submit))
                        .route("/sign-out", get(http_signout));

                    let protected = Router::new()
                        .route("/providers", get(http_list_providers))
                        .route("/providers/:name/ping", get(http_ping_provider))
                        .route("/plugins/load", post(http_load_plugin))
                        .route("/my-account", get(http_my_account))
                        .route("/my-account/refresh", post(http_my_account_refresh))
                        .route_layer(middleware::from_fn_with_state(
                            state_clone.clone(),
                            require_login,
                        ));

                    let app = public.merge(protected).with_state(state_clone);

                    info!("HTTP listening on {}", http_addr);

                    let listener = tokio::net::TcpListener::bind(http_addr).await.unwrap();
                    axum::serve(listener, app).await.unwrap();
                });
            })
        };

        // TCP
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

                    let providers = Arc::clone(&self.providers);
                    let auth_service = Arc::clone(&self.auth_service);

                    thread::spawn(move || {
                        if let Err(e) = handle_connection(providers, auth_service, &mut stream) {
                            error!("connection error ({:?}): {}", peer, e);
                        }
                    });
                }
                Err(e) => error!("Accept failed: {}", e),
            }
        }
    }
}

fn handle_connection(
    providers: Arc<Mutex<Providers>>,
    auth_service: Arc<AuthService>,
    stream: &mut TcpStream,
) -> io::Result<()> {
    let addr = stream.peer_addr()?;
    info!("TCP STREAM STARTED : {}", addr);

    let reader_stream = stream.try_clone()?;
    let mut reader = BufReader::new(reader_stream);

    let mut processed = 0usize;
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }

        let msg = line.trim();
        if msg.is_empty() {
            continue;
        }

        info!("Received from {}: {}", addr, msg);

        // 1) parse as raw JSON first so we can grab token
        let raw_val: serde_json::Value = match serde_json::from_str(msg) {
            Ok(v) => v,
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
                continue;
            }
        };

        // try to pull out request_id before we deserialize, so we can echo it on auth errors
        let request_id = raw_val
            .get("request_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // 2) if auth is ON, require a token
        if auth_service.is_enabled() {
            // support either "token" or "access_token" on the top-level
            let token = raw_val
                .get("token")
                .or_else(|| raw_val.get("access_token"))
                .and_then(|v| v.as_str());

            if let Some(token) = token {
                match auth_service.validate_access_token(token) {
                    Ok(user) => {
                        // optional: log who it was
                        info!("auth ok for {} -> {}", addr, user.email);
                    }
                    Err(e) => {
                        // send unauthorized and continue
                        let resp = ResponseEnvelope::<Value> {
                            ok: false,
                            request_id,
                            kind: ResponseKind::Unauthorized,
                            provider: None,
                            request_kind: None,
                            result: None,
                            error: Some(ResponseError {
                                code: Some("unauthorized".to_string()),
                                message: format!("{e}"),
                            }),
                            ts_ms: now_ms(),
                        };
                        let line = serde_json::to_string(&resp).unwrap();
                        writeln!(stream, "{line}")?;
                        stream.flush()?;
                        continue;
                    }
                }
            } else {
                // no token supplied
                let resp = ResponseEnvelope::<Value> {
                    ok: false,
                    request_id,
                    kind: ResponseKind::Unauthorized,
                    provider: None,
                    request_kind: None,
                    result: None,
                    error: Some(ResponseError {
                        code: Some("missing_token".to_string()),
                        message: "auth is enabled but no token was provided".to_string(),
                    }),
                    ts_ms: now_ms(),
                };
                let line = serde_json::to_string(&resp).unwrap();
                writeln!(stream, "{line}")?;
                stream.flush()?;
                continue;
            }
        }

        // 3) NOW deserialize into your real envelope
        let env_res =
            serde_json::from_value::<QueryEnvelope<QueryEnvelopePayload>>(raw_val.clone());
        let env = match env_res {
            Ok(e) => e,
            Err(e) => {
                warn!("Invalid envelope from {}: {}  (raw: {})", addr, e, msg);
                let resp = ResponseEnvelope::<Value> {
                    ok: false,
                    request_id,
                    kind: ResponseKind::InvalidJson,
                    provider: None,
                    request_kind: None,
                    result: None,
                    error: Some(ResponseError {
                        code: Some("invalid_envelope".to_string()),
                        message: format!("invalid envelope: {e}"),
                    }),
                    ts_ms: now_ms(),
                };
                let line = serde_json::to_string(&resp).unwrap();
                writeln!(stream, "{line}")?;
                stream.flush()?;
                continue;
            }
        };

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
