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
