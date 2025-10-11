/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use diesel::prelude::*;

use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;

pub mod query;

pub mod http;
pub mod models;
pub mod providers;
pub mod pyadapter;
pub mod query_parser;
pub mod schema;
pub mod tcp;

/// Convenient alias for your app.
pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;

/// Build a thread-safe SQLite connection pool.
/// `db_path` can be a file path or ":memory:".
pub fn establish_connection(db_path: &str) -> DbPool {
    let manager = ConnectionManager::<SqliteConnection>::new(db_path);

    // Tune pool size as needed.
    let pool = Pool::builder()
        .max_size(8)
        .build(manager)
        .unwrap_or_else(|e| panic!("Error creating SQLite pool for {}: {}", db_path, e));

    // Optional: set useful SQLite PRAGMAs once.
    {
        use diesel::RunQueryDsl;
        use diesel::sql_query;

        let mut conn = pool.get().expect("pool.get() failed to set PRAGMAs");
        let _ = sql_query("PRAGMA foreign_keys = ON").execute(&mut conn);
        let _ = sql_query("PRAGMA journal_mode = WAL").execute(&mut conn);
        let _ = sql_query("PRAGMA synchronous = NORMAL").execute(&mut conn);
    }

    pool
}
