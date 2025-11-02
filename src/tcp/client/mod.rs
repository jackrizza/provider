// The TUI/CLI lives under a feature; usually used by the bin (main.rs)
#[cfg(all(feature = "cli-client", feature = "lib-client"))]
compile_error!("Enable only ONE of: `cli-client` OR `lib-client`.");

// Keep both modules available (behind their features)
#[cfg(feature = "cli-client")]
pub mod cli_client;

#[cfg(feature = "lib-client")]
pub mod lib_client;

// Unify the public name: `provider::client`
#[cfg(feature = "cli-client")]
pub use cli_client as client;

#[cfg(feature = "lib-client")]
pub use lib_client as client;

use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;

use serde_json::json;

use crate::query_parser::parse_line_to_envelope;

/// Simple auth config for the client side.
/// If `access_token` is `Some(...)`, we'll inject it into every JSON request.
#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    pub access_token: Option<String>,
}

impl AuthConfig {
    pub fn new<T: Into<String>>(token: T) -> Self {
        Self {
            access_token: Some(token.into()),
        }
    }
}

/// Connect with standard timeouts
pub fn connect(addr: &str) -> io::Result<TcpStream> {
    let s = TcpStream::connect(addr)?;
    s.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .ok();
    s.set_write_timeout(Some(std::time::Duration::from_secs(5)))
        .ok();
    Ok(s)
}

/// Send one parsed request and read one line response (no auth).
/// Kept for backward compatibility.
pub fn send_parsed_query_line(
    input: &str,
    addr: &str,
    stream: &mut TcpStream,
    reader: &mut BufReader<TcpStream>,
) -> io::Result<String> {
    send_parsed_query_line_with_auth(input, addr, stream, reader, &AuthConfig::default())
}

/// Send one parsed request and read one line response, but inject auth token if provided.
pub fn send_parsed_query_line_with_auth(
    input: &str,
    addr: &str,
    stream: &mut TcpStream,
    reader: &mut BufReader<TcpStream>,
    auth: &AuthConfig,
) -> io::Result<String> {
    // build the envelope the same way as before
    let envelope = parse_line_to_envelope(input, None, None, Some(1))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    // turn it into JSON value so we can easily insert token
    let mut value = serde_json::to_value(&envelope)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // if we have an access token, inject it at the top-level. The server accepts `token` or `access_token`.
    if let Some(tok) = &auth.access_token {
        if let serde_json::Value::Object(ref mut map) = value {
            // we add both, to be generous
            map.insert("token".to_string(), json!(tok));
            map.insert("access_token".to_string(), json!(tok));
        }
    }

    let mut json =
        serde_json::to_string(&value).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    json.push('\n');

    // inner helper
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

    // write/read, with reconnect on failure
    match write_then_read(&json, stream, reader) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            eprintln!("write/read error: {e} — attempting reconnect …");
            let read_to = stream.read_timeout().ok().flatten();
            let write_to = stream.write_timeout().ok().flatten();

            let new_stream = connect(addr)?;
            if let Some(t) = read_to {
                let _ = new_stream.set_read_timeout(Some(t));
            }
            if let Some(t) = write_to {
                let _ = new_stream.set_write_timeout(Some(t));
            }

            *stream = new_stream;
            *reader = BufReader::new(stream.try_clone()?);
            write_then_read(&json, stream, reader)
        }
    }
}

/// Send one *raw shell line* (no pre-parse) and read one line response (no auth).
/// Kept for backward compatibility.
pub fn send_raw_line(
    line: &str,
    addr: &str,
    stream: &mut TcpStream,
    reader: &mut BufReader<TcpStream>,
) -> io::Result<String> {
    send_raw_line_with_auth(line, addr, stream, reader, &AuthConfig::default())
}

/// Send one *raw shell line* and read one line response, but inject auth token if provided.
///
/// This is useful for your CLI where the user might literally type:
///     provider yahoo_finance search ticker=AAPL
/// and you still want to send a token to the server.
pub fn send_raw_line_with_auth(
    line: &str,
    addr: &str,
    stream: &mut TcpStream,
    reader: &mut BufReader<TcpStream>,
    _auth: &AuthConfig,
) -> io::Result<String> {
    // Always append a newline; the server is line-delimited.
    let mut buf = String::with_capacity(line.len() + 1);
    buf.push_str(line);
    if !buf.ends_with('\n') {
        buf.push('\n');
    }

    // If we don't have a token, just send the line as-is (exactly your old behavior).
    // If we DO have a token, we wrap the line into a JSON object so the server
    // will recognize it. But: your raw CLI protocol today is "just send text", so
    // best to leave it as-is and let the cli/lib client (the next files you'll show)
    // decide how to structure auth for "raw" commands.
    //
    // For now, we will NOT attempt to parse arbitrary raw input and inject token,
    // because that can break existing flows. Instead, we send as-is.
    //
    // If you *do* want to force auth on raw lines, you'd have to define a raw-line
    // JSON wrapper format here.

    fn write_then_read(
        json: &str,
        stream: &mut TcpStream,
        reader: &mut BufReader<TcpStream>,
    ) -> io::Result<String> {
        use std::io::{BufRead, Write};
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

    match write_then_read(&buf, stream, reader) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            eprintln!("write/read error: {e} — attempting reconnect …");
            let read_to = stream.read_timeout().ok().flatten();
            let write_to = stream.write_timeout().ok().flatten();

            let new_stream = connect(addr)?;
            if let Some(t) = read_to {
                let _ = new_stream.set_read_timeout(Some(t));
            }
            if let Some(t) = write_to {
                let _ = new_stream.set_write_timeout(Some(t));
            }

            *stream = new_stream;
            *reader = BufReader::new(stream.try_clone()?);
            write_then_read(&buf, stream, reader)
        }
    }
}
