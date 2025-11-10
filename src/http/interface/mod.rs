/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

pub mod account;
pub mod init;
pub mod landing;
pub mod log;
pub mod login;
pub mod plugins;
pub mod projects;
pub mod providers;
pub mod users;

use crate::models::Auth;

use maud::{DOCTYPE, Markup, html};

/// Role-aware version
pub fn render_page_with_user(title: &str, user: &Auth, body: Markup) -> String {
    let is_admin = user.role == "Admin" || user.role == "owner" || user.role == "superuser";

    let markup = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                title { (title) }
                meta name="viewport" content="width=device-width, initial-scale=1";
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uikit@3.19.4/dist/css/uikit.min.css" {}
                link rel="stylesheet" href="/cdn/css/base.css" {}
                script src="https://cdn.jsdelivr.net/npm/uikit@3.19.4/dist/js/uikit.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/uikit@3.19.4/dist/js/uikit-icons.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/chart.js@4.5.1/dist/chart.umd.min.js" {}
            }
            body class="" {
                div id="navbar" {
                    (navbar(Some(user)))
                }
                div id="navbar-mobile" {
                    (navbar_mobile(Some(user)))
                }

                br {}
                br {}
                div class="uk-container" {
                    (body)
                }
            }
            script defer src="/cdn/js/navbar.js" {}
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
                link rel="stylesheet" href="/cdn/css/base.css" {}
                // UIkit JS
                script src="https://cdn.jsdelivr.net/npm/uikit@3.19.4/dist/js/uikit.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/uikit@3.19.4/dist/js/uikit-icons.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/chart.js@4.5.1/dist/chart.umd.min.js" {}
            }
            body class="uk-background-primary uk-padding uk-panel" {
                div class="uk-container center-parent" {
                    div class="center-child" {
                        (core)
                    }
                }
            }
        }
    };

    markup.into_string()
}

