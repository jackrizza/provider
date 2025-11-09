/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/
use crate::http::AppState;
use crate::http::interface::render_page_with_user;
use crate::models::Auth;

use axum::Extension;
use axum::http::StatusCode;
use axum::{
    extract::{Form, Path, State},
    response::{Html, IntoResponse, Redirect},
};
use diesel::SelectableHelper;
use maud::{Markup, html};
use serde::Deserialize;

//
// GET /plug_ins  — page
//
pub async fn http_plugins(
    State(state): State<AppState>,
    Extension(user): Extension<Auth>,
) -> impl IntoResponse {
    // If `Auth.id` is Option<String>, keep this line; if it's String, use `let uid = user.id.as_str();`
    let uid: &str = user.id.as_deref().unwrap_or("");

    let projects = state
        .project_service
        .list_projects_for_user(&user)
        .unwrap_or_default();

    let plugins = state
        .plugin_service
        .list_plugins_for_owner(uid)
        .unwrap_or_default();

    let body: Markup = html! {
        // external CSS
        link rel="stylesheet" href="/cdn/css/plugins.css";

        // Monaco loader (CDN)
        script src="https://cdn.jsdelivr.net/npm/monaco-editor@0.52.0/min/vs/loader.min.js" {}

        // External JS for this page
        script src="/cdn/js/plugins.js" {}

        div uk-grid {
            // SIDEBAR
            div class="uk-width-1-4@m uk-width-1-1" {
                h3 { "Plugins" }
                ul class="uk-nav uk-nav-default" id="plugin_list" {
                    @for p in &plugins {
                        @let pid   = p.id.as_deref().unwrap_or("");
                        @let name  = p.name.as_str();
                        @let entry = p.entry_path.as_str();
                        @let rt    = p.runtime.as_str();

                        li
                          data-plugin-item = (pid)
                          data-plugin-id   = (pid)
                          data-plugin-name = (name)
                          data-plugin-entry= (entry)
                          data-plugin-runtime = (rt) {
                            a href="javascript:void(0);" class="plugin-item" {
                                (name)
                                br;
                                span class="muted mono" { (entry) }
                            }
                            a class="uk-button uk-button-text uk-text-danger plugin-delete"
                               href="javascript:void(0);"
                               data-plugin-id=(pid) { "Delete" }
                        }
                    }
                    @if plugins.is_empty() {
                        li { span class="muted" { "No plugins yet." } }
                    }
                }

                hr;
                ul uk-accordion {
                    li {
                        a class="uk-accordion-title" { "Plugins" }
                        div class="uk-accordion-content" {
                            form id="form_new" class="uk-form-stacked" {
                                div class="uk-margin" {
                                    label class="uk-form-label" for="project_id" { "Project" }
                                    div class="uk-form-controls" {
                                        select class="uk-select" name="project_id" id="project_id" {
                                            @for pr in &projects {
                                                @let pr_id   = pr.id.as_deref().unwrap_or("");
                                                @let pr_name = pr.name.as_str();
                                                option value=(pr_id) { (pr_name) }
                                            }
                                        }
                                    }
                                }
                                div class="uk-margin" {
                                    label class="uk-form-label" for="name" { "Name" }
                                    div class="uk-form-controls" {
                                        input class="uk-input" type="text" name="name" id="name" placeholder="acme.quotes";
                                    }
                                }
                                div class="uk-margin" {
                                    label class="uk-form-label" for="entry_path" { "Entry path" }
                                    div class="uk-form-controls" {
                                        input class="uk-input" type="text" name="entry_path" id="entry_path" placeholder="/plugins/acme/main.py";
                                    }
                                }
                                div class="uk-margin" {
                                    label class="uk-form-label" for="runtime" { "Runtime" }
                                    div class="uk-form-controls" {
                                        input class="uk-input" type="text" name="runtime" id="runtime" value="python";
                                    }
                                }
                                button class="uk-button uk-button-primary uk-width-1-1" type="submit" { "Create" }
                            }
                        }
                        }
                    }
                }


            // MAIN
            div class="uk-width-3-4@m uk-width-1-1" {
                ul uk-accordion {
                    li {
                        a class="uk-accordion-title" { "Edit Plugin" }
                        div class="uk-accordion-content" {
                            form id="form_edit" class="uk-form-stacked" {
                                input type="hidden" id="edit_id" name="id";
                                div class="uk-grid-small" uk-grid {
                                    div class="uk-width-1-2" {
                                        label class="uk-form-label" for="edit_name" { "Name" }
                                        input class="uk-input" id="edit_name" name="name" placeholder="acme.quotes";
                                    }
                                    div class="uk-width-1-2" {
                                        label class="uk-form-label" for="edit_runtime" { "Runtime" }
                                        input class="uk-input" id="edit_runtime" name="runtime";
                                    }
                                    div class="uk-width-1-1" {
                                        label class="uk-form-label" for="edit_entry_path" { "Entry path" }
                                        input class="uk-input mono" id="edit_entry_path" name="entry_path";
                                    }
                                }

                            }
                        }
                    }
                }
                div class="toolbar" {
                    button class="uk-button uk-button-default" type="submit" { "Update" }
                    button id="btn_save_code" class="uk-button uk-button-primary" type="button" { "Save code" }
                }
                div id="editor" {}
            }
        }
    };

    Html(render_page_with_user("Plugins", &user, body))
}

