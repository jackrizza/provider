// src/http/interface/providers.rs

use crate::http::interface::render_page;
use axum::{
    Json,
    extract::{Query, State},
    response::{Html, IntoResponse, Response},
};
use maud::{Markup, html};
use serde::Deserialize;

use crate::http::AppState;
// allow ?format=json
#[derive(Deserialize)]
pub struct ProvidersQuery {
    #[serde(default)]
    format: String,
}

pub async fn http_list_providers(
    State(state): State<AppState>,
    Query(q): Query<ProvidersQuery>,
) -> Response {
    let names = state.providers.lock().unwrap().provider_list();

    if q.format == "json" {
        return Json(names).into_response();
    }

    Html(providers_page(&names)).into_response()
}

/// Render the providers page as a table.
pub fn providers_page(names: &[String]) -> String {
    let body: Markup = html! {
        div class="uk-section uk-section-muted uk-padding" {
            h2 { "Providers" }
            p { "These providers are currently registered in the server." }

            table class="uk-table uk-table-divider uk-table-striped uk-table-hover uk-margin-top" {
                thead {
                    tr {
                        th { "Name" }
                        th { "Actions" }
                    }
                }
                tbody {
                    @for name in names {
                        tr {
                            td { (name) }
                            td {
                                a class="uk-button uk-button-default uk-button-small"
                                  href={(format!("/providers/{}/ping", name))} {
                                    "Ping"
                                }
                            }
                        }
                    }
                    @if names.is_empty() {
                        tr {
                            td colspan="2" {
                                em { "No providers are registered." }
                            }
                        }
                    }
                }
            }

            p class="uk-margin-top" {
                a href="/plugins" class="uk-link-text" { "Load a plugin…" }
            }
        }
    };

    render_page("Provider – Providers", body)
}
