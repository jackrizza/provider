/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use diesel::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Queryable, Insertable, Selectable)]
#[diesel(table_name = crate::schema::entities)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: Option<String>,
    pub source: String,
    pub tags: String,
    pub data: String,
    pub etag: String,
    pub fetched_at: String,
    pub refresh_after: String,
    pub state: String,
    pub last_error: String,
    pub updated_at: String,
}

impl Entity {
    pub fn get_tags(&self) -> Vec<String> {
        self.tags
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

#[derive(Queryable, Insertable, Selectable)]
#[diesel(table_name = crate::schema::auth)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Auth {
    pub id: Option<String>,
    pub email: String,
    pub password: String,
    pub refresh_token: String,
    pub access_token: String,
    pub refresh_token_expires_at: String,
    pub access_token_expires_at: String,
    pub state: String,
    pub last_error: String,
    pub updated_at: String,
}

impl Auth {
    pub fn new(email: String, password: String) -> Self {
        Auth {
            id: None,
            email,
            password,
            refresh_token: String::new(),
            access_token: String::new(),
            refresh_token_expires_at: String::new(),
            access_token_expires_at: String::new(),
            state: String::new(),
            last_error: String::new(),
            updated_at: String::new(),
        }
    }
}

// For creating a new user (without tokens yet)
#[derive(Insertable)]
#[diesel(table_name = crate::schema::auth)]
pub struct NewAuth {
    pub id: String,
    pub email: String,
    pub password: String,
    pub refresh_token: String,
    pub access_token: String,
    pub refresh_token_expires_at: String,
    pub access_token_expires_at: String,
    pub state: String,
    pub last_error: String,
    pub updated_at: String,
}
