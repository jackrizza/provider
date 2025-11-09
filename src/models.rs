/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use diesel::SelectableHelper; // <-- IMPORTANT
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::schema::plugin_file_content::dsl as pfc;
use crate::schema::plugin_nodes::dsl as pn;
use crate::schema::plugins::dsl as pl;

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
    pub role: String,
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
            role: String::new(),
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
    pub role: String,
}

#[derive(Queryable, Identifiable, Debug, Clone, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::projects)]
pub struct Project {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub owner_id: String,
    pub visibility: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::projects)]
pub struct NewProject<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub description: &'a str,
    pub owner_id: &'a str,
    pub visibility: &'a str,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::project_providers)]
pub struct NewProjectProvider<'a> {
    pub project_id: &'a str,
    pub provider_name: &'a str,
}

#[derive(Queryable, Debug, Clone)]
pub struct ProjectUser {
    pub project_id: String,
    pub user_id: String,
    pub role: String,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::project_users)]
pub struct NewProjectUser<'a> {
    pub project_id: &'a str,
    pub user_id: &'a str,
    pub role: &'a str,
}

use crate::schema::{
    blobs,               // content-addressed blobs
    plugin_file_content, // node -> blob link for files
    plugin_nodes,        // folder/file nodes (tree)
    plugin_path_cache,   // cached absolute path per node
    plugins,             // plugin rows
};

/// ======================= plugins =======================

#[derive(Debug, Clone, serde::Serialize, Queryable, Selectable)]
#[diesel(table_name = crate::schema::plugins)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Plugin {
    /// TEXT PRIMARY KEY
    pub id: Option<String>,
    pub project_id: String,
    pub owner_id: String,
    pub name: String,
    pub entry_path: String, // e.g. "/plugins/acme/main.py"
    pub runtime: String,    // e.g. "python"
    pub created_at: String, // RFC3339
    pub updated_at: String, // RFC3339
}

#[derive(Debug, Insertable)]
#[diesel(table_name = plugins)]
pub struct NewPlugin<'a> {
    pub id: &'a str,
    pub project_id: &'a str,
    pub owner_id: &'a str,
    pub name: &'a str,
    pub entry_path: &'a str,
    pub runtime: &'a str,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = plugins)]
pub struct UpdatePlugin<'a> {
    pub name: Option<&'a str>,
    pub entry_path: Option<&'a str>,
    pub runtime: Option<&'a str>,
    pub updated_at: Option<&'a str>,
}

/// ======================= plugin_nodes =======================

#[derive(Debug, Clone, serde::Serialize, Queryable, Selectable)]
#[diesel(table_name = crate::schema::plugin_nodes)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct PluginNode {
    pub id: Option<i32>, // INTEGER → i32
    pub plugin_id: String,
    pub parent_id: Option<i32>, // NULLABLE → Option<i32>
    pub name: String,
    pub kind: String,
    pub created_at: String, // use Option<String> if NULL in DB
    pub updated_at: String, // use Option<String> if NULL in DB
}
#[derive(Debug, Insertable)]
#[diesel(table_name = plugin_nodes)]
pub struct NewPluginNode<'a> {
    pub plugin_id: &'a str,
    pub parent_id: Option<i32>,
    pub name: &'a str,
    pub kind: &'a str, // "dir" | "file"
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = plugin_nodes)]
pub struct UpdatePluginNode<'a> {
    pub parent_id: Option<Option<i32>>, // Some(None) to clear parent, None to ignore
    pub name: Option<&'a str>,
    pub kind: Option<&'a str>,
    pub updated_at: Option<&'a str>,
}

/// ======================= blobs =======================

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = blobs)]
pub struct Blob {
    /// INTEGER PRIMARY KEY AUTOINCREMENT
    pub id: i32,
    pub sha256_hex: String, // UNIQUE
    pub size_bytes: i32,
    pub mime: Option<String>,
    pub content: Vec<u8>, // raw bytes
}

#[derive(Debug, Insertable)]
#[diesel(table_name = blobs)]
pub struct NewBlob<'a> {
    pub sha256_hex: &'a str,
    pub size_bytes: i32,
    pub mime: Option<&'a str>,
    pub content: &'a [u8],
}

/// ======================= plugin_file_content =======================
/// 1:1 mapping: a file node -> blob

#[derive(Debug, Clone, Queryable, Identifiable, Associations, Serialize)]
#[diesel(table_name = plugin_file_content)]
#[diesel(primary_key(node_id))]
#[diesel(belongs_to(PluginNode, foreign_key = node_id))]
#[diesel(belongs_to(Blob,       foreign_key = blob_id))]
pub struct PluginFileContent {
    pub node_id: i32, // PK, FK -> plugin_nodes.id
    pub blob_id: i32, // FK -> blobs.id
    pub line_count: Option<i32>,
    pub eol: Option<String>, // 'lf' | 'crlf'
}

#[derive(Debug, Insertable)]
#[diesel(table_name = plugin_file_content)]
pub struct NewPluginFileContent {
    pub node_id: i32,
    pub blob_id: i32,
    pub line_count: Option<i32>,
    pub eol: Option<String>,
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = plugin_file_content)]
#[diesel(primary_key(node_id))]
pub struct UpdatePluginFileContent {
    pub blob_id: Option<i32>,
    pub line_count: Option<i32>,
    pub eol: Option<String>,
}

/// ======================= plugin_path_cache =======================
/// Optional: cached absolute path for fast lookups

#[derive(Debug, Clone, Queryable, Identifiable, Associations, Serialize)]
#[diesel(table_name = plugin_path_cache)]
#[diesel(primary_key(node_id))]
#[diesel(belongs_to(PluginNode, foreign_key = node_id))]
pub struct PluginPathCache {
    pub node_id: i32,     // PK, FK -> plugin_nodes.id
    pub abs_path: String, // UNIQUE
}

#[derive(Debug, Insertable)]
#[diesel(table_name = plugin_path_cache)]
pub struct NewPluginPathCache<'a> {
    pub node_id: i32,
    pub abs_path: &'a str,
}
