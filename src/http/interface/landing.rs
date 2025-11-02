use super::render_page;
use axum::response::{Html, IntoResponse};
use maud::html;

pub async fn http_landing() -> impl IntoResponse {
    let body = html! {
        div class="uk-section uk-section-muted uk-padding-large uk-text-center" {
            h1 class="uk-heading-medium" { "Provider" }
            p class="uk-text-lead" {
                "Tiny data provider hub — TCP for queries, HTTP for admin, SQLite cache, Rust + hot Python providers."
            }
            p {
                a class="uk-button uk-button-primary uk-button-large"
                  href="https://github.com/jackrizza/provider"
                  target="_blank" rel="noreferrer" {
                    "View on GitHub"
                }
            }
        }
    };

    Html(render_page("Provider – Home", body))
}
