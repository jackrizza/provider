/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use axum::{
    extract::{Form, Path, State},
    response::{Html, IntoResponse, Redirect, Response},
};
use maud::{Markup, html};
use serde::Deserialize;

use crate::http::AppState;
use crate::http::interface::render_page_with_user;
use crate::models::Auth; // your DB model

#[derive(Deserialize)]
pub struct NewUserForm {
    email: String,
    password: String,
    role: String,
}

// GET /users
pub async fn http_users(
    State(state): State<AppState>,
    axum::Extension(current): axum::Extension<Auth>,
) -> impl IntoResponse {
    let users = match state.auth_service.list_users() {
        Ok(u) => u,
        Err(e) => {
            let body = html! {
                div class="uk-section uk-section-muted uk-padding" {
                    div class="uk-alert-danger" uk-alert {
                        a class="uk-alert-close" uk-close {}
                        p { (format!("Could not load users: {e}")) }
                    }
                }
            };
            return Html(render_page_with_user("Users", &current, body));
        }
    };

    let body: Markup = html! {
        div class="uk-section uk-section-muted uk-padding" {
            h2 { "Users" }

            // add form
            ul uk-accordion {
                li {
                    a class="uk-accordion-title" { "Add user" }
                    div class="uk-accordion-content" {
                        form class="uk-form-stacked" method="post" action="/users" {
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
                                label class="uk-form-label" for="role" { "Role" }
                                div class="uk-form-controls" {
                                    select class="uk-select" id="role" name="role" {
                                        option value="user" { "user" }
                                        option value="admin" { "admin" }
                                    }
                                }
                            }
                            div class="uk-margin" {
                                button class="uk-button uk-button-primary" type="submit" { "Create" }
                            }
                        }
                    }
                }

            }


            // table
            div class="uk-overflow-auto" {
                table class="uk-table uk-table-divider uk-table-striped uk-table-hover" {
                    thead {
                        tr {
                            th { "ID" }
                            th { "Email" }
                            th { "Role" }
                            th { "Actions" }
                        }
                    }
                    tbody {
                        @for u in users {
                            tr {
                                td { (u.id.clone().unwrap_or_else(|| "-".to_string())) }
                                td { (u.email) }
                                td { (u.role) }
                                td {
                                    // delete form
                                    @if let Some(id) = u.id.clone() {
                                        form method="post" action={(format!("/users/{}/delete", id))} style="display:inline-block" {
                                            button class="uk-button uk-button-danger uk-button-small" type="submit" {
                                                "Delete"
                                            }
                                        }
                                    } @else {
                                        em { "n/a" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    Html(render_page_with_user("Users", &current, body))
}

// POST /users  (add)
pub async fn http_users_add(
    State(state): State<AppState>,
    axum::Extension(current): axum::Extension<Auth>,
    Form(form): Form<NewUserForm>,
) -> impl IntoResponse {
    // only admins should reach here because route is behind require_role
    match state
        .auth_service
        .create_user_with_role(&form.email, &form.password, &form.role)
    {
        Ok(_) => Redirect::to("/users").into_response(),
        Err(e) => {
            // render with error
            let body = html! {
                div class="uk-section uk-section-muted uk-padding" {
                    div class="uk-alert-danger" uk-alert {
                        a class="uk-alert-close" uk-close {}
                        p { (format!("Could not create user: {e}")) }
                    }
                    p { a href="/users" { "Back to users" } }
                }
            };
            Html(render_page_with_user("Users – error", &current, body)).into_response()
        }
    }
}

// POST /users/:id/delete
pub async fn http_users_delete(
    State(state): State<AppState>,
    axum::Extension(current): axum::Extension<Auth>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // you probably don't want admins to delete themselves:
    if current.id.as_deref() == Some(id.as_str()) {
        let body = html! {
            div class="uk-section uk-section-muted uk-padding" {
                div class="uk-alert-warning" uk-alert {
                    a class="uk-alert-close" uk-close {}
                    p { "You cannot delete yourself." }
                }
                p { a href="/users" { "Back to users" } }
            }
        };
        return Html(render_page_with_user("Users – warning", &current, body)).into_response();
    }

    match state.auth_service.delete_user(&id) {
        Ok(_) => Redirect::to("/users").into_response(),
        Err(e) => {
            let body = html! {
                div class="uk-section uk-section-muted uk-padding" {
                    div class="uk-alert-danger" uk-alert {
                        a class="uk-alert-close" uk-close {}
                        p { (format!("Could not delete user: {e}")) }
                    }
                    p { a href="/users" { "Back to users" } }
                }
            };
            Html(render_page_with_user("Users – error", &current, body)).into_response()
        }
    }
}
