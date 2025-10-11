/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

use crate::query_parser::parse_line_to_envelope;
use crate::tcp::response::{ResponseEnvelope, ResponseError, ResponseKind, now_ms};

use serde_json::Value;
use std::collections::HashMap;

// NEW: blocking HTTP client for plugin loader
use reqwest::blocking::Client as HttpClient;

fn history_path() -> PathBuf {
    let mut p = dirs::data_local_dir().unwrap_or_else(|| std::env::current_dir().unwrap());
    p.push("clap-shell");
    std::fs::create_dir_all(&p).ok();
    p.push("history.txt");
    p
}

pub fn run_client(addr: &str) -> io::Result<()> {
    use rustyline::error::ReadlineError;
    use rustyline::DefaultEditor;

    // Clear the screen initially
    print!("\x1B[2J\x1B[H");
    io::stdout().flush().ok();
    println!();
    println!("Query CLI – connected target: {addr}");
    println!("Type your query and press Enter.  Commands start with ':'. Try :help");
    println!();

    // Set up line editor with persistent history
    let mut rl = DefaultEditor::new().map_err(to_io)?;
    let hist_path = history_path();
    let _ = rl.load_history(&hist_path);

    // Open a persistent TCP connection
    let mut stream = connect(addr)?;

    // Make reads snappier if server stalls
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

    // Buffered reader for responses
    let mut reader = BufReader::new(stream.try_clone()?);

    // NEW: HTTP base + client for plugin ops
    let mut http_base = "http://127.0.0.1:8080".to_string();
    let http = HttpClient::new();

    loop {
        let prompt = format!("[{}] > ", addr);
        let line = match rl.readline(&prompt) {
            Ok(l) => l,
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl-D
                println!("Bye!");
                break;
            }
            Err(e) => {
                eprintln!("readline error: {e}");
                continue;
            }
        };

        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        // Add to history if not a duplicate of the previous line
        rl.add_history_entry(input).ok();
        let _ = rl.save_history(&hist_path);

        // Handle REPL meta-commands
        if input.starts_with(':') {
            let cmd = input.trim_start_matches(':').trim();
            match cmd {
                "q" | "quit" | "exit" => {
                    println!("Bye!");
                    break;
                }
                "help" | "h" => {
                    println!(
                        "\
Commands:
  :help                        Show this help
  :reconnect                   Reconnect to the server
  :clear                       Clear the screen
  :addr <HOST:PORT>            Change TCP target address (and reconnect)
  :http <BASE>                 Set HTTP base for plugin ops (default http://127.0.0.1:7070)
  :loadpy module=<mod> class=<Class> base=<project_base_dir> [name=<alias>]
  :loadpy file=<abs.py> class=<Class> [name=<alias>]
  :quit                        Exit the client
"
                    );
                }
                "reconnect" => {
                    println!("Reconnecting to {addr} …");
                    match connect(addr) {
                        Ok(s) => {
                            stream = s;
                            stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
                            stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
                            reader = BufReader::new(stream.try_clone()?);
                            println!("Reconnected.");
                        }
                        Err(e) => eprintln!("Reconnect failed: {e}"),
                    }
                }
                "clear" => {
                    // Basic terminal clear (portable enough)
                    print!("\x1B[2J\x1B[H");
                    io::stdout().flush().ok();
                }
                s if s.starts_with("addr ") => {
                    let new_addr = s.trim_start_matches("addr").trim();
                    if new_addr.is_empty() {
                        eprintln!("Usage: :addr HOST:PORT");
                        continue;
                    }
                    println!("Switching target to {new_addr} and reconnecting …");
                    match connect(new_addr) {
                        Ok(s) => {
                            stream = s;
                            stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
                            stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
                            reader = BufReader::new(stream.try_clone()?);
                            println!("Connected to {new_addr}");
                        }
                        Err(e) => eprintln!("Connect failed: {e}"),
                    }
                }
                // NEW: set HTTP base for plugin operations
                s if s.starts_with("http ") => {
                    let new_base = s.trim_start_matches("http").trim();
                    if new_base.is_empty() {
                        eprintln!("Usage: :http http://host:port");
                    } else {
                        http_base = new_base.to_string();
                        println!("HTTP base set to {http_base}");
                    }
                }
                // NEW: load Python provider (module under <project_base_dir>/provider)
                s if s.starts_with("loadpy ") => {
                    let args = s.trim_start_matches("loadpy").trim();
                    let kv = parse_kv_args(args);

                    let url = format!("{}/plugins/load", http_base);
                    let resp = if let (Some(module), Some(class), Some(base)) =
                        (kv.get("module"), kv.get("class"), kv.get("base"))
                    {
                        http.post(&url)
                            .json(&serde_json::json!({
                                "module": module,
                                "class": class,
                                "name": kv.get("name"),
                                "project_base_dir": base
                            }))
                            .send()
                    } else if let (Some(file), Some(class)) = (kv.get("file"), kv.get("class")) {
                        http.post(&url)
                            .json(&serde_json::json!({
                                "file": file,
                                "class": class,
                                "name": kv.get("name")
                            }))
                            .send()
                    } else {
                        eprintln!(
                            "Usage:\n  :loadpy module=<mod> class=<Class> base=<project_base_dir> [name=<alias>]\n  :loadpy file=<abs.py> class=<Class> [name=<alias>]"
                        );
                        continue;
                    };

                    match resp {
                        Ok(r) => {
                            let text = r.text().unwrap_or_default();
                            println!("{text}");
                        }
                        Err(e) => eprintln!("loadpy failed: {e}"),
                    }
                }
                _ => eprintln!("Unknown command. Try :help"),
            }
            continue;
        }

        // === Parse -> send -> read-one-response (with reconnect) ===
        match send_parsed_query_line(input, addr, &mut stream, &mut reader) {
            Ok(resp) => {
                let parsed: ResponseEnvelope =
                    serde_json::from_str(&resp).unwrap_or(ResponseEnvelope {
                        ok: false,
                        request_id: None,
                        kind: ResponseKind::InvalidJson,
                        provider: None,
                        request_kind: None,
                        result: None,
                        error: Some(ResponseError {
                            code: Some("invalid_json".into()),
                            message: format!("invalid response JSON: {resp}"),
                        }),
                        ts_ms: now_ms(),
                    });
                let pretty = format_response_pretty(&parsed, /*use_color=*/ true);
                println!("{}", pretty);
            }
            Err(e) => eprintln!("request failed: {e}"),
        }
    }

    Ok(())
}

