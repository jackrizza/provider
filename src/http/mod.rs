/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

pub mod files;
pub mod interface;

use std::sync::{Arc, Mutex};

use crate::auth::AuthService;
use crate::providers::Providers;
// use crate::pyadapter::PyProviderAdapter;

#[derive(Clone)]
pub struct AppState {
    pub db_path: String,
    pub providers: Arc<Mutex<Providers>>,
    pub auth_service: Arc<AuthService>,
}
