use std::io::{self, BufReader};
use std::net::TcpStream;

use crate::tcp_client_common::{connect, send_parsed_query_line};

/// A simple programmatic client for Provider TCP API.
pub struct Client {
    addr: String,
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

pub struct ClientBuilder {
    addr: String,
}

impl ClientBuilder {
    pub fn new(addr: impl Into<String>) -> Self {
        Self { addr: addr.into() }
    }

    pub fn connect(self) -> io::Result<Client> {
        let stream = connect(&self.addr)?;
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Client { addr: self.addr, stream, reader })
    }
}

impl Client {
    /// Send a raw shell line (same as typing in the CLI, e.g. `provider yahoo_finance search ...`)
    pub fn send_line(&mut self, line: &str) -> io::Result<Response> {
        let resp = send_parsed_query_line(line, &self.addr, &mut self.stream, &mut self.reader)?;
        Ok(Response { raw: resp })
    }

    /// Convenience: list providers
    pub fn list_providers(&mut self) -> io::Result<Response> {
        self.send_line("providers")
    }

    /// Convenience: provider request, e.g. `provider yahoo_finance search ticker=AAPL`
    pub fn provider(&mut self, line: &str) -> io::Result<Response> {
        self.send_line(line)
    }
}

/// A typed wrapper around the raw response JSON for convenience.
#[derive(Debug, Clone)]
pub struct Response {
    pub raw: String,
}

impl Response {
    pub fn as_json(&self) -> serde_json::Result<serde_json::Value> {
        serde_json::from_str(&self.raw)
    }
}
