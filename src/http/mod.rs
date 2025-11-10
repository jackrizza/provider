/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

pub mod interface;
pub mod services;

use std::sync::{Arc, Mutex};

use crate::auth::AuthService;
use crate::models::Auth;
use crate::providers::Providers;
use services::logs::LogService;
use services::plugins::PluginService;
use services::projects::ProjectService;

// use crate::pyadapter::PyProviderAdapter;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::CookieJar;

#[derive(Clone)]
pub struct AppState {
    pub db_path: String,
    pub providers: Arc<Mutex<Providers>>,
    pub auth_service: Arc<AuthService>,
    pub project_service: ProjectService,
    pub plugin_service: PluginService,
}

pub async fn require_login(
    State(state): State<AppState>,
    jar: CookieJar,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    if let Some(cookie) = jar.get("provider_auth") {
        let token = cookie.value().to_string();
        match state.auth_service.validate_access_token(&token) {
            Ok(user) => {
                // 👇 make the user available to handlers
                req.extensions_mut().insert::<Auth>(user);
                return next.run(req).await;
            }
            Err(_) => {
                // fall through to redirect
            }
        }
    }

    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "/login")
        .body(Body::from("redirecting to /login..."))
        .unwrap()
}
