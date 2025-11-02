/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

pub mod account;
pub mod init;
pub mod landing;
pub mod login;
pub mod providers;

use maud::{DOCTYPE, Markup, html};

pub fn render_page(title: &str, body: Markup) -> String {
    let markup = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                title { (title) }
                meta name="viewport" content="width=device-width, initial-scale=1";
                // UIkit CSS
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uikit@3.19.4/dist/css/uikit.min.css" {}
                // UIkit JS
                script src="https://cdn.jsdelivr.net/npm/uikit@3.19.4/dist/js/uikit.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/uikit@3.19.4/dist/js/uikit-icons.min.js" {}
            }
            body {
                nav class="uk-navbar uk-background-muted uk-navbar-container uk-margin" {
                    div class="uk-container" {
                        div uk-navbar {
                            div class="uk-navbar-left" {
                                a class="uk-navbar-item uk-logo" href="/" { "Provider" }
                                ul class="uk-navbar-nav" {
                                    li { a href="/providers" { "Providers" } }
                                    li { a href="/plugins" { "Plugins" } }
                                    li { a href="/my-account" { "My Account" } }
                                    li {
                                        a href="/sign-out" { span class="uk-text-danger" {
                                            "Sign Out"
                                        } }
                                    }
                                }
                            }
                        }
                    }
                }

                div class="uk-container" {
                    (body)
                }
            }
        }
    };

    markup.into_string()
}

pub fn render_page_no_nav(title: &str, core: Markup) -> String {
    let markup = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                title { (title) }
                meta name="viewport" content="width=device-width, initial-scale=1";
                // UIkit CSS
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uikit@3.19.4/dist/css/uikit.min.css" {}
                // UIkit JS
                script src="https://cdn.jsdelivr.net/npm/uikit@3.19.4/dist/js/uikit.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/uikit@3.19.4/dist/js/uikit-icons.min.js" {}
            }
            body {
                div class="uk-container" {
                    (core)
                }
            }
        }
    };

    markup.into_string()
}
