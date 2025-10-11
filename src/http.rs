/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use serde::Deserialize;
use serde_json::{self, Value, json};
use std::sync::{Arc, Mutex};

use crate::providers::Providers;
use crate::pyadapter::PyProviderAdapter;

// =================== HTTP (AXUM) HANDLERS ===================

use axum::{
    Json,
    extract::{Path, State},
};

#[derive(Clone)]
pub struct AppState {
    pub providers: Arc<Mutex<Providers>>,
}

/// GET /providers → list providers
pub async fn http_list_providers(State(state): State<AppState>) -> Json<Value> {
    let names = state.providers.lock().unwrap().provider_list();
    Json(json!({ "ok": true, "providers": names }))
}

#[derive(Deserialize)]
pub struct LoadPluginReq {
    module: String,
    class: String,
    // optional explicit name override if desired
    name: Option<String>,
}

/// POST /plugins/load {module, class[, name] }
/// Loads a Python Provider dynamically and registers it.
/// Returns the registered name.
pub async fn http_load_plugin(
    State(state): State<AppState>,
    Json(req): Json<LoadPluginReq>,
) -> Json<Value> {
    // NOTE: if your PyProviderAdapter is in crate::providers::pyprovider
    match PyProviderAdapter::inner_load(&req.module, &req.class) {
        Ok(adapter) => {
            let name = req.name.unwrap_or_else(|| adapter.name().to_string());
            state
                .providers
                .lock()
                .unwrap()
                .add_provider(name.clone(), Box::new(adapter));
            Json(json!({ "ok": true, "name": name }))
        }
        Err(e) => {
            Json(json!({ "ok": false, "error": { "code": "plugin_load_failed", "message": e } }))
        }
    }
}

/// Example: GET /providers/{name}/ping
pub async fn http_ping_provider(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Json<Value> {
    let exists = state
        .providers
        .lock()
        .unwrap()
        .get_provider(&name)
        .is_some();
    Json(json!({ "ok": exists, "provider": name }))
}
