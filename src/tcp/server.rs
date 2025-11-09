/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use axum::Router;
use axum::routing::get_service;
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

use http::StatusCode;
use http::{HeaderValue, header::CACHE_CONTROL};
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
};

use crate::auth::AuthService;
use crate::auth::projects::ProjectService;
use crate::providers::Providers;
use crate::query::{EntityInProvider, QueryEnvelope, QueryEnvelopePayload};
// Bring the unified response into scope
use crate::auth::plugins::PluginService;
use crate::http::{
    AppState,
    files::{http_load_plugin, http_ping_provider},
    interface::{
        account::{http_my_account, http_my_account_refresh},
        init::{http_setup_form, http_setup_submit},
        landing::http_landing,
        login::{http_login_form, http_login_submit, http_signout},
        plugins::http_plugins,
        plugins::{http_plugins_delete, http_plugins_new, http_plugins_save, http_plugins_update},
        projects::{
            http_project_add_user, http_project_detail, http_projects, http_projects_new_form,
            http_projects_new_submit,
        },
        providers::http_list_providers,
        users::{http_users, http_users_add, http_users_delete},
    },
    require_login,
    roles::require_role,
};
use crate::pyadapter::add_dirs_to_syspath;
use crate::tcp::response::{ResponseEnvelope, ResponseError, ResponseKind};

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
        let pool = crate::establish_connection(&self.db_path);
        let project_service = ProjectService::new(pool.clone());
        let plugin_service = PluginService::new(pool.clone());
        let state = AppState {
            db_path: self.db_path.clone(),
            providers: Arc::clone(&self.providers),
            auth_service: Arc::clone(&self.auth_service),
            project_service,
            plugin_service,
        };
        let http_addr: SocketAddr = self
            .http_address
            .parse()
            .expect(format!("invalid http_address (e.g., {})", self.http_address).as_str());

        let static_files = get_service(
            ServeDir::new("./www").append_index_html_on_directories(true), // /static/ -> /static/index.html if present
        )
        .handle_error(|err| async move {
            log::error!("static file error: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("static file error: {err}"),
            )
        });

        // Optional: long-cache immutable assets (tweak to your needs)
        let cache_static: SetResponseHeaderLayer<HeaderValue> =
            SetResponseHeaderLayer::if_not_present(
                CACHE_CONTROL,
                "public, max-age=31536000, immutable".parse().unwrap(),
            );

        // Optional: gzip/deflate/br for static
        let compression = CompressionLayer::new();

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
                        // redirect to login page
                        // .route("/", get(http_setup_form))
                        // setup is public ONLY when no users; handler will deal with that
                        .route("/setup", get(http_setup_form).post(http_setup_submit))
                        .route("/login", get(http_login_form).post(http_login_submit))
                        .route("/sign-out", get(http_signout))
                        .nest_service("/cdn", static_files);
                    // .layer(cache_static);
                    // .layer(compression);

                    // 1) admin router: only the role check lives here
                    let admin = Router::new()
                        .route("/users", get(http_users).post(http_users_add))
                        .route("/users/:id/delete", post(http_users_delete))
                        .route("/providers", get(http_list_providers))
                        // admin still needs admin-role check
                        .route_layer(middleware::from_fn_with_state(
                            state_clone.clone(),
                            require_role,
                        ));

                    // 2) protected router: build routes...
                    let protected = Router::new()
                        // landing/dashboard
                        .route("/", get(http_landing))
                        // projects
                        .route("/projects", get(http_projects))
                        .route("/projects/new", get(http_projects_new_form))
                        .route("/projects/new", post(http_projects_new_submit))
                        .route("/projects/:id", get(http_project_detail))
                        .route("/projects/:id/users", post(http_project_add_user))
                        // providers
                        .route("/providers/:name/ping", get(http_ping_provider))
                        // account
                        .route("/my-account", get(http_my_account))
                        .route("/my-account/refresh", post(http_my_account_refresh))
                        // plugins
                        .route("/plugins", get(http_plugins))
                        .route("/plugins/load", post(http_load_plugin))
                        .route("/plug_ins", get(http_plugins)) // page
                        .route("/plug_ins/new", post(http_plugins_new)) // create
                        .route("/plug_ins/:id/update", post(http_plugins_update)) // update
                        .route("/plug_ins/:id/delete", post(http_plugins_delete)) // delete
                        .route("/plug_ins/:id/save", post(http_plugins_save)) // save code (file content)
                        // 👇 merge admin **before** we add require_login
                        .merge(admin)
                        // 👇 NOW wrap the WHOLE thing in require_login
                        .route_layer(middleware::from_fn_with_state(
                            state_clone.clone(),
                            require_login,
                        ));

                    // 3) final app
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
