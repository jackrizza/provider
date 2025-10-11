/*
SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza
*/

use crate::tcp::client::{connect, send_parsed_query_line};

use crossterm::event::KeyEventKind;
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::query_parser::parse_line_to_envelope;
use crate::tcp::response::{ResponseEnvelope, ResponseError, ResponseKind, now_ms};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use polars::prelude::*;
use serde_json::Value;
use std::io::Cursor;

// HTTP client for plugin loader
use reqwest::blocking::Client as HttpClient;

// -------------------- App State --------------------

struct App {
    addr: String,
    http_base: String,
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    http: HttpClient,

    // UI state
    input: String,
    cursor: usize,
    messages: Vec<String>, // rendered lines
    scroll: u16,

    // history (for up/down)
    history: Vec<String>,
    history_idx: Option<usize>,

    // misc
    last_tick: Instant,
    scroll_top: u16,
    autoscroll: bool,
}

impl App {
    fn push_msg<S: Into<String>>(&mut self, s: S) {
        self.messages.push(s.into());
    }

    fn reconnect(&mut self) -> io::Result<()> {
        let addr = self.addr.clone();
        let mut s = connect(&addr)?;
        s.set_read_timeout(Some(Duration::from_secs(10))).ok();
        s.set_write_timeout(Some(Duration::from_secs(5))).ok();
        self.reader = BufReader::new(s.try_clone()?);
        self.stream = s;
        Ok(())
    }
}

// -------------------- Public entry --------------------

pub fn run_client(addr: &str) -> io::Result<()> {
    let mut stream = connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
    let reader = BufReader::new(stream.try_clone()?);

    // seed app state
    let mut app = App {
        addr: addr.to_string(),
        http_base: "http://127.0.0.1:7070".to_string(),
        stream,
        reader,
        http: HttpClient::new(),
        input: String::new(),
        cursor: 0,
        messages: vec![
            format!("Query TUI connected → {}", addr),
            "Type a query and press Enter. Commands start with ':' (try :help)".to_string(),
        ],
        scroll: 0,
        history: load_history(),
        history_idx: None,
        last_tick: Instant::now(),
        scroll_top: 0,
        autoscroll: true,
    };

    // TUI init
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let res = run_loop(&mut terminal, &mut app);

    // teardown TUI
    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .ok();
    terminal.show_cursor().ok();

    // persist history
    save_history(&app.history);

    res
}

// -------------------- Event loop --------------------

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    let tick_rate = Duration::from_millis(100);

    loop {
        terminal.draw(|f| ui(f, app))?;

        // poll lets us avoid blocking forever on read(), so UI stays responsive
        if crossterm::event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                // only react to actual key presses
                if key.kind == KeyEventKind::Press {
                    if handle_key(app, key)? {
                        break; // true means "quit"
                    }
                }
            }
        }
        // (optional) you can handle Event::Resize here if needed
    }
    Ok(())
}

// -------------------- UI --------------------

fn ui(f: &mut ratatui::Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status
            Constraint::Min(3),    // output
            Constraint::Length(1), // help
            Constraint::Length(3), // input
        ])
        .split(f.size());

    render_status(f, chunks[0], app);
    render_output(f, chunks[1], app);
    render_help(f, chunks[2]);
    render_input(f, chunks[3], app);
}

fn render_status(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let line = Line::from(vec![
        Span::styled(
            " Provider Client ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("TCP {}", app.addr),
            Style::default().fg(Color::Green),
        ),
        Span::raw("   "),
        Span::styled(
            format!("HTTP {}", app.http_base),
            Style::default().fg(Color::Yellow),
        ),
    ]);
    let p = Paragraph::new(line).block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(p, area);
}

