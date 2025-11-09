/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/
use crate::http::AppState;
use crate::http::interface::render_page_with_user;
use crate::models::Auth;
use axum::{
    extract::{Form, Path, State},
    response::{Html, IntoResponse, Redirect},
};
use maud::{Markup, html};
use serde::Deserialize;
use uuid::Uuid;

use axum::Extension;

#[derive(Deserialize)]
pub struct NewProjectForm {
    name: String,
    description: String,
    // list of provider names selected from UI
    // e.g. "yahoo_finance,sec_edgar"
    providers: Option<String>,
}

pub async fn http_projects(
    State(state): State<AppState>,
    axum::Extension(user): axum::Extension<Auth>,
) -> impl IntoResponse {
    // get projects owned by or shared with this user
    let list = state
        .project_service
        .list_projects_for_user(&user)
        .unwrap_or_default();

    let body: Markup = html! {
        div class="uk-section uk-section-muted uk-padding" {
            h2 { "Projects" }
            p { "Projects limit which providers can be used." }
            a class="uk-button uk-button-primary uk-margin-bottom" href="/projects/new" { "New project" }

            div class="uk-overflow-auto" {
                table class="uk-table uk-table-divider uk-table-striped" {
                    thead {
                        tr {
                            th { "Name" }
                            th { "Owner" }
                            th { "Visibility" }
                            th { "Actions" }
                        }
                    }
                    tbody {
                        @for p in list {
                            tr {
                                td { (p.name) }
                                td { (state.auth_service.get_email_from_id(&p.owner_id).unwrap_or_default()) }
                                td { (p.visibility) }
                                td {
                                    a class="uk-button uk-button-default uk-button-small" href={(format!("/projects/{}", p.id.unwrap_or_default()))} { "Open" }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    Html(render_page_with_user("Projects", &user, body))
}

pub async fn http_projects_new_form(
    State(state): State<AppState>,
    axum::Extension(user): axum::Extension<Auth>,
) -> impl IntoResponse {
    // you can get the full provider list from the in-memory registry
    let provs = state.providers.lock().unwrap().provider_list();

    let body: Markup = html! {
        div class="uk-section uk-section-muted uk-padding" {
            h2 { "New project" }
            form class="uk-form-stacked" method="post" action="/projects/new" {
                div class="uk-margin" {
                    label class="uk-form-label" for="name" { "Name" }
                    div class="uk-form-controls" {
                        input class="uk-input" id="name" name="name" required;
                    }
                }
                div class="uk-margin" {
                    label class="uk-form-label" for="description" { "Description" }
                    div class="uk-form-controls" {
                        textarea class="uk-textarea" id="description" name="description" {}
                    }
                }
                div class="uk-margin" {
                    label class="uk-form-label" { "Allowed providers" }
                    p class="uk-text-meta" { "Select providers this project can use." }
                    div class="uk-form-controls" {
                        @for p in provs {
                            label class="uk-margin-small-right" {
                                input class="uk-checkbox" type="checkbox" name="providers" value=(p) {}
                                (p)
                            }
                        }
                    }
                }
                div class="uk-margin" {
                    button class="uk-button uk-button-primary" type="submit" { "Create" }
                }
            }
        }
    };

    Html(render_page_with_user("New project", &user, body))
}

pub async fn http_projects_new_submit(
    State(state): State<AppState>,
    axum::Extension(user): axum::Extension<Auth>,
    Form(form): Form<NewProjectForm>,
) -> impl IntoResponse {
    let id = Uuid::new_v4().to_string();

    // providers may come as a single comma string or multiple form values; depends on your form
    // keep it simple: split on commas if present
    let selected = form
        .providers
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    if let Err(e) = state.project_service.create_project_with_providers(
        &id,
        &form.name,
        &form.description,
        user.id.as_deref().unwrap(),
        &selected,
    ) {
        // show error
        let body = html! {
            div class="uk-section uk-section-muted uk-padding" {
                div class="uk-alert-danger" uk-alert {
                    a class="uk-alert-close" uk-close {}
                    p { (format!("Could not create project: {e}")) }
                }
                a class="uk-button uk-button-default" href="/projects/new" { "Try again" }
            }
        };
        return Html(render_page_with_user("New project", &user, body)).into_response();
    }

    Redirect::to("/projects").into_response()
}

#[derive(Deserialize)]
pub struct AddProjectUserForm {
    user_id: String,
    role: Option<String>,
}

// GET /projects/:id
pub async fn http_project_detail(
    State(state): State<AppState>,
    Extension(user): Extension<Auth>,
    Path(project_id): Path<String>,
) -> impl IntoResponse {
    // load project
    let project = match state.project_service.get_project(&project_id) {
        Ok(Some(p)) => p,
        Ok(None) => {
            // 404 page
            let body = html! {
                div class="uk-section uk-section-muted uk-padding" {
                    div class="uk-alert-danger" uk-alert {
                        p { "Project not found." }
                    }
                }
            };
            return Html(crate::http::interface::render_page_with_user(
                "Project not found",
                &user,
                body,
            ))
            .into_response();
        }
        Err(e) => {
            let body = html! {
                div class="uk-section uk-section-muted uk-padding" {
                    div class="uk-alert-danger" uk-alert {
                        p { (format!("Error loading project: {e}")) }
                    }
                }
            };
            return Html(crate::http::interface::render_page_with_user(
                "Project error",
                &user,
                body,
            ))
            .into_response();
        }
    };

    // only owner OR admin can manage members
    let is_admin = state.auth_service.is_admin(&user);
    let is_owner = user.id.as_deref() == Some(project.owner_id.as_str());
    let can_edit = is_admin || is_owner;

    // list current project users
    let members = state
        .project_service
        .list_project_users(&project_id)
        .unwrap_or_default();

    // list all users (for the select); only if admin/owner
    let all_users = if can_edit {
        state.auth_service.list_users().unwrap_or_default()
    } else {
        vec![]
    };

    let body: Markup = html! {
        div class="uk-section uk-section-muted uk-padding" {
            h2 { (format!("Project: {}", project.name)) }
            p { (project.description) }

            h3 { "Members" }
            div class="uk-overflow-auto" {
                table class="uk-table uk-table-divider uk-table-small" {
                    thead {
                        tr {
                            th { "User" }
                            th { "Role" }
                        }
                    }
                    tbody {
                        @for m in &members {
                            tr {
                                td { (state.auth_service.get_email_from_id(&m.user_id).unwrap_or_default()) }
                                td { (m.role) }
                            }
                        }
                        @if members.is_empty() {
                            tr {
                                td colspan="2" {
                                    em { "No members yet." }
                                }
                            }
                        }
                    }
                }
            }

            @if can_edit {
                h3 { "Add member" }
                form class="uk-form-stacked uk-margin" method="post" action={(format!("/projects/{}/users", project_id))} {
                    div class="uk-margin" {
                        label class="uk-form-label" for="user_id" { "User" }
                        div class="uk-form-controls" {
                            select class="uk-select" name="user_id" id="user_id" required {
                                option value="" { "-- select user --" }
                                @for u in &all_users {
                                    // assuming u.email exists
                                    option value={(u.id.as_deref().unwrap_or(""))} {
                                        (u.email)
                                        @if state.auth_service.is_admin(u) {
                                            " (admin)"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div class="uk-margin" {
                        label class="uk-form-label" for="role" { "Role" }
                        div class="uk-form-controls" {
                            select class="uk-select" name="role" id="role" {
                                option value="editor" { "Editor" }
                                option value="viewer" { "Viewer" }
                            }
                        }
                    }
                    div class="uk-margin" {
                        button class="uk-button uk-button-primary" type="submit" { "Add to project" }
                    }
                }
            } @else {
                div class="uk-alert-primary" uk-alert {
                    p { "You can view this project but not manage members." }
                }
            }
        }
    };

    Html(crate::http::interface::render_page_with_user(
        "Project detail",
        &user,
        body,
    ))
    .into_response()
}

// POST /projects/:id/users
pub async fn http_project_add_user(
    State(state): State<AppState>,
    Extension(user): Extension<Auth>,
    Path(project_id): Path<String>,
    Form(form): Form<AddProjectUserForm>,
) -> impl IntoResponse {
    // get project
    let project = match state.project_service.get_project(&project_id) {
        Ok(Some(p)) => p,
        _ => return Redirect::to("/projects").into_response(),
    };

    // check perms: only owner or admin can add
    let is_admin = state.auth_service.is_admin(&user);
    let is_owner = user.id.as_deref() == Some(project.owner_id.as_str());
    if !is_admin && !is_owner {
        return Redirect::to("/projects").into_response();
    }

    let role = form.role.unwrap_or_else(|| "editor".to_string());

    if let Err(e) = state
        .project_service
        .add_user_to_project(&project_id, &form.user_id, &role)
    {
        // on error, just go back to detail for now
        eprintln!("add_user_to_project error: {e}");
    }

    Redirect::to(&format!("/projects/{}", project_id)).into_response()
}