//
// POST /plug_ins/new  — create
//
#[derive(Deserialize)]
pub struct NewPluginForm {
    project_id: String,
    name: String,
    entry_path: String,
    runtime: String,
}
// POST /plug_ins/new
pub async fn http_plugins_new(
    State(state): State<AppState>,
    Extension(user): Extension<Auth>,
    Form(f): Form<NewPluginForm>,
) -> Result<Redirect, (StatusCode, String)> {
    let id = nanoid::nanoid!();
    state
        .plugin_service
        .new_plugin(
            &user,
            &f.project_id,
            &f.name,
            &f.entry_path,
            &f.runtime,
            &id,
        )
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Redirect::to("/plug_ins"))
}

// POST /plug_ins/:id/update
pub async fn http_plugins_update(
    State(state): State<AppState>,
    Extension(user): Extension<Auth>,
    Path(id): Path<String>,
    Form(f): Form<UpdatePluginForm>,
) -> Result<Redirect, (StatusCode, String)> {
    state
        .plugin_service
        .update_plugin(
            &user,
            &id,
            Some(&f.name),
            Some(&f.entry_path),
            Some(&f.runtime),
        )
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Redirect::to("/plug_ins"))
}

// POST /plug_ins/:id/delete
pub async fn http_plugins_delete(
    State(state): State<AppState>,
    Extension(user): Extension<Auth>,
    Path(id): Path<String>,
    Form(_noop): Form<std::collections::HashMap<String, String>>,
) -> Result<Redirect, (StatusCode, String)> {
    state
        .plugin_service
        .delete_plugin(&user, &id)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Redirect::to("/plug_ins"))
}

//
// POST /plug_ins/:id/update  — update metadata
//
#[derive(Deserialize)]
pub struct UpdatePluginForm {
    name: String,
    entry_path: String,
    runtime: String,
}

//
// POST /plug_ins/:id/save  — save code (file contents)
// Wire this to your blob/node writer that persists `entry_path`
//
#[derive(Deserialize)]
pub struct SaveCodeForm {
    code: String,
}

// POST /plug_ins/:id/save
pub async fn http_plugins_save(
    State(state): State<AppState>,
    Extension(user): Extension<Auth>,
    Path(id): Path<String>,
    Form(f): Form<SaveCodeForm>,
) -> Result<&'static str, (StatusCode, String)> {
    // implement your actual persist logic; for now just OK:
    // Example if you add a method on PluginService:
    // state.plugin_service.save_entry_code(&user, &id, f.code.as_bytes())
    //     .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok("ok")
}
