/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

extern crate provider;

use clap::{ArgGroup, Parser};

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

    /// HTTP Address to bind or connect to
    #[arg(long, default_value = "127.0.0.1:7070")]
    http_addr: String,

    /// sqlite database file (for server)
    #[arg(long, default_value = None)]
    db: Option<String>,
}

fn main() {
    env_logger::builder()
        .format_timestamp_millis()
        .filter_level(log::LevelFilter::Info)
        .init();

    let args = Args::parse();

    if args.server {
        let db = match args.db {
            Some(db) => db,
            None => {
                log::error!("--db <file> is required in server mode");
                std::process::exit(1);
            }
        };
        provider::tcp::server::ProviderServer::new(args.tcp_addr, args.http_addr, db).listen();
    } else if args.client {
        let _ = provider::tcp::client::run_client(&args.tcp_addr);
    }
}
