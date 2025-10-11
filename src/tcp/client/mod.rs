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

use crate::query_parser::parse_line_to_envelope;

/// Connect with standard timeouts
pub fn connect(addr: &str) -> io::Result<TcpStream> {
    let s = TcpStream::connect(addr)?;
    s.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .ok();
    s.set_write_timeout(Some(std::time::Duration::from_secs(5)))
        .ok();
    Ok(s)
}

/// Send one parsed request and read one line response.
pub fn send_parsed_query_line(
    input: &str,
    addr: &str,
    stream: &mut TcpStream,
    reader: &mut BufReader<TcpStream>,
) -> io::Result<String> {
    let envelope = parse_line_to_envelope(input, None, None, Some(1))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let mut json = serde_json::to_string(&envelope)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    json.push('\n');

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
