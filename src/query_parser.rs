/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/


use crate::query::{EntityFilter, EntityInProvider, QueryEnvelope, QueryEnvelopePayload};
use std::time::{SystemTime, UNIX_EPOCH};

/// Build a full QueryEnvelope (adds request_id, ts_ms, optional auth/version)
pub fn parse_line_to_envelope(
    line: &str,
    return_address: Option<String>,
    auth: Option<String>,
    version: Option<u16>,
) -> Result<QueryEnvelope, String> {
    let payload = parse_line_to_payload(line)?;
    Ok(QueryEnvelope {
        request_id: new_request_id(),
        return_address,
        v: version,
        auth,
        query: payload,
        ts_ms: Some(now_ms()),
    })
}

/// CLI → QueryEnvelopePayload
///
/// Supported:
///   providers
///   provider <name> get <id>
///   provider <name> get-many <id1,id2,...>
///   provider <name> all [limit=<n>] [offset=<n>]
///   provider <name> search [id=<id>] [source=<s>] [state=<s>]
///                         [tags=t1,t2,...] [ticker=<SYM>]
///                         [date=<start>..<end>] [updated=<start>..<end>]
///                         [limit=<n>]
///   provider <name> report url=<URL>
///   provider <name> record url=<URL>
///   (for report/record, a raw URL without `url=` is also accepted)
pub fn parse_line_to_payload(line: &str) -> Result<QueryEnvelopePayload, String> {
    let s = line.trim();
    if s.is_empty() {
        return Err("empty input".into());
    }

    let mut it = s.splitn(2, char::is_whitespace);
    let verb = it.next().unwrap().to_ascii_lowercase();
    let rest = it.next().unwrap_or("").trim();

    match verb.as_str() {
        "providers" | "provider-list" | "providerlist" => Ok(QueryEnvelopePayload::ProviderList),

        "provider" => {
            if rest.is_empty() {
                return Err("usage: provider <name> <get|get-many|all|search|report> ...".into());
            }
            let mut it2 = rest.splitn(2, char::is_whitespace);
            let provider = it2.next().unwrap().trim();
            let after_provider = it2.next().unwrap_or("").trim();

            if provider.is_empty() {
                return Err("missing <name> after `provider`".into());
            }
            if after_provider.is_empty() {
                return Err("missing subcommand after `provider <name>`".into());
            }

            let mut it3 = after_provider.splitn(2, char::is_whitespace);
            let sub = it3.next().unwrap().to_ascii_lowercase();
            let args = it3.next().unwrap_or("").trim();

            let request = match sub.as_str() {
                "get" => {
                    let id = require_nonempty(args, "id after `provider <name> get`")?;
                    EntityInProvider::GetEntity { id: id.to_string() }
                }

                "get-many" | "getmany" => {
                    if args.is_empty() {
                        return Err("usage: provider <name> get-many <id1,id2,...>".into());
                    }
                    let ids = parse_list(args);
                    if ids.is_empty() {
                        return Err("no ids provided to get-many".into());
                    }
                    EntityInProvider::GetEntities { ids }
                }

                "all" | "get-all" => {
                    let mut limit: Option<u32> = None;
                    let mut offset: Option<u32> = None;

                    for kv in tokenize(args) {
                        let (k, v) = split_kv_owned(kv)?;
                        match k.as_str() {
                            "limit" => limit = Some(parse_u32(&v, "limit")?),
                            "offset" => offset = Some(parse_u32(&v, "offset")?),
                            _ => return Err(format!("unknown arg for `all`: {k}")),
                        }
                    }

                    EntityInProvider::GetAllEntities { limit, offset }
                }

                "search" => {
                    // Multiple filters allowed; optional limit
                    let mut filters: Vec<EntityFilter> = Vec::new();
                    let mut limit: Option<u32> = None;

                    for tok in tokenize(args) {
                        let (k, v) = split_kv_owned(tok)?;
                        match k.as_str() {
                            "id" => {
                                if v.is_empty() {
                                    return Err("id filter requires a value".into());
                                }
                                filters.push(EntityFilter::ById(v));
                            }
                            "source" => {
                                if v.is_empty() {
                                    return Err("source filter requires a value".into());
                                }
                                filters.push(EntityFilter::BySource(v));
                            }
                            "state" => {
                                if v.is_empty() {
                                    return Err("state filter requires a value".into());
                                }
                                filters.push(EntityFilter::ByState(v));
                            }
                            "tags" | "tag" => {
                                let tags = parse_list(&v);
                                if tags.is_empty() {
                                    return Err("tags filter requires at least one tag".into());
                                }
                                filters.push(EntityFilter::ByTags(tags));
                            }
                            "ticker" => {
                                if v.is_empty() {
                                    return Err("ticker filter requires a value".into());
                                }
                                filters.push(EntityFilter::Ticker(v));
                            }
                            "date" | "date_range" | "daterange" => {
                                let (start, end) = parse_range(&v)?;
                                filters.push(EntityFilter::DateRange { start, end });
                            }
                            "updated" | "updated_at" | "updated_range" => {
                                let (start, end) = parse_range(&v)?;
                                filters.push(EntityFilter::ByUpdatedAtRange { start, end });
                            }
                            "limit" => {
                                limit = Some(parse_u32(&v, "limit")?);
                            }
                            other => return Err(format!("unknown search key: {other}")),
                        }
                    }

                    if filters.is_empty() {
                        return Err("search requires at least one filter".into());
                    }

                    EntityInProvider::SearchEntities {
                        query: filters,
                        limit,
                    }
                }

                // NEW: support `provider <name> report ...` and alias `record`
                "report" | "record" => {
                    let url = parse_report_url_arg(args)?;
                    EntityInProvider::GetReport { url }
                }

                _ => {
                    return Err(format!(
                        "unknown provider subcommand `{sub}`. Valid: get, get-many, all, search, report"
                    ));
                }
            };

            Ok(QueryEnvelopePayload::ProviderRequest {
                provider: provider.to_string(),
                request,
            })
        }

        _ => Err(format!(
            "unknown command `{verb}`. Valid: providers | provider <name> <get|get-many|all|search|report>"
        )),
    }
}

