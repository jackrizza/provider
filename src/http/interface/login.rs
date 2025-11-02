/*
SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza
*/

use super::{render_page, render_page_no_nav};
use crate::http::AppState;
use axum::{
    extract::{Form, State},
    http::{HeaderMap, HeaderValue, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use maud::{Markup, html};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct LoginForm {
    email: String,
    password: String,
}

/* ---------- markup helpers ---------- */

fn login_form_markup() -> Markup {
    html! {
        div class="uk-card uk-card-default uk-card-body uk-border-rounded uk-box-shadow-small uk-margin-large-top uk-width-1-2@m" {
            h3 class="uk-card-title" { "Sign in" }
            form class="uk-form-stacked" method="post" action="/login" {
                div class="uk-margin" {
                    label class="uk-form-label" for="email" { "Email" }
                    div class="uk-form-controls" {
                        input class="uk-input" id="email" name="email" type="email" required;
                    }
                }
                div class="uk-margin" {
                    label class="uk-form-label" for="password" { "Password" }
                    div class="uk-form-controls" {
                        input class="uk-input" id="password" name="password" type="password" required;
                    }
                }
                div class="uk-margin" {
                    button class="uk-button uk-button-primary" type="submit" { "Sign in" }
                }
            }
        }
    }
}

fn login_error_markup(msg: &str) -> Markup {
    html! {
        div class="uk-alert-danger uk-margin-large-top" uk-alert {
            a class="uk-alert-close" uk-close {}
            p { (msg) }
        }
        (login_form_markup())
    }
}

/* ---------- public page builders ---------- */

pub fn login_form() -> String {
    render_page_no_nav("Provider – Sign in", login_form_markup())
}

pub fn login_error(msg: &str) -> String {
    render_page_no_nav("Provider – Sign in", login_error_markup(msg))
}

/* ---------- http handlers ---------- */

pub async fn http_login_form() -> Html<String> {
    Html(login_form())
}

pub async fn http_login_submit(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    match state.auth_service.login(&form.email, &form.password) {
        Ok(tokens) => {
            // build cookie
            let cookie = format!(
                "provider_auth={}; Path=/; HttpOnly; SameSite=Lax",
                tokens.access_token
            );

            // set-cookie header
            let mut headers = HeaderMap::new();
            headers.insert(
                header::SET_COOKIE,
                HeaderValue::from_str(&cookie).expect("set-cookie header"),
            );

            // redirect home (or /providers, your choice)
            let mut resp: Response = Redirect::to("/").into_response();
            resp.headers_mut().extend(headers);
            resp
        }
        Err(e) => Html(login_error(&format!("Invalid credentials: {e}"))).into_response(),
    }
}

/// GET /signout
pub async fn http_signout() -> impl IntoResponse {
    // expire the cookie
    // note: we set Path=/ so it matches what we created on login
    let expired = "provider_auth=deleted; Path=/; Max-Age=0; HttpOnly; SameSite=Lax";

    let mut resp = Redirect::to("/login").into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(expired).expect("set-cookie"),
    );
    resp
}
