/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use crate::http::AppState;
use crate::http::interface::render_page_with_user;
use crate::models::Auth;
use axum::extract::Extension;
use axum::http::{HeaderValue, header};
use axum::response::Response;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Redirect},
};
use axum_extra::extract::cookie::CookieJar;
use maud::{Markup, html}; // your auth row

pub async fn http_my_account(
    State(_state): State<AppState>,
    Extension(user): Extension<Auth>,
) -> impl IntoResponse {
    let body = html! {
        div class="uk-section uk-section-muted uk-padding" {
            h2 { "My account" }
            dl class="uk-description-list" {
                dt { "Email" }
                dd { (user.email) }

                dt { "Role" }
                dd { (user.role) }

                dt { "Access token" }
                dd {
                    pre id="access_token"
                        class="uk-background-default uk-padding-small uk-text-break copy-on-dblclick"
                        uk-tooltip="Click to Copy" {
                        (user.access_token)
                    }
                }
            }
            form class="uk-margin-top" method="post" action="/my-account/refresh" {
                button class="uk-button uk-button-primary" type="submit" { "Refresh token" }
            }
        }

        script defer src="/cdn/js/click_to_copy.js" {}
    };

    Html(render_page_with_user("Provider – My account", &user, body))
}

pub async fn http_my_account_refresh(
    State(state): State<AppState>,
    Extension(user): Extension<Auth>,
    jar: CookieJar,
) -> impl IntoResponse {
    // 1) get current access token from cookie
    let current = jar
        .get("provider_auth")
        .map(|c| c.value().to_string())
        .unwrap_or_default();

    // 2) ask auth service for a fresh pair
    match state.auth_service.refresh_access_token(&current) {
        Ok(tokens) => {
            // success → redirect back to /my-account, but set new cookie
            let mut resp: Response = Redirect::to("/my-account").into_response();
            let cookie = format!(
                "provider_auth={}; Path=/; HttpOnly; SameSite=Lax",
                tokens.access_token
            );
            resp.headers_mut().insert(
                header::SET_COOKIE,
                HeaderValue::from_str(&cookie).expect("set-cookie"),
            );
            resp
        }
        Err(e) => {
            // show error but still show page
            let body: Markup = html! {
                div class="uk-section uk-section-muted uk-padding" {
                    div class="uk-alert-danger" uk-alert {
                        a class="uk-alert-close" uk-close {}
                        p { (format!("Could not refresh token: {e}")) }
                    }
                    h2 { "My account" }
                    p { "Try signing out and back in." }
                    form class="uk-margin-top" method="post" action="/my-account/refresh" {
                        button class="uk-button uk-button-primary" type="submit" {
                            "Retry"
                        }
                    }
                }
            };
            Html(render_page_with_user("Provider – My account", &user, body)).into_response()
        }
    }
}
