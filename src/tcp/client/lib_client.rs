use std::collections::HashMap;
use std::io::{self, BufReader};
use std::net::TcpStream;

use crate::models::Entity;
use crate::tcp::client::{connect, send_parsed_query_line};

/// A simple programmatic client for Provider TCP API.

#[derive(Debug)]
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
        Ok(Client {
            addr: self.addr,
            stream,
            reader,
        })
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

    //TODO: need to a a fuck_data tick for when used searching for data
    /// Convenience: provider request, e.g. `provider yahoo_finance search ticker=AAPL`
    pub fn provider(&mut self, line: &str) -> io::Result<Response> {
        self.send_line(line)
    }

    fn inner_get_multiple_responses(&mut self, lines: Vec<String>) -> io::Result<Vec<Response>> {
        let mut responses = Vec::with_capacity(lines.len());
        for line in lines {
            responses.push(self.send_line(&line)?);
        }
        Ok(responses)
    }

    fn get_multiple_responses(
        &mut self,
        lines: Vec<(String, String)>,
    ) -> io::Result<HashMap<String, serde_json::Value>> {
        let responses = self.inner_get_multiple_responses(
            lines.iter().map(|(_, df)| df.clone()).collect::<Vec<_>>(),
        )?;

        let names = lines
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();

        let mut results = HashMap::new();
        for (index, response) in responses.iter().enumerate() {
            let json = response.as_json()?;
            results.insert(names[index].clone(), json);
        }
        Ok(results)
    }

    pub fn get_data<T>(&mut self, lines: Vec<(String, String)>) -> io::Result<Vec<(String, T)>>
    where
        T: serde::de::DeserializeOwned,
    {
        let responses = self.get_multiple_responses(lines)?;
        let mut results = Vec::with_capacity(responses.len());
        for (name, json) in responses {
            let entity: Entity = serde_json::from_value(json)?;
            let value: T = serde_json::from_str(&entity.data)?;
            results.push((name, value));
        }
        Ok(results)
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
