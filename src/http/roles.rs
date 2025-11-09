/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/
use crate::http::AppState;
use crate::models::Auth;
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::Response,
};

/// Strict: user must already be in extensions (so run AFTER require_login)
pub async fn require_role(
    State(_state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // read user from extensions
    if let Some(user) = req.extensions().get::<Auth>() {
        log::info!("User {}, role: {}", user.email, user.role);
        if user.role == "Admin" || user.role == "owner" || user.role == "superuser" {
            return next.run(req).await;
        }

        return Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::from("forbidden"))
            .unwrap();
    }

    // no user at all → go to login
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "/login")
        .body(Body::from("redirecting to /login..."))
        .unwrap()
}
