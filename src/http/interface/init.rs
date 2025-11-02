/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use crate::http::AppState;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::{Form, Html};
use serde::Deserialize;

pub async fn http_setup_form(State(state): State<AppState>) -> impl IntoResponse {
    // if a user already exists, just say so
    if state.auth_service.is_enabled() {
        if let Ok(true) = state.auth_service.has_any_users() {
            return Html("<h1>Setup already completed</h1><p>There is already a user.</p>");
        }
    }

    Html(
        r#"
        <html>
          <body>
            <h1>Initial user setup</h1>
            <form method="post" action="/setup">
              <label>Email: <input type="email" name="email" required /></label><br/>
              <label>Password: <input type="password" name="password" required /></label><br/>
              <button type="submit">Create user</button>
            </form>
          </body>
        </html>
        "#,
    )
}

#[derive(Deserialize)]
pub struct SetupForm {
    email: String,
    password: String,
}

pub async fn http_setup_submit(
    State(state): State<AppState>,
    Form(form): Form<SetupForm>,
) -> impl IntoResponse {
    // block setup if already has users
    if let Ok(true) = state.auth_service.has_any_users() {
        return Html(format!(
            "<h1>Setup already completed</h1><p>There is already a user.</p>"
        ));
    }

    match state.auth_service.register(&form.email, &form.password) {
        Ok(_) => Html(format!(
            "<h1>User created</h1><p>You can now log in / use tokens.</p>"
        )),
        Err(e) => Html(format!("<h1>Error</h1><p>{}</p>", e)),
    }
}
