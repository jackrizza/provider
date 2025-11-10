![logo](iconset/primary_with_header.png)

A tiny **data provider hub** with:
- a **TCP server** for queries,
- an **HTTP API** for admin/ops,
- a **SQLite cache** (via Diesel + r2d2 pool),
- **Rust providers** (built-in Yahoo Finance),
- **hot-loadable Python providers** (no server restart).

It caches external JSON in SQLite, serves DB-first, and *stitches* gaps by only fetching missing slices from upstream.

---

## Table of contents

- [Features](#features)
- [Architecture](#architecture)
- [Quick start](#quick-start)
- [Running the servers](#running-the-servers)
- [Authentication](#authentication)
- [Client shell](#client-shell)
- [HTTP API](#http-api)
- [Providers](#providers)
  - [Rust (Yahoo Finance)](#rust-yahoo-finance)
  - [Python providers (hot-load)](#python-providers-hotload)
  - [Python provider contract](#python-provider-contract)
- [Stitching logic](#stitching-logic)
- [SQLite & Diesel](#sqlite--diesel)
- [Project layout](#project-layout)
- [Troubleshooting](#troubleshooting)
- [License](#license)

---

## Features

- **DB-first cache:** answers from SQLite if present; otherwise fetches upstream and persists.
- **Stitching:** if the user asks for a range partially cached, fetch only the missing segments and union.
- **Two protocols:**
  - **TCP** (request/response JSON per line) for low-overhead queries.
  - **HTTP (Axum)** for listing providers and loading Python plugins dynamically.
- **Hot-pluggable Python providers:** add a provider at runtime (no server restart), from a module or a file path.
- **Pretty CLI:** a small REPL client that talks TCP and can call the HTTP loader for plugins.
- **Polars-powered previews:** renders `DataFrame.head()` for entity JSON payloads in the CLI.

---

## Architecture

```
+-------------------+                     +------------------------+
|  Client (REPL)    |  TCP (line JSON)    |   Provider TCP Server  |
|  - :loadpy uses   +--------------------->  - dispatch to provider|
|    HTTP loader    |                     |  - Axum HTTP admin     |
+-------------------+                     +-----------+------------+
                                                     |
                                                     | HTTP /plugins/load
                                                     v
                                           +---------+-----------+
                                           |  Python Adapter     |
                                           |  - sys.path setup   |
                                           |  - instantiate Py   |
                                           +---------+-----------+
                                                     |
                                                     v
+----------------------+   r2d2 Pool   +------------+-------------+
|  SQLite (entities)   |<------------->|  Rust Providers (e.g.    |
|  - cached JSON rows  |               |  YahooFinance)           |
+----------------------+               +--------------------------+
```

---

## Quick start

### Prereqs

- Rust (stable), cargo
- SQLite available on your system
- Python 3.x installed (for Python providers)
- macOS users embedding Python: see [Troubleshooting](#troubleshooting)

### Build

```bash
# make sure Cargo.toml enables these:
# diesel = { version = "2.3", features = ["sqlite", "r2d2"] }
# pyo3   = { version = "0.22", features = ["auto-initialize"] }
# axum   = "0.7"

cargo build
```

### Run

```bash
# start the server (both TCP and HTTP)
# e.g. binary `main` supports flags --server --db provider.db --tcp 127.0.0.1:7000 --http 127.0.0.1:7070
target/debug/main --server --db provider.db
```

In a second terminal:

```bash
# start the client (talks to TCP)
target/debug/main --client
```

---

## Running the servers

- **TCP:** defaults to `127.0.0.1:7000`
  Protocol is **1 JSON line per request** and **1 JSON line per response** (see examples in the client).
- **HTTP (Axum):** defaults to `127.0.0.1:7070`
  Used to list providers and hot-load Python plugins.

The server keeps a **`project_base_dir`** (configured when building `ProviderServer`) and on startup adds these directories to Python `sys.path` if they exist:

- `<base>`
- `<base>/provider`
- `<base>/providers`

> This lets you organize Python plugins under `provider/` or `providers/`.

---

## Authentication

> **TL;DR:** start the server with `--auth` → if there are no users, it prints a `/setup` URL → open it in a browser to create the first user → every TCP/HTTP request must then include a token.

The provider can run in two modes:

1. **Auth disabled** (default):
```bash
target/debug/main --server --db provider.db
```
- TCP accepts all requests.
- HTTP admin endpoints accept all requests.
- The CLI can talk to the server without extra headers/fields.

Auth enabled:

```
target/debug/main --server --db provider.db --auth
```

On startup the server checks the auth table.

If it’s empty, it will log something like:

Auth is ENABLED but there are no users in the database.
Open this in a browser to create the first user: http://127.0.0.1:7070/setup

Go to that URL and submit the form to create the bootstrap user (email + password).

After that, the server issues access/refresh tokens and stores them in SQLite.

How requests are authenticated

TCP: every JSON line sent to the TCP server can include either

```json
{ "token": "<ACCESS_TOKEN>", "query": { ... } }
```
or

```json
{ "access_token": "<ACCESS_TOKEN>", "query": { ... } }
```

The server will reject unauthenticated requests with:

```json
{
  "ok": false,
  "kind": "Unauthorized",
  "error": { "code": "unauthorized", "message": "..." }
}
```
HTTP: the CLI already forwards the token to HTTP plugin endpoints as

    Authorization: Bearer <ACCESS_TOKEN>

Client support

    Core TCP client gained an AuthConfig and
    send_parsed_query_line_with_auth(...) so every higher-level client can
    transparently inject tokens.

    lib client (provider::client when built with --features lib-client) now has:


```rust
let mut client = ClientBuilder::new("127.0.0.1:7000")
    .with_token("YOUR_ACCESS_TOKEN")
    .connect()?;
client.list_providers()?;
```


```
CLI / TUI client got runtime commands:

    :auth TOKEN       # set / switch token
    :auth clear       # drop token
    :auth show        # print masked token

    and it persists the token next to the command history, so the next run
    “just works.”
```

Server-side implementation notes

    Auth logic lives in src/auth/ (service + repo + utils) and uses the same
    Diesel pool as the rest of the app.

    Tokens are UUID-based + timestamped; access tokens expire sooner, refresh
    tokens last longer.

    The TCP handler checks auth before dispatching to any provider, so all
    providers automatically become “protected” once --auth is on.

## Client shell

Start the shell:

```bash
target/debug/main --client
```

Commands:
```
:help                        Show help
:reconnect                   Reconnect TCP
:clear                       Clear screen
:addr <HOST:PORT>            Change TCP target and reconnect
:http <BASE>                 Set HTTP base (default http://127.0.0.1:7070)
:loadpy module=<mod> class=<Class> base=<project_base_dir> [name=<alias>]
:loadpy file=<abs.py> class=<Class> [name=<alias>]
:quit                        Exit
```

Examples:

```text
# point client to server’s HTTP admin (if not default)
:http http://127.0.0.1:7070

# load a provider found at <base>/providers/my_plugins/dummy.py
:loadpy module=providers.my_plugins.dummy class=Provider base=/Users/you/Code/myproj name=dummy

# or load directly from a file (no sys.path gymnastics)
:loadpy file=/Users/you/Code/myproj/providers/my_plugins/dummy.py class=Provider name=dummy
```

Query examples (TCP):
```
# list providers
provider list

# yahoo finance search
provider yahoo_finance search ticker=AAPL date=2025-09-05T00:00:00Z..2025-10-05T00:00:00Z

# newly loaded python provider
provider dummy search ticker=TSLA date=2025-09-01T00:00:00Z..2025-10-01T00:00:00Z
```

---

## HTTP API

Base: `http://127.0.0.1:7070`

### `GET /providers`
List registered providers.
```json
{ "ok": true, "providers": ["dummy", "yahoo_finance"] }
```

### `GET /providers/:name/ping`
Sanity check for a provider name.
```json
{ "ok": true, "provider": "dummy" }
```

### `POST /plugins/load`
Load a Python provider module from a base directory:
```json
{
  "module": "providers.my_plugins.dummy",
  "class": "Provider",
  "name": "dummy",
  "project_base_dir": "/Users/you/Code/myproj"
}
```

Load from a single file:
```json
{
  "file": "/Users/you/Code/myproj/providers/my_plugins/dummy.py",
  "class": "Provider",
  "name": "dummy"
}
```

Response:
```json
{ "ok": true, "name": "dummy" }
```

---

## Providers

### Rust (Yahoo Finance)

- DB-first read: returns cached range if present; otherwise hits Yahoo and persists.
- **Stitching:** if the requested date range is partially cached, it fetches only the missing segments and concatenates frames.
- Entities persisted with:
  - `source="yahoo_finance"`
  - `tags=["ticker=...","from=...","to=..."]` (JSON string)
  - `data="<json array of records>"`
  - plus metadata (etag, fetched_at, refresh_after, etc.)

### Python providers (hot-load)

The server embeds Python (PyO3). The HTTP loader:
- adds base dirs (`<base>`, `<base>/provider`, `<base>/providers`) to `sys.path`,
- imports your module or loads your file,
- instantiates your class (default `Provider`),
- registers it under an alias (`name`) in the providers registry.

After that, you can call it through TCP like any other provider.

### Python provider contract

Minimal example (`providers/my_plugins/dummy.py`):

```python
# dummy.py
class Provider:
    def name(self):
        return "dummy"  # used as default alias if not provided

    def fetch_entities(self, request_json):
        """
        request_json is a JSON object (already parsed in Python) mirroring
        the Rust-side EntityInProvider. Return a list of Entity objects.
        Each Entity is a dict with keys matching the Rust Entity struct:
          {
            "id": "...",
            "source": "dummy",
            "tags": "[\"ticker=AAPL\",\"from=...\",\"to=...\"]",  # stringified JSON array
            "data": "[{...}, ...]",  # stringified JSON array of records
            "etag": "...",
            "fetched_at": "...",
            "refresh_after": "...",
            "state": "ready",
            "last_error": "",
            "updated_at": "..."
          }
        """
        # return []
        ...

    # Optional: implement stitch(filters_json) -> Entity
    def stitch(self, filters_json):
        """
        Given filter objects, return a single Entity that represents the stitched result.
        If not implemented, the Rust adapter will report 'not supported'.
        """
        ...
```

> Your Python object doesn’t touch the DB directly—the Rust side handles persistence. If you want **bi-directional CSV/Excel** ingestion, implement your Python provider to accept a local CSV/XLSX path (or buffer), transform it to the required Entity rows, and return them; the server will persist.

---

## Stitching logic

“Stitch” = figure out which parts of a requested range are already cached, fetch **only** the missing segments, and return a single entity that represents the union. The Yahoo provider:
1. Parses the request (ticker, `from..to`).
2. Finds overlapping DB entities for that ticker.
3. Computes gaps.
4. Fetches only the gaps from Yahoo.
5. Concats cached+fresh frames (Polars), normalizes & dedups.
6. Persists/returns the final entity.

---

## SQLite & Diesel

- **Connection pooling:** `r2d2::Pool<ConnectionManager<SqliteConnection>>` (thread-safe).
- Don’t store a raw `SqliteConnection` in providers; always `pool.get()` per operation.
- Recommended PRAGMAs on startup:
  - `PRAGMA foreign_keys = ON`
  - `PRAGMA journal_mode = WAL`
  - `PRAGMA synchronous = NORMAL`

Helper (example):

```rust
pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;

pub fn establish_pool(db_path: &str) -> DbPool {
    let manager = ConnectionManager::<SqliteConnection>::new(db_path);
    let pool = Pool::builder().max_size(8).build(manager)
        .expect("create SQLite pool");
    {
        use diesel::{sql_query, RunQueryDsl};
        let mut conn = pool.get().expect("pool.get");
        let _ = sql_query("PRAGMA foreign_keys = ON").execute(&mut conn);
        let _ = sql_query("PRAGMA journal_mode = WAL").execute(&mut conn);
        let _ = sql_query("PRAGMA synchronous = NORMAL").execute(&mut conn);
    }
    pool
}
```

---

## Project layout

```
.
├── src/
│   ├── http.rs                 # Axum handlers (list providers, load plugin)
│   ├── tcp/
│   │   └── server.rs           # ProviderServer (starts TCP + HTTP)
│   ├── providers/
│   │   ├── mod.rs              # ProviderTrait (Send+Sync), registry, etc.
│   │   ├── yahoo_finance.rs    # Rust provider with stitch
│   │   └── pyprovider.rs       # PyProviderAdapter (Send+Sync wrapper)
│   ├── pyadapter.rs            # sys.path helpers (add_dirs_to_syspath)
│   ├── query.rs                # EntityInProvider, filters
│   ├── models.rs               # Entity model
│   ├── schema.rs               # Diesel schema (entities)
│   └── main.rs                 # arg parsing; starts server/client
├── provider.db                 # SQLite (created at runtime)
├── providers/                  # (recommended) Python provider packages live here
│   └── my_plugins/
│       ├── __init__.py
│       └── dummy.py
└── README.md
```

> You can also use `provider/` (singular). The server will add both `<base>/provider` and `<base>/providers` to `sys.path` (plus `<base>`).

---

## Troubleshooting

### macOS + PyO3: `Library not loaded: Python3.framework`
Your binary linked against a framework Python but dyld can’t find it. Fix by adding the correct rpath at build time:

```bash
# If you use Homebrew Python:
export PYO3_PYTHON="$(brew --prefix)/bin/python3.12"
export RUSTFLAGS="-C link-arg=-Wl,-rpath,$(brew --prefix)/Frameworks"
cargo clean && cargo build
```

Or for python.org installs:

```bash
export PYO3_PYTHON="/Library/Frameworks/Python.framework/Versions/3.12/bin/python3"
export RUSTFLAGS="-C link-arg=-Wl,-rpath,/Library/Frameworks"
cargo clean && cargo build
```

### `ModuleNotFoundError` when loading Python plugins
- Ensure you load with the correct **module** and **base**:
  If your file is `<base>/providers/my_plugins/dummy.py`, use
  `module=providers.my_plugins.dummy base=<base>`.
- The server inserts `<base>`, `<base>/provider`, and `<base>/providers` into `sys.path`.
  If you still see issues, load by file path instead:
  ```
  :loadpy file=/abs/path/to/providers/my_plugins/dummy.py class=Provider name=dummy
  ```

### Diesel/SQLite is not Send/Sync
Use the **r2d2 pool** pattern (already wired). Don’t store raw `SqliteConnection` in providers.

### Axum 0.7 routing compile errors
Use:
```rust
let listener = TcpListener::bind(addr).await?;
axum::serve(listener, app.into_make_service()).await?;
```
(not `axum::Server`).

---

## License
This project is licensed under the **AGPL-3.0-only** license. See the [LICENSE](./LICENSE) file for details.
