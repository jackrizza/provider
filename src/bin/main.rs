/*
SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza
*/

extern crate provider;

use clap::{ArgGroup, Parser};
use diesel::Connection;
use diesel::RunQueryDsl;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use log::{error, info};
// Embed migrations from the default "./migrations" folder
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

#[derive(Debug, Parser)]
#[command(author, version, about)]
#[command(group(
    ArgGroup::new("role")
        .required(true)
        .args(["server", "client"])
))]
struct Args {
    /// Run as server
    #[arg(long)]
    server: bool,

    /// Run as client
    #[arg(long)]
    client: bool,

    /// TCP Address to bind or connect to
    #[arg(long, default_value = "127.0.0.1:7000")]
    tcp_addr: String,

    /// HTTP Address to bind (server) or set as default in the client shell via :http
    #[arg(long, default_value = "127.0.0.1:7070")]
    http_addr: String,

    /// sqlite database file (server mode)
    #[arg(long)]
    db: Option<String>,

    /// Project base directory (used to seed PYTHONPATH for Python providers).
    /// Defaults to current working directory in server mode.
    #[arg(long)]
    base: Option<String>,
}

#[cfg(feature = "cli-client")]
fn main() {
    env_logger::builder()
        .format_timestamp_millis()
        .filter_level(log::LevelFilter::Info)
        .init();

    let args = Args::parse();

    if args.server {
        let db_path = match args.db.clone() {
            Some(db) => db,
            None => {
                error!("--db <file> is required in server mode");
                std::process::exit(1);
            }
        };

        // 1) Ensure DB file exists and run migrations
        if let Err(e) = ensure_db_and_migrate(&db_path) {
            error!("Database init/migrate failed: {e}");
            std::process::exit(1);
        }

        // 2) Seed PYTHONPATH to help find providers
        let base_dir = args
            .base
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        seed_pythonpath(&base_dir);

        info!(
            "Starting server (TCP={}, HTTP={}, DB={}, BASE={})",
            args.tcp_addr,
            args.http_addr,
            db_path,
            base_dir.display()
        );

        provider::tcp::server::ProviderServer::new(args.tcp_addr, args.http_addr, db_path).listen();
    } else if args.client {
        // client mode (no DB init needed)
        info!("Starting client (TCP={})", args.tcp_addr);
        let _ = provider::tcp::client::client::run_client(&args.tcp_addr);
    }
}

/// Ensure the DB file exists AND apply pending Diesel migrations (embedded).
fn ensure_db_and_migrate(db_path: &str) -> Result<(), String> {
    let p = Path::new(db_path);

    // Create parent dirs if needed
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("create_dir_all({}): {e}", parent.display()))?;
        }
    }

    // Opening a sqlite file will create it if missing.
    // We reuse your establish_connection pool builder to get a connection
    // OR open a one-shot SqliteConnection for migrations.
    //
    // Option A: use the same manager/pool as your app:
    //   let pool = provider::establish_connection(db_path);
    //   let mut conn = pool.get().map_err(|e| format!("pool.get(): {e}"))?;
    //
    // Option B (simple, one-shot for migrations):
    let mut conn = SqliteConnection::establish(db_path)
        .map_err(|e| format!("SqliteConnection::establish({db_path}): {e}"))?;

    // Optional: PRAGMAs that are generally good defaults
    if let Err(e) = diesel::sql_query("PRAGMA foreign_keys = ON").execute(&mut conn) {
        log::warn!("PRAGMA foreign_keys = ON failed: {e}");
    }
    if let Err(e) = diesel::sql_query("PRAGMA journal_mode = WAL").execute(&mut conn) {
        log::warn!("PRAGMA journal_mode = WAL failed: {e}");
    }
    if let Err(e) = diesel::sql_query("PRAGMA synchronous = NORMAL").execute(&mut conn) {
        log::warn!("PRAGMA synchronous = NORMAL failed: {e}");
    }

    // Run pending migrations (embedded)
    conn.run_pending_migrations(MIGRATIONS)
        .map_err(|e| format!("run_pending_migrations: {e}"))?;

    info!("Database ready and migrations applied at {}", db_path);
    Ok(())
}

/// Seed PYTHONPATH with base, base/provider, base/providers.
fn seed_pythonpath(base_dir: &Path) {
    let mut add = Vec::new();
    add.push(base_dir.to_path_buf());
    add.push(base_dir.join("provider"));
    add.push(base_dir.join("providers"));

    // Keep only existing directories
    add.retain(|p| p.is_dir());

    if add.is_empty() {
        return;
    }

    // Prepend to existing PYTHONPATH (if any)
    let existing = env::var_os("PYTHONPATH")
        .map(|s| s.into_string().unwrap_or_default())
        .unwrap_or_default();

    // Construct the new path with platform-appropriate separator
    #[cfg(target_os = "windows")]
    let sep = ";";
    #[cfg(not(target_os = "windows"))]
    let sep = ":";

    let additions = add
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(sep);

    let new_val = if existing.is_empty() {
        additions.clone()
    } else {
        format!("{additions}{sep}{existing}")
    };

    unsafe { env::set_var("PYTHONPATH", &new_val) };
    info!("PYTHONPATH seeded with: {}", additions);
}

#[cfg(feature = "lib-client")]
fn main() {
    println!(
        "lib-client is set as a feature which allows the library to be used in a client application."
    );
    println!("If you are trying to use the cli, you can run the following command:");
    println!("cargo run -- --client --features cli-client");
}
