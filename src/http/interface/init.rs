/*
SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza
*/

use super::{render_page_no_nav, render_page_with_user};
use crate::http::AppState;
use axum::Form;
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use maud::{Markup, html};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SetupForm {
    email: String,
    password: String,
}

/* ---------- markup helpers ---------- */

fn setup_form_markup() -> Markup {
    html! {
        div class="uk-card uk-card-default uk-card-body uk-border-rounded uk-box-shadow-small uk-margin-large-top uk-width-1-2@m" {
            h3 class="uk-card-title" { "Initial admin user" }
            p { "No users were found. Create the first one below." }
            form class="uk-form-stacked" method="post" action="/setup" {
                div class="uk-margin" {
                    label class="uk-form-label" for="email" { "Email" }
                    div class="uk-form-controls" {
                        input class="uk-input" id="email" name="email" type="email"
                              required
                              placeholder="you@example.com";
                    }
                }
                div class="uk-margin" {
                    label class="uk-form-label" for="password" { "Password" }
                    div class="uk-form-controls" {
                        input class="uk-input" id="password" name="password" type="password" required;
                    }
                }
                div class="uk-margin" {
                    button class="uk-button uk-button-primary" type="submit" { "Create user" }
                }
            }
        }
    }
}

fn setup_done_markup() -> Markup {
    html! {
        div class="uk-margin-large-top" {
            div class="uk-alert-success" uk-alert {
                a class="uk-alert-close" uk-close {}
                p { "User created. You can now use the CLI / TCP client with your token." }
            }
            a class="uk-button uk-button-default" href="/providers" { "Go to providers" }
        }
    }
}

fn setup_already_done_markup() -> Markup {
    html! {
        div class="uk-margin-large-top" {
            div class="uk-alert-primary" uk-alert {
                a class="uk-alert-close" uk-close {}
                p { "Setup already completed. There is at least one user." }
            }
        }
    }
}

/* ---------- public builders (string) ---------- */

pub fn setup_form() -> String {
    render_page_no_nav("Provider – Setup", setup_form_markup())
}

pub fn setup_done() -> String {
    render_page_no_nav("Provider – Setup done", setup_done_markup())
}

pub fn setup_already_done() -> String {
    render_page_no_nav("Provider – Already set up", setup_already_done_markup())
}

/* ---------- http handlers ---------- */

pub async fn http_setup_submit(
    State(state): State<AppState>,
    Form(form): Form<SetupForm>,
) -> Html<String> {
    // if we already have a user, don't allow re-setup
    if let Ok(true) = state.auth_service.has_any_users() {
        return Html(setup_already_done());
    }

    match state.auth_service.register(&form.email, &form.password) {
        Ok(_) => Html(setup_done()),
        Err(e) => {
            // show error + form again
            let page = render_page_no_nav(
                "Provider – Setup error",
                html! {
                    div class="uk-alert-danger uk-margin-large-top" uk-alert {
                        a class="uk-alert-close" uk-close {}
                        p { (format!("Could not create user: {e}")) }
                    }
                    (setup_form_markup())
                },
            );
            Html(page)
        }
    }
}

pub async fn http_setup_form(State(state): State<AppState>) -> impl IntoResponse {
    // if auth is enabled AND we already have at least 1 user,
    // don't show setup again — go to /login
    if state.auth_service.is_enabled() {
        if let Ok(true) = state.auth_service.has_any_users() {
            return axum::response::Redirect::temporary("/login").into_response();
        }
    }

    Html(setup_form()).into_response()
}