/* -------------------------- helpers -------------------------- */

fn parse_report_url_arg(args: &str) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: provider <name> report url=<URL> | provider <name> report <URL>".into());
    }

    // 1) Accept key=value tokens (url=...)
    for tok in tokenize(args) {
        if let Some(v) = tok.strip_prefix("url=") {
            let v = v.trim();
            if v.is_empty() {
                return Err("url= requires a value".into());
            }
            return Ok(v.to_string());
        }
    }

    // 2) Accept a bare URL token (or the whole args if it looks like a URL)
    if args.starts_with("http://") || args.starts_with("https://") {
        return Ok(args.to_string());
    }
    for tok in tokenize(args) {
        if tok.starts_with("http://") || tok.starts_with("https://") {
            return Ok(tok.to_string());
        }
    }

    Err("missing report URL; use url=<URL> or paste the URL directly".into())
}

fn require_nonempty<'a>(s: &'a str, what: &str) -> Result<&'a str, String> {
    if s.is_empty() {
        Err(format!("missing {what}"))
    } else {
        Ok(s)
    }
}

/// Tokenize by whitespace; no quoting support.
fn tokenize(s: &str) -> impl Iterator<Item = &str> {
    s.split_whitespace()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
}

/// Return owned key & value, lowercasing the key.
/// Accepts only `key=value` tokens.
fn split_kv_owned(tok: &str) -> Result<(String, String), String> {
    match tok.split_once('=') {
        Some((k, v)) => Ok((k.trim().to_ascii_lowercase(), v.trim().to_string())),
        None => Err(format!("expected key=value, got `{tok}`")),
    }
}

fn parse_u32(s: &str, name: &str) -> Result<u32, String> {
    s.parse::<u32>()
        .map_err(|_| format!("invalid {name}: `{s}` (expected u32)"))
}

/// Split "<start>..<end>" into owned Strings.
fn parse_range(v: &str) -> Result<(String, String), String> {
    v.split_once("..")
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .ok_or_else(|| format!("invalid range `{v}` (expected start..end)"))
}

/// "a,b,c" → ["a","b","c"], trimming empties.
fn parse_list(v: &str) -> Vec<String> {
    v.split(',')
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn new_request_id() -> String {
    format!("req-{}", now_ms())
}