fn connect(addr: &str) -> io::Result<TcpStream> {
    match TcpStream::connect(addr) {
        Ok(s) => Ok(s),
        Err(e) => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("connect {addr} failed: {e}"),
        )),
    }
}

fn to_io<E: std::error::Error + Send + Sync + 'static>(e: E) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e)
}

/// Parse "k=v" tokens into a map: e.g. `module=my_plugins.dummy class=Provider`
fn parse_kv_args(s: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for tok in s.split_whitespace() {
        if let Some((k, v)) = tok.split_once('=') {
            m.insert(k.to_string(), v.to_string());
        }
    }
    m
}

/// Parse the CLI line into a QueryEnvelope via your provider::query_parser,
/// serialize to JSON + newline, send it, and read one response line.
/// On write/read error, automatically reconnects once and retries.
fn send_parsed_query_line(
    input: &str,
    addr: &str,
    stream: &mut TcpStream,
    reader: &mut BufReader<TcpStream>,
) -> io::Result<String> {
    // 1) Parse
    let envelope = parse_line_to_envelope(
        input,
        /* return_address */ None,
        /* auth */ None,
        /* version */ Some(1),
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    // 2) Serialize as one line of JSON
    let mut json = serde_json::to_string(&envelope)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    json.push('\n');

    // helper: write, flush, read-one-line
    fn write_then_read(
        json: &str,
        stream: &mut TcpStream,
        reader: &mut BufReader<TcpStream>,
    ) -> io::Result<String> {
        stream.write_all(json.as_bytes())?;
        stream.flush()?;

        let mut resp = String::new();
        let n = reader.read_line(&mut resp)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "server closed connection",
            ));
        }
        Ok(resp.trim_end_matches(&['\r', '\n'][..]).to_string())
    }

    // 3) Try once; on failure, reconnect and retry once
    match write_then_read(&json, stream, reader) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            eprintln!("write/read error: {e} — attempting reconnect …");

            // Try to preserve the existing timeouts
            let read_to = stream.read_timeout().ok().flatten();
            let write_to = stream.write_timeout().ok().flatten();

            // Reconnect
            let new_stream = connect(addr)?;
            if let Some(t) = read_to {
                let _ = new_stream.set_read_timeout(Some(t));
            }
            if let Some(t) = write_to {
                let _ = new_stream.set_write_timeout(Some(t));
            }

            // Swap in the new stream & reader
            *stream = new_stream;
            *reader = BufReader::new(stream.try_clone()?);

            // Retry once
            write_then_read(&json, stream, reader)
        }
    }
}

/* ---------- Rendering helpers (unchanged from your version) ---------- */

