// src/http/interface/my_account.rs

use crate::http::AppState;
use crate::http::interface::render_page;
use crate::models::Auth;
use axum::extract::Extension;
use axum::{
    extract::State,
    response::{Html, IntoResponse},
};
use maud::{Markup, html}; // your auth row

pub async fn http_my_account(
    State(_state): State<AppState>,
    Extension(user): Extension<Auth>,
) -> impl IntoResponse {
    let body: Markup = html! {
        div class="uk-section uk-section-muted uk-padding" {
            h2 { "My account" }
            p { "This is the account currently signed in." }

            dl class="uk-description-list" {
                dt { "Email" }
                dd { (user.email) }

                dt { "Access token" }
                dd {
                    pre class="uk-background-default uk-padding-small uk-text-break" {
                        (user.access_token)
                    }
                }

                dt { "Refresh token" }
                dd {
                    @if !user.refresh_token.is_empty() {
                        pre class="uk-background-default uk-padding-small uk-text-break" {
                            (user.refresh_token)
                        }
                    } @else {
                        em { "none" }
                    }
                }
            }

            form class="uk-margin-top" method="post" action="/my-account/refresh" {
                button class="uk-button uk-button-primary" type="submit" {
                    "Refresh token"
                }
            }
        }
    };

    Html(render_page("Provider – My account", body))
}

use axum::http::{HeaderValue, header};
use axum::response::Response;
use axum_extra::extract::cookie::CookieJar;

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
            // build updated user to show (email stays same)
            let refreshed_user = Auth {
                access_token: tokens.access_token.clone(),
                refresh_token: tokens.refresh_token.clone(),
                ..user
            };

            // build alert + page
            let body: Markup = html! {
                div class="uk-section uk-section-muted uk-padding" {
                    div class="uk-alert-success" uk-alert {
                        a class="uk-alert-close" uk-close {}
                        p { "Token refreshed." }
                    }
                    h2 { "My account" }
                    dl class="uk-description-list" {
                        dt { "Email" }
                        dd { (refreshed_user.email) }

                        dt { "Access token" }
                        dd {
                            pre class="uk-background-default uk-padding-small uk-text-break" {
                                (refreshed_user.access_token)
                            }
                        }

                        dt { "Refresh token" }
                        dd {
                            @if !refreshed_user.refresh_token.is_empty() {
                                pre class="uk-background-default uk-padding-small uk-text-break" {
                                    (refreshed_user.refresh_token)
                                }
                            } @else {
                                em { "none" }
                            }
                        }
                    }

                    form class="uk-margin-top" method="post" action="/my-account/refresh" {
                        button class="uk-button uk-button-primary" type="submit" {
                            "Refresh token"
                        }
                    }
                }
            };

            // set new cookie
            let mut resp: Response =
                Html(render_page("Provider – My account", body)).into_response();
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
                    ( // reuse the normal layout
                        html! {
                            h2 { "My account" }
                            p { "Try signing out and back in." }
                            form class="uk-margin-top" method="post" action="/my-account/refresh" {
                                button class="uk-button uk-button-primary" type="submit" {
                                    "Retry"
                                }
                            }
                        }
                    )
                }
            };
            Html(render_page("Provider – My account", body)).into_response()
        }
    }
}
