/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/
use super::render_page_with_user;
use crate::models::Auth;
use axum::extract::Extension;
use axum::response::{Html, IntoResponse};
use maud::{Markup, html};

pub async fn http_landing(Extension(user): Extension<Auth>) -> impl IntoResponse {
    let readme: Markup = html! {
        article class="uk-article" {
            h3 class="uk-article-title" { "Provider" }
            p class="uk-text-meta" { "A tiny data provider hub (TCP + HTTP + SQLite + hot-loadable providers)." }

            p {
                "It caches external JSON in SQLite, serves DB-first, and stitches gaps by only fetching missing slices from upstream."
            }

            ul class="uk-list uk-list-bullet" {
                li { "TCP server for queries" }
                li { "HTTP API for admin/ops" }
                li { "SQLite cache via Diesel + r2d2" }
                li { "Rust + Python providers (hot-load)" }
            }

            p {
                a class="uk-button uk-button-text" href="https://github.com/jackrizza/provider" target="_blank" {
                    "View on GitHub"
                }
            }
        }
    };

    let body = html! {
        // main section
        div class="uk-section" {

            div class="uk-grid-small" uk-grid {
                div class="uk-width-1-1 uk-width-5-5@" {
                    div class="uk-card uk-card-default uk-card-body" {
                        canvas id="landing_chart" width="400" height="150" {}
                        script src="/cdn/js/dashboard_chart.js" {}
                    }
                }

                // left column: welcome / status
                div class="uk-width-1-1 uk-width-3-5@m " {
                    div class="uk-card uk-card-default uk-card-body" {
                        h2 { "Welcome back, " (user.email) "!" }
                        p class="uk-text-meta" { "This is your Provider admin dashboard." }

                        ul class="uk-list uk-list-divider" {
                            li {
                                a class="uk-link-reset" href="/providers" {
                                    span uk-icon="database" {}
                                    span class="uk-margin-small-left" { "View providers" }
                                }
                            }
                            li {
                                a class="uk-link-reset" href="/projects" {
                                    span uk-icon="folder" {}
                                    span class="uk-margin-small-left" { "Your projects" }
                                }
                            }
                            li {
                                a class="uk-link-reset" href="/my-account" {
                                    span uk-icon="user" {}
                                    span class="uk-margin-small-left" { "My account / tokens" }
                                }
                            }
                        }
                    }
                }

                // right column: README card
                div class="uk-width-1-1 uk-width-2-5@m" {
                    div class="uk-card uk-card-default uk-card-small uk-overflow-auto" style="max-height: 600px;" {
                        div class="uk-card-header" {
                            h3 class="uk-card-title" { "Repository README" }
                        }
                        div class="uk-card-body" {
                            (readme)
                        }
                    }
                }
            }
        }
    };

    Html(render_page_with_user("Provider – Home", &user, body))
}