pub fn format_response_pretty(resp: &ResponseEnvelope<Value>, use_color: bool) -> String {
    // colors
    fn c(s: &str, code: &str, on: bool) -> String {
        if on {
            format!("{code}{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    let green = |s: &str| c(s, "\x1b[32m", use_color);
    let red = |s: &str| c(s, "\x1b[31m", use_color);
    let cyan = |s: &str| c(s, "\x1b[36m", use_color);
    let bold = |s: &str| c(s, "\x1b[1m", use_color);

    let mut out = String::new();

    // header
    let icon = if resp.ok { "✔" } else { "✖" };
    let label = if resp.ok { green("OK") } else { red("ERROR") };
    let kind = match resp.kind {
        ResponseKind::ProviderList => "ProviderList",
        ResponseKind::ProviderRequest => "ProviderRequest",
        ResponseKind::InvalidJson => "InvalidJson",
    };
    let reqid = resp.request_id.as_deref().unwrap_or("-");
    let provider = resp.provider.as_deref().unwrap_or("-");
    let rkind = resp.request_kind.as_deref().unwrap_or("-");

    out.push_str(&format!(
        "{} {}  {}  req_id={}  provider={}  request={}\n",
        if resp.ok { green(icon) } else { red(icon) },
        bold(&label),
        cyan(kind),
        reqid,
        provider,
        rkind
    ));
    out.push_str(&format!("ts_ms={}\n", resp.ts_ms));

    if !resp.ok {
        if let Some(err) = &resp.error {
            let code = err.code.as_deref().unwrap_or("unknown");
            out.push_str(&format!("error.code: {}\n", red(code)));
            out.push_str(&format!("error.message: {}\n", red(&err.message)));
        } else {
            out.push_str(&format!("error: {}\n", red("unknown")));
        }
        return out;
    }

    // OK path
    match &resp.result {
        None => out.push_str("result: null\n"),
        Some(v) => {
            if let Some(arr) = v.as_array() {
                // ARRAY result
                if arr.first().map(looks_like_entity).unwrap_or(false) {
                    for (i, ent) in arr.iter().enumerate() {
                        out.push_str(&format!("entity[{}]\n", i));
                        match render_entity_head(ent, 5, 24) {
                            Ok(s) => {
                                out.push_str(&indent(&s, 2));
                                out.push('\n');
                            }
                            // on render error, pretty-print the entity JSON
                            Err(_e) => {
                                let pretty = serde_json::to_string_pretty(ent)
                                    .unwrap_or_else(|_| ent.to_string());
                                out.push_str(&indent(&pretty, 2));
                                out.push('\n');
                            }
                        }
                    }
                } else if arr.iter().all(|x| x.is_string()) {
                    out.push_str("providers:\n");
                    for s in arr.iter().filter_map(|x| x.as_str()) {
                        out.push_str(&format!("  - {}\n", s));
                    }
                } else {
                    let pretty = serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string());
                    out.push_str("result:\n");
                    out.push_str(&indent(&pretty, 2));
                    out.push('\n');
                }
            } else if looks_like_entity(v) {
                // SINGLE entity
                match render_entity_head(v, 5, 24) {
                    Ok(s) => {
                        out.push_str("entity\n");
                        out.push_str(&indent(&s, 2));
                        out.push('\n');
                    }
                    Err(_e) => {
                        let pretty =
                            serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string());
                        out.push_str(&indent(&pretty, 2));
                        out.push('\n');
                    }
                }
            } else {
                // fallback
                let pretty = serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string());
                out.push_str("result:\n");
                out.push_str(&indent(&pretty, 2));
                out.push('\n');
            }
        }
    }
    out
}

fn looks_like_entity(v: &Value) -> bool {
    v.get("data").and_then(|d| d.as_str()).is_some()
}

fn indent(s: &str, n: usize) -> String {
    let pad = " ".repeat(n);
    s.lines()
        .map(|l| format!("{pad}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render Entity header + data head() table.
/// Assumes entity.data is a JSON array of objects (records).
pub fn render_entity_head(
    entity: &Value,
    max_rows: usize,
    max_col_width: usize,
) -> Result<String, String> {
    let mut out = String::new();

    let id = entity.get("id").and_then(|x| x.as_str()).unwrap_or("-");
    let source = entity.get("source").and_then(|x| x.as_str()).unwrap_or("-");
    let tags_json = entity.get("tags").and_then(|x| x.as_str()).unwrap_or("[]");
    let tags_vec: Vec<String> = serde_json::from_str(tags_json).unwrap_or_default();
    let tags = tag_map(&tags_vec);

    out.push_str(&format!(
        "id={}  source={}  ticker={}  from={}  to={}\n",
        id,
        source,
        tags.get("ticker").map(String::as_str).unwrap_or("-"),
        tags.get("from").map(String::as_str).unwrap_or("-"),
        tags.get("to").map(String::as_str).unwrap_or("-"),
    ));

    let data_str = entity
        .get("data")
        .and_then(|x| x.as_str())
        .ok_or("entity.data missing or not a string")?;

    // 1) Try Polars DataFrame.head()
    if let Some(table) = try_render_df_head_from_json(data_str, max_rows) {
        out.push_str(&table);
        return Ok(out);
    }

    // 2) Fallback: ASCII table from JSON (keeps your previous behavior)
    let data_val: Value =
        serde_json::from_str(data_str).map_err(|e| format!("bad data JSON: {e}"))?;
    match json_records_head(&data_val, max_rows, max_col_width) {
        Ok(tbl) => {
            out.push_str(&tbl);
            Ok(out)
        }
        Err(_) => {
            // 3) Last resort: pretty print the raw entity JSON
            let pretty =
                serde_json::to_string_pretty(entity).unwrap_or_else(|_| entity.to_string());
            out.push_str(&pretty);
            Ok(out)
        }
    }
}

fn tag_map(tags: &[String]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for t in tags {
        if let Some((k, v)) = t.split_once('=') {
            m.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    m
}

/// Render a JSON array of row-objects as a minimal ASCII table head().
fn json_records_head(
    rows_val: &Value,
    max_rows: usize,
    max_col_width: usize,
) -> Result<String, String> {
    let rows = rows_val.as_array().ok_or("data is not an array")?;
    if rows.is_empty() {
        return Ok("(empty)".into());
    }

    // use first row for columns
    let first = rows[0].as_object().ok_or("data[0] is not an object")?;
    let cols: Vec<String> = first.keys().cloned().collect();

    // widths init from headers
    let mut widths: Vec<usize> = cols.iter().map(|c| c.len()).collect();

    // calc widths from first N rows
    let limit = rows.len().min(max_rows);
    for r in 0..limit {
        let obj = rows[r]
            .as_object()
            .ok_or_else(|| format!("data[{r}] is not an object"))?;
        for (ci, col) in cols.iter().enumerate() {
            let cell = obj.get(col).unwrap_or(&Value::Null);
            let s = cell_display(cell);
            let w = s.chars().count().min(max_col_width);
            if w > widths[ci] {
                widths[ci] = w;
            }
        }
    }
    // cap widths
    for w in &mut widths {
        if *w > max_col_width {
            *w = max_col_width;
        }
    }

    // build table
    let mut s = String::new();
    s.push_str(&row_line(&cols, &widths, true));
    s.push_str(&sep_line(&widths));
    for r in 0..limit {
        let obj = rows[r].as_object().unwrap(); // safe
        let cells: Vec<String> = cols
            .iter()
            .map(|c| cell_display(obj.get(c).unwrap_or(&Value::Null)))
            .collect();
        s.push_str(&row_line(&cells, &widths, false));
    }
    if rows.len() > limit {
        s.push_str(&format!("… {} more rows\n", rows.len() - limit));
    }
    Ok(s)
}

fn cell_display(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(_) => "[…]".into(),
        Value::Object(_) => "{…}".into(),
    }
}

fn row_line(cells: &[String], widths: &[usize], header: bool) -> String {
    let mut s = String::new();
    for (i, cell) in cells.iter().enumerate() {
        let w = widths[i];
        let clipped = clip(cell, w);
        if i == 0 {
            s.push('|');
        }
        s.push(' ');
        s.push_str(&pad_right(&clipped, w));
        s.push(' ');
        s.push('|');
    }
    s.push('\n');
    if header {
        s.push_str(&sep_line(widths));
    }
    s
}

fn sep_line(widths: &[usize]) -> String {
    let mut s = String::new();
    for (i, w) in widths.iter().enumerate() {
        if i == 0 {
            s.push('|');
        }
        s.push_str(&"-".repeat(*w + 2));
        s.push('|');
    }
    s.push('\n');
    s
}

fn pad_right(s: &str, w: usize) -> String {
    let len = s.chars().count();
    if len >= w {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(w - len))
    }
}

fn clip(s: &str, w: usize) -> String {
    s.chars().take(w).collect()
}

use polars::prelude::*;
use std::io::Cursor;

/// Try to parse a JSON array-of-objects into a Polars DataFrame and
/// return `df.head(n)` as a pretty-printed table string.
/// Returns None if parsing/printing fails.
fn try_render_df_head_from_json(json: &str, n: usize) -> Option<String> {
    let reader =
        JsonReader::new(Cursor::new(json.as_bytes())).with_json_format(JsonFormat::Json);
    let df = reader.finish().ok()?;
    let head = df.head(Some(n));
    Some(format!("{head}"))
}