fn render_output(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let mut text = Text::default();
    for line in &app.messages {
        text.lines.push(style_line(line));
    }

    let inner_rows = area.height.saturating_sub(2);
    let total_lines = app.messages.len() as u16;
    let desired_bottom_top = total_lines.saturating_sub(inner_rows);
    let top = if app.autoscroll {
        desired_bottom_top
    } else {
        app.scroll_top.min(desired_bottom_top)
    };

    let para = Paragraph::new(text)
        .block(Block::default().title("Output").borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((top, 0));

    f.render_widget(para, area);
}

fn render_help(f: &mut ratatui::Frame, area: Rect) {
    let help = Line::from(vec![
        Span::styled(":help", Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled(":reconnect", Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled(":addr HOST:PORT", Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled(":http http://host:port", Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled(":loadpy ...", Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled(
            "PgUp/PgDn scroll • Ctrl+C quit",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let p = Paragraph::new(help).block(Block::default().borders(Borders::TOP));
    f.render_widget(p, area);
}

fn render_input(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let block = Block::default().title("Input").borders(Borders::ALL);
    let inner = block.inner(area);

    let display = app.input.clone();
    let para = Paragraph::new(display)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);

    // place cursor
    let x = (inner.x + app.cursor as u16).min(inner.x + inner.width.saturating_sub(1));
    let y = inner.y;
    f.set_cursor(x, y);
}

// -------------------- Key handling --------------------
fn handle_key(app: &mut App, key: KeyEvent) -> io::Result<bool> {
    // --- global exits ---
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        // Ctrl+C: exit
        return Ok(true);
    }
    if matches!(key.code, KeyCode::Esc) {
        // ESC: optional exit if you want
        // return Ok(true);
    }

    match key.code {
        // ---- scrolling (output pane) ----
        KeyCode::PageUp => {
            app.autoscroll = false;
            app.scroll_top = app.scroll_top.saturating_sub(10);
        }
        KeyCode::PageDown => {
            app.scroll_top = app.scroll_top.saturating_add(10);
            // if you want to auto-resume when near bottom, you can toggle app.autoscroll here.
        }
        KeyCode::Home => {
            app.autoscroll = false;
            app.scroll_top = 0;
        }
        KeyCode::End => {
            app.autoscroll = true; // pin to bottom
        }

        // ---- history navigation (input line) ----
        KeyCode::Up => {
            if app.history.is_empty() {
                return Ok(false);
            }
            let next_idx = match app.history_idx {
                None => Some(app.history.len().saturating_sub(1)),
                Some(0) => Some(0),
                Some(i) => Some(i.saturating_sub(1)),
            };
            if let Some(i) = next_idx {
                app.history_idx = Some(i);
                app.input = app.history[i].clone();
                app.cursor = app.input.len();
            }
        }
        KeyCode::Down => {
            if app.history.is_empty() {
                return Ok(false);
            }
            let next_idx = match app.history_idx {
                None => return Ok(false),
                Some(i) if i + 1 >= app.history.len() => {
                    // past the newest: clear input and reset idx
                    app.history_idx = None;
                    app.input.clear();
                    app.cursor = 0;
                    return Ok(false);
                }
                Some(i) => Some(i + 1),
            };
            if let Some(i) = next_idx {
                app.history_idx = Some(i);
                app.input = app.history[i].clone();
                app.cursor = app.input.len();
            }
        }

        // ---- input editing ----
        KeyCode::Left => {
            if app.cursor > 0 {
                app.cursor -= 1;
            }
        }
        KeyCode::Right => {
            if app.cursor < app.input.len() {
                app.cursor += 1;
            }
        }
        KeyCode::Backspace => {
            if app.cursor > 0 && !app.input.is_empty() {
                let pos = app.cursor;
                app.input.remove(pos - 1);
                app.cursor -= 1;
            }
        }
        KeyCode::Delete => {
            if app.cursor < app.input.len() && !app.input.is_empty() {
                let pos = app.cursor;
                app.input.remove(pos);
            }
        }
        // Ctrl+U = clear to start, Ctrl+K = clear to end, Ctrl+L = clear screen
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.input.replace_range(..app.cursor, "");
            app.cursor = 0;
        }
        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.input.replace_range(app.cursor.., "");
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.messages.clear();
            app.autoscroll = true;
        }

        // typing (printable)
        KeyCode::Char(ch) => {
            // ignore control-modified printable chars except Ctrl+ combos we handled above
            if !key.modifiers.contains(KeyModifiers::CONTROL) {
                app.input.insert(app.cursor, ch);
                app.cursor += 1;
            }
        }

        // ---- submit ----
        KeyCode::Enter => {
            let line = app.input.trim().to_string();
            if line.is_empty() {
                return Ok(false);
            }

            // Echo input into output pane
            app.push_msg(format!("› {}", line));

            // Save into history (de-dupe last)
            if app.history.last().map(|s| s.as_str()) != Some(line.as_str()) {
                app.history.push(line.clone());
            }
            app.history_idx = None;

            // Reset autoscroll (new content)
            app.autoscroll = true;

            if let Some(rest) = line.strip_prefix(':') {
                // command mode
                if let Err(e) = handle_command(app, rest) {
                    app.push_msg(format!("command error: {e}"));
                }
            } else {
                // send to TCP server
                match send_parsed_query_line(&line, &app.addr, &mut app.stream, &mut app.reader) {
                    Ok(resp) => {
                        let parsed: ResponseEnvelope<Value> = serde_json::from_str(&resp)
                            .unwrap_or(ResponseEnvelope {
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
                        let pretty = format_response_pretty(&parsed, false);
                        for l in pretty.lines() {
                            app.push_msg(l.to_string());
                        }
                        // NEW: spacer line between responses
                        app.push_msg("");
                    }
                    Err(e) => app.push_msg(format!("request failed: {e}")),
                }
            }

            // clear input after processing
            app.input.clear();
            app.cursor = 0;
        }

        _ => {}
    }

    Ok(false)
}

// -------------------- Commands --------------------

fn handle_command(app: &mut App, cmdline: &str) -> io::Result<()> {
    let cmdline = cmdline.trim();
    if cmdline.is_empty() {
        return Ok(());
    }

    match cmdline {
        "q" | "quit" | "exit" => {
            // handled by caller (we return true), but we’ll just print instruction here
            app.push_msg("Use Ctrl+C to exit.");
        }
        "help" | "h" => {
            app.push_msg("Commands:");
            app.push_msg("  :help                         Show this help");
            app.push_msg("  :reconnect                    Reconnect to the server");
            app.push_msg(
                "  :addr HOST:PORT               Change TCP target address (and reconnect)",
            );
            app.push_msg("  :http http://host:port        Set HTTP base for plugin ops");
            app.push_msg(
                "  :loadpy module=<mod> class=<Class> base=<project_base_dir> [name=<alias>]",
            );
            app.push_msg("  :loadpy file=<abs.py> class=<Class> [name=<alias>]");
            app.push_msg("  :clear                        Clear screen");
        }
        "reconnect" => match app.reconnect() {
            Ok(()) => app.push_msg(format!("Reconnected to {}", app.addr)),
            Err(e) => app.push_msg(format!("Reconnect failed: {e}")),
        },
        "clear" => {
            app.messages.clear();
        }
        _ if cmdline.starts_with("addr ") => {
            let new_addr = cmdline.trim_start_matches("addr").trim();
            if new_addr.is_empty() {
                app.push_msg("Usage: :addr HOST:PORT");
            } else {
                app.addr = new_addr.to_string();
                match app.reconnect() {
                    Ok(()) => app.push_msg(format!("Connected to {}", app.addr)),
                    Err(e) => app.push_msg(format!("Connect failed: {e}")),
                }
            }
        }
        _ if cmdline.starts_with("http ") => {
            let new_base = cmdline.trim_start_matches("http").trim();
            if new_base.is_empty() {
                app.push_msg("Usage: :http http://host:port");
            } else {
                app.http_base = new_base.to_string();
                app.push_msg(format!("HTTP base set to {}", app.http_base));
            }
        }
        _ if cmdline.starts_with("loadpy ") => {
            let args = cmdline.trim_start_matches("loadpy").trim();
            let kv = parse_kv_args(args);

            let url = format!("{}/plugins/load", app.http_base);
            let resp = if let (Some(module), Some(class), Some(base)) =
                (kv.get("module"), kv.get("class"), kv.get("base"))
            {
                app.http
                    .post(&url)
                    .json(&serde_json::json!({
                        "module": module,
                        "class": class,
                        "name": kv.get("name"),
                        "project_base_dir": base
                    }))
                    .send()
            } else if let (Some(file), Some(class)) = (kv.get("file"), kv.get("class")) {
                app.http
                    .post(&url)
                    .json(&serde_json::json!({
                        "file": file,
                        "class": class,
                        "name": kv.get("name")
                    }))
                    .send()
            } else {
                app.push_msg("Usage:\n  :loadpy module=<mod> class=<Class> base=<project_base_dir> [name=<alias>]\n  :loadpy file=<abs.py> class=<Class> [name=<alias>]");
                return Ok(());
            };

            match resp {
                Ok(r) => {
                    let text = r.text().unwrap_or_default();
                    let pretty = format_response_pretty(
                        &serde_json::from_str(&text).unwrap_or(ResponseEnvelope {
                            ok: false,
                            request_id: None,
                            kind: ResponseKind::InvalidJson,
                            provider: None,
                            request_kind: None,
                            result: None,
                            error: Some(ResponseError {
                                code: Some("invalid_json".into()),
                                message: format!("invalid response JSON: {text}"),
                            }),
                            ts_ms: now_ms(),
                        }),
                        false,
                    );
                    for l in pretty.lines() {
                        app.push_msg(l.to_string());
                    }
                }
                Err(e) => app.push_msg(format!("loadpy failed: {e}")),
            }
        }
        _ => app.push_msg("Unknown command. Try :help"),
    }

    Ok(())
}

// -------------------- Helpers --------------------

fn history_path() -> PathBuf {
    let mut p = dirs::data_local_dir().unwrap_or_else(|| std::env::current_dir().unwrap());
    p.push("clap-shell");
    std::fs::create_dir_all(&p).ok();
    p.push("history.txt");
    p
}

fn load_history() -> Vec<String> {
    use std::fs::File;
    use std::io::Read;
    let mut out = Vec::new();
    if let Ok(mut f) = File::open(history_path()) {
        let mut s = String::new();
        if f.read_to_string(&mut s).is_ok() {
            for line in s.lines() {
                if !line.trim().is_empty() {
                    out.push(line.trim().to_string());
                }
            }
        }
    }
    out
}

fn save_history(hist: &[String]) {
    use std::fs::File;
    use std::io::Write;
    if let Ok(mut f) = File::create(history_path()) {
        for l in hist {
            let _ = writeln!(f, "{l}");
        }
    }
}

// fn connect(addr: &str) -> io::Result<TcpStream> {
//     match TcpStream::connect(addr) {
//         Ok(s) => Ok(s),
//         Err(e) => Err(io::Error::new(
//             io::ErrorKind::Other,
//             format!("connect {addr} failed: {e}"),
//         )),
//     }
// }

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
// fn send_parsed_query_line(
//     input: &str,
//     addr: &str,
//     stream: &mut TcpStream,
//     reader: &mut BufReader<TcpStream>,
// ) -> io::Result<String> {
//     // 1) Parse
//     let envelope = parse_line_to_envelope(input, None, None, Some(1))
//         .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

//     // 2) Serialize as one line of JSON
//     let mut json = serde_json::to_string(&envelope)
//         .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
//     json.push('\n');

//     // helper: write, flush, read-one-line
//     fn write_then_read(
//         json: &str,
//         stream: &mut TcpStream,
//         reader: &mut BufReader<TcpStream>,
//     ) -> io::Result<String> {
//         stream.write_all(json.as_bytes())?;
//         stream.flush()?;

//         let mut resp = String::new();
//         let n = reader.read_line(&mut resp)?;
//         if n == 0 {
//             return Err(io::Error::new(
//                 io::ErrorKind::UnexpectedEof,
//                 "server closed connection",
//             ));
//         }
//         Ok(resp.trim_end_matches(&['\r', '\n'][..]).to_string())
//     }

//     // 3) Try once; on failure, reconnect and retry once
//     match write_then_read(&json, stream, reader) {
//         Ok(resp) => Ok(resp),
//         Err(e) => {
//             eprintln!("write/read error: {e} — attempting reconnect …");

//             // Try to preserve the existing timeouts
//             let read_to = stream.read_timeout().ok().flatten();
//             let write_to = stream.write_timeout().ok().flatten();

//             // Reconnect
//             let new_stream = connect(addr)?;
//             if let Some(t) = read_to {
//                 let _ = new_stream.set_read_timeout(Some(t));
//             }
//             if let Some(t) = write_to {
//                 let _ = new_stream.set_write_timeout(Some(t));
//             }

//             // Swap in the new stream & reader
//             *stream = new_stream;
//             *reader = BufReader::new(stream.try_clone()?);

//             // Retry once
//             write_then_read(&json, stream, reader)
//         }
//     }
// }

/* ---------- Rendering helpers (reused) ---------- */

pub fn format_response_pretty(resp: &ResponseEnvelope<Value>, _use_color: bool) -> String {
    // (No ANSI colors inside TUI; we style in ratatui instead)
    let mut out = String::new();

    // header
    let icon = if resp.ok { "✔ OK" } else { "✖ ERROR" };
    let kind = match resp.kind {
        ResponseKind::ProviderList => "ProviderList",
        ResponseKind::ProviderRequest => "ProviderRequest",
        ResponseKind::InvalidJson => "InvalidJson",
    };
    let reqid = resp.request_id.as_deref().unwrap_or("-");
    let provider = resp.provider.as_deref().unwrap_or("-");
    let rkind = resp.request_kind.as_deref().unwrap_or("-");
    out.push_str(&format!(
        "{icon}  {kind}  req_id={reqid}  provider={provider}  request={rkind}\n"
    ));
    out.push_str(&format!("ts_ms={}\n", resp.ts_ms));

    if !resp.ok {
        if let Some(err) = &resp.error {
            let code = err.code.as_deref().unwrap_or("unknown");
            out.push_str(&format!("error.code: {}\n", code));
            out.push_str(&format!("error.message: {}\n", &err.message));
        } else {
            out.push_str("error: unknown\n");
        }
        return out;
    }

    match &resp.result {
        None => out.push_str("result: null\n"),
        Some(v) => {
            if let Some(arr) = v.as_array() {
                if arr.first().map(looks_like_entity).unwrap_or(false) {
                    for (i, ent) in arr.iter().enumerate() {
                        out.push_str(&format!("entity[{i}]\n"));
                        match render_entity_head(ent, 5, 24) {
                            Ok(s) => {
                                out.push_str(&indent(&s, 2));
                                out.push('\n');
                            }
                            Err(_) => {
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
                match render_entity_head(v, 5, 24) {
                    Ok(s) => {
                        out.push_str("entity\n");
                        out.push_str(&indent(&s, 2));
                        out.push('\n');
                    }
                    Err(_) => {
                        let pretty =
                            serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string());
                        out.push_str(&indent(&pretty, 2));
                        out.push('\n');
                    }
                }
            } else {
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

    if let Some(table) = try_render_df_head_from_json(data_str, max_rows) {
        out.push_str(&table);
        return Ok(out);
    }

    let data_val: Value =
        serde_json::from_str(data_str).map_err(|e| format!("bad data JSON: {e}"))?;
    match json_records_head(&data_val, max_rows, max_col_width) {
        Ok(tbl) => {
            out.push_str(&tbl);
            Ok(out)
        }
        Err(_) => {
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

fn json_records_head(
    rows_val: &Value,
    max_rows: usize,
    max_col_width: usize,
) -> Result<String, String> {
    let rows = rows_val.as_array().ok_or("data is not an array")?;
    if rows.is_empty() {
        return Ok("(empty)".into());
    }

    let first = rows[0].as_object().ok_or("data[0] is not an object")?;
    let cols: Vec<String> = first.keys().cloned().collect();

    let mut widths: Vec<usize> = cols.iter().map(|c| c.len()).collect();

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

    for w in &mut widths {
        if *w > max_col_width {
            *w = max_col_width;
        }
    }

    let mut s = String::new();
    s.push_str(&row_line(&cols, &widths, true));
    s.push_str(&sep_line(&widths));
    for r in 0..limit {
        let obj = rows[r].as_object().unwrap();
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

fn try_render_df_head_from_json(json: &str, n: usize) -> Option<String> {
    let reader = JsonReader::new(Cursor::new(json.as_bytes())).with_json_format(JsonFormat::Json);
    let df = reader.finish().ok()?;
    let head = df.head(Some(n));
    Some(format!("{head}"))
}

fn style_line(s: &str) -> Line<'_> {
    use ratatui::style::{Color, Modifier, Style};

    // common styles
    let dim = Style::default().fg(Color::DarkGray);
    let normal = Style::default().fg(Color::White);
    let bold = Style::default().add_modifier(Modifier::BOLD);

    // heuristics
    if s.starts_with("✔ OK") {
        return Line::from(s.to_string()).style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );
    }
    if s.starts_with("✖ ERROR") || s.starts_with("error.") || s.contains(" provider_request_failed")
    {
        return Line::from(s.to_string())
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
    }
    if s.starts_with("› ") {
        return Line::from(s.to_string()).style(Style::default().fg(Color::Cyan));
    }
    if s.starts_with("entity[") || s == "entity" {
        return Line::from(s.to_string()).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    }
    if s.starts_with("providers:") || s.starts_with("result:") {
        return Line::from(s.to_string()).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    }
    if s.starts_with('|') {
        // table rows from the ASCII/Polars output
        // headers bold, separators dim
        if s.contains("---") {
            return Line::from(s.to_string()).style(dim);
        } else {
            return Line::from(s.to_string()).style(normal);
        }
    }
    if s.starts_with("id=") {
        return Line::from(s.to_string()).style(bold.fg(Color::Magenta));
    }
    if s.trim().is_empty() {
        return Line::from("".to_string()); // keep blank lines
    }
    Line::from(s.to_string()).style(normal)
}