pub fn navbar(user: Option<&Auth>) -> Markup {
    let (is_logged_in, is_admin, email) = match user {
        Some(u) => {
            let is_admin = u.role.eq_ignore_ascii_case("admin")
                || u.role.eq_ignore_ascii_case("superuser")
                || u.role.eq_ignore_ascii_case("owner");
            (true, is_admin, u.email.clone())
        }
        None => (false, false, String::new()),
    };

    html! {
        // primary background
        nav class="uk-navbar-container" {
            // keep nav content constrained
            div class="uk-container" {
                // ONE navbar attribute only
                div  uk-navbar="align: center; dropbar: true;" {
                    // LEFT
                    div class="uk-navbar-left" {
                        a class="uk-navbar-item uk-logo" href="/" {
                            img src="/cdn/images/logo.png" width="32" height="32" {}
                            "Provider"
                        }

                        ul class="uk-navbar-nav" {
                            // Providers
                            // li {
                            //     a href="/providers" { "Providers" }
                            // }

                            // Projects
                            li {
                                a href="/projects" { "Projects" }
                                // align dropdown to container
                                div class="uk-navbar-dropdown" uk-navbar-dropdown="boundary: .uk-container; boundary-align: true;" {
                                    ul class="uk-nav uk-navbar-dropdown-nav" {
                                        li { a href="/projects" { "My projects" } }
                                        li { a href="/projects/new" { "New project" } }
                                    }
                                }
                            }

                            // Plugins
                            li {
                                a href="/plugins" { "Plugins" }
                            }

                            // Admin (only if admin)
                            @if is_admin {
                                li {
                                    a href="#" { "Admin" }
                                    // wider dropdown, aligned to container
                                    div class="uk-navbar-dropdown uk-navbar-dropdown-width-2"
                                         uk-navbar-dropdown="boundary: .uk-container; boundary-align: true;" {
                                        div class="uk-drop-grid uk-child-width-1-2" uk-grid {
                                            div {
                                                ul class="uk-nav uk-navbar-dropdown-nav" {
                                                    li class="uk-nav-header" { "Users" }
                                                    li { a href="/users" { "All users" } }
                                                }
                                            }
                                            div {
                                                ul class="uk-nav uk-navbar-dropdown-nav" {
                                                    li class="uk-nav-header" { "Data" }
                                                    li { a href="/providers" { "Providers" } }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // RIGHT
                    div class="uk-navbar-right" {
                        ul class="uk-navbar-nav" {
                            @if is_logged_in {
                                li {
                                    a href="#" {
                                        span class="uk-margin-small-right" uk-icon="user" {}
                                        (email)
                                    }
                                    div class="uk-navbar-dropdown"
                                         uk-navbar-dropdown="boundary: .uk-container; boundary-align: true;" {
                                        ul class="uk-nav uk-navbar-dropdown-nav" {
                                            li { a href="/my-account" { "My account" } }
                                            li class="uk-nav-divider" {}
                                            li { a href="/sign-out" { span class="uk-text-danger" { "Sign out" } } }
                                        }
                                    }
                                }
                            } @else {
                                li {
                                    a href="/login" { "Sign in" }
                                }
                            }

                            // GitHub link
                            li {
                                a href="https://github.com/jackrizza/provider" target="_blank" {
                                    span uk-icon="github" {}
                                    span class="uk-visible@s uk-margin-small-left" { "GitHub" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn navbar_mobile(user: Option<&Auth>) -> Markup {
    let (is_logged_in, is_admin, email) = match user {
        Some(u) => {
            let is_admin = u.role.eq_ignore_ascii_case("admin")
                || u.role.eq_ignore_ascii_case("superuser")
                || u.role.eq_ignore_ascii_case("owner");
            (true, is_admin, u.email.clone())
        }
        None => (false, false, String::new()),
    };

    html! {
        // MOBILE NAVBAR (hidden at >= small breakpoint)
        nav class="uk-navbar-container" {
            div class="uk-container" {
                // one navbar attribute only
                div uk-navbar="align: left; dropbar: true;" {

                    // LEFT: Logo
                    div class="uk-navbar-left" {
                        a class="uk-navbar-item uk-logo" href="/" {
                            img src="/cdn/images/logo.png" width="32" height="32" {}
                            "Provider"
                        }
                    }

                    // RIGHT: Toggle button
                    div class="uk-navbar-right" {
                        a class="uk-navbar-toggle"
                           href="#"
                           uk-toggle="target: #mobile-offcanvas" {
                            span uk-icon="menu" {}
                        }
                    }
                }
            }
        }

        // OFFCANVAS MENU
        div id="mobile-offcanvas" uk-offcanvas="overlay: true" {
            div class="uk-offcanvas-bar" {
                button class="uk-offcanvas-close" type="button" uk-close {} // close

                // (optional) user/email at top
                @if is_logged_in {
                    div class="uk-margin-small-bottom" {
                        span class="uk-margin-small-right" uk-icon="user" {}
                        span { (email) }
                    }
                    hr class="uk-margin-small" {};
                }

                ul class="uk-nav uk-nav-primary uk-nav-parent-icon" uk-nav="multiple: true" {
                    // Primary links
                    li { a href="/projects" { "Projects" } }
                    li { a href="/plugins"  { "Plugins" } }

                    // Admin group
                    @if is_admin {
                        li class="uk-parent" {
                            a href="#" { "Admin" }
                            ul class="uk-nav-sub" {
                                li { a href="/users"     { "All users" } }
                                li { a href="/providers" { "Providers" } }
                            }
                        }
                    }

                    li class="uk-nav-divider" {}

                    // Account / Auth
                    @if is_logged_in {
                        li { a href="/my-account" { "My account" } }
                        li { a href="/sign-out"   { span class="uk-text-danger" { "Sign out" } } }
                    } @else {
                        li { a href="/login" { "Sign in" } }
                    }

                    li class="uk-nav-divider" {}

                    // GitHub
                    li {
                        a href="https://github.com/jackrizza/provider" target="_blank" {
                            span uk-icon="github" {}
                            span class="uk-margin-small-left" { "GitHub" }
                        }
                    }
                }
            }
        }
    }
}
