/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use crate::models::Entity;
use crate::query::{EntityFilter, EntityInProvider};
use crate::{establish_connection, schema::entities};
use diesel::ExpressionMethods;
use diesel::{QueryDsl, RunQueryDsl, SqliteConnection};
use polars::prelude::SerReader;
use polars::prelude::{AnyValue, DataFrame, Series, df};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use serde_json::{Value, json};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration as StdDuration;
use time::{Date, Duration, OffsetDateTime, format_description::well_known::Rfc3339};
// at top with other uses
use reqwest::header::CONTENT_TYPE;
use sha2::{Digest, Sha256};

use scraper::{Html, Selector}; // HTML table parsing
use std::io::Cursor;
use zip::ZipArchive;

/// Provider that pulls filings metadata from SEC EDGAR submissions API (data.sec.gov)
pub struct SecEdgarProvider {
    db_connect: SqliteConnection,
    http: Client,
    user_agent: String,
}

impl SecEdgarProvider {
    /// `user_agent` must include contact info, e.g. "provider/0.1 ([email protected])"
    pub fn new(db_path: &str, user_agent: &str) -> Self {
        // Required headers
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(user_agent).unwrap_or(HeaderValue::from_static("provider/0.1")),
        );

        let http = Client::builder()
            .default_headers(headers)
            .timeout(StdDuration::from_secs(20))
            .build()
            .expect("reqwest client");

        Self {
            db_connect: establish_connection(db_path),
            http,
            user_agent: user_agent.to_string(),
        }
    }

    /* ========================= DB helpers ========================= */

    fn store_entity(&mut self, entity: &Entity) -> Result<(), String> {
        match diesel::insert_into(entities::table)
            .values(entity)
            .execute(&mut self.db_connect)
        {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Database insert error: {}", e)),
        }
    }

    fn get_all(&mut self) -> Result<Vec<Entity>, String> {
        use crate::schema::entities::dsl::*;
        entities
            .load::<Entity>(&mut self.db_connect)
            .map_err(|e| format!("Database query error: {}", e))
    }

    fn get_one(&mut self, entity_id: &str) -> Result<Option<Entity>, String> {
        use crate::schema::entities::dsl::{entities, id};
        match entities
            .filter(id.eq(entity_id))
            .first::<Entity>(&mut self.db_connect)
        {
            Ok(result) => Ok(Some(result)),
            Err(diesel::result::Error::NotFound) => Ok(None),
            Err(e) => Err(format!("Database query error: {}", e)),
        }
    }

    fn compute_id(source: &str, ticker_or_cik: &str, from: &str, to: &str) -> String {
        format!("{source}:{ticker_or_cik}:{from}..{to}")
    }

    fn compute_report_id(url_or_zip: &str) -> String {
        let mut h = Sha256::new();
        h.update(url_or_zip.as_bytes());
        let short = &h.finalize()[..8];
        format!("sec_edgar_report:{}", hex::encode(short))
    }
    /* =================== Fetch / Ensure (DB-first) =================== */

    fn ensure_entity(
        &mut self,
        ticker: &str,
        cik: &str,
        from: &str,
        to: &str,
    ) -> Result<Entity, String> {
        let id = Self::compute_id("sec_edgar", ticker, from, to);
        if let Some(e) = self.get_one(&id)? {
            return Ok(e);
        }

        let df = self.fetch_submissions_df(cik, from, to)?;
        let entity = self.persist_df_as_entity(ticker, cik, from, to, &df)?;
        Ok(entity)
    }

    fn ensure_report_entity(&mut self, url: &str) -> Result<Entity, String> {
        // NEW: quarterly-aware zip resolver (errors if not a 10-Q directory)
        let zip_url = self.resolve_quarterly_zip_url(url)?;
        let id = Self::compute_report_id(&zip_url);
        if let Some(e) = self.get_one(&id)? {
            return Ok(e);
        }
        let df = self.fetch_zip_and_build_tables_df(&zip_url)?;
        self.persist_report_df_as_entity(url, &zip_url, &df)
    }

    /// Resolve a filing directory (or primary doc URL) to the *quarterly* XBRL zip.
    /// Fails if the directory is not a 10-Q filing (e.g., 8-K).
    fn resolve_quarterly_zip_url(&self, url: &str) -> Result<String, String> {
        // If user passed a zip directly, enforce that it's a 10-Q zip
        if url.ends_with(".zip") {
            let name = url.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
            if is_quarterly_zip_name(&name) {
                return Ok(url.to_string());
            } else {
                return Err(format!(
                    "provided zip does not look like a 10-Q: `{name}` (expected name to contain 10q/10-q)"
                ));
            }
        }

        // Derive directory URL and fetch index.json
        let dir_url = dir_of_url(url);
        let index = self.read_filing_index_json(&dir_url)?;
        let (items, base) = parse_index_items(&index, &dir_url)?;

        // Check if this directory actually contains a 10-Q primary HTML
        let has_10q_html = items.iter().any(|it| is_quarterly_html_name(&it.name));
        let has_8k_html = items.iter().any(|it| is_8k_html_name(&it.name));

        if !has_10q_html {
            if has_8k_html {
                return Err(
                    "the given filing directory appears to be an 8-K, not a 10-Q".to_string(),
                );
            }
            return Err("no 10-Q primary document found in this filing directory".to_string());
        }

        // Prefer a zip that looks like the 10-Q xbrl package
        if let Some(zip) = prefer_quarterly_zip(&items) {
            return Ok(format!("{base}{}", zip.href));
        }

        // Fallback: any xbrl zip if 10-Q HTML exists
        if let Some(zip) = items
            .iter()
            .find(|it| it.name.to_ascii_lowercase().ends_with(".zip"))
        {
            return Ok(format!("{base}{}", zip.href));
        }

        Err("no .zip found in filing directory (expected an XBRL archive)".to_string())
    }

    fn _ensure_df(
        &mut self,
        ticker: &str,
        cik: &str,
        from: &str,
        to: &str,
    ) -> Result<DataFrame, String> {
        let id = Self::compute_id("sec_edgar", ticker, from, to);
        if let Some(e) = self.get_one(&id)? {
            return _df_from_entity_data(&e.data);
        }
        let df = self.fetch_submissions_df(cik, from, to)?;
        let _ = self.persist_df_as_entity(ticker, cik, from, to, &df)?;
        Ok(df)
    }

    /* =================== SEC requests & transforms =================== */

    /// Resolve a ticker to 10-digit zero-padded CIK using company_tickers.json
    fn resolve_ticker_to_cik(&self, ticker: &str) -> Result<String, String> {
        let url = "https://www.sec.gov/files/company_tickers.json";
        let resp = self
            .http
            .get(url)
            .send()
            .map_err(|e| format!("tickers.json request error: {e}"))?
            .error_for_status()
            .map_err(|e| format!("tickers.json http error: {e}"))?;

        // This file is a JSON object with numeric keys: {"0": {"ticker": "...", "cik_str": 1234, "title": "..."}, ...}
        let v: Value = resp
            .json()
            .map_err(|e| format!("tickers.json decode error: {e}"))?;

        let tk = ticker.to_ascii_uppercase();
        let mut match_cik: Option<String> = None;

        if let Some(obj) = v.as_object() {
            for (_k, rec) in obj {
                let rt = rec.get("ticker").and_then(|x| x.as_str()).unwrap_or("");
                if rt.eq_ignore_ascii_case(&tk) {
                    if let Some(cik_num) = rec.get("cik_str").and_then(|x| x.as_i64()) {
                        match_cik = Some(format!("{:010}", cik_num));
                        break;
                    }
                }
            }
        }
        match_cik.ok_or_else(|| format!("Unknown ticker `{}` in SEC tickers file", ticker))
    }

    /// Pull *quarterly* filings (10-Q and variants) for a CIK within [from, to].
    /// Accepts RFC3339 (YYYY-MM-DDTHH:MM:SSZ) or YYYY-MM-DD and normalizes to date-only.
    fn fetch_submissions_df(
        &self,
        cik_10: &str,
        from: &str,
        to: &str,
    ) -> Result<DataFrame, String> {
        let (from_d, to_d) = (parse_date_relaxed(from)?, parse_date_relaxed(to)?);
        let cik_no_leading = cik_10.trim_start_matches('0');

        // load root submissions
        let root_url = format!(
            "https://data.sec.gov/submissions/CIK{cik}.json",
            cik = cik_10
        );
        let root_v: Value = self
            .http
            .get(&root_url)
            .send()
            .map_err(|e| format!("submissions request error: {e}"))?
            .error_for_status()
            .map_err(|e| format!("submissions http error: {e}"))?
            .json()
            .map_err(|e| format!("submissions decode error: {e}"))?;

        // accumulators
        let mut acc_v: Vec<String> = Vec::new();
        let mut form_v: Vec<String> = Vec::new();
        let mut fdate_v: Vec<String> = Vec::new();
        let mut rdate_v: Vec<String> = Vec::new();
        let mut pdoc_v: Vec<String> = Vec::new();
        let mut url_v: Vec<String> = Vec::new();

        // recent page
        if let Some(recent) = root_v
            .get("filings")
            .and_then(|f| f.get("recent"))
            .and_then(|r| r.as_object())
        {
            collect_quarterlies_from_arrays_relaxed(
                recent,
                cik_no_leading,
                &from_d,
                &to_d,
                &mut acc_v,
                &mut form_v,
                &mut fdate_v,
                &mut rdate_v,
                &mut pdoc_v,
                &mut url_v,
            )?;
        }

        // yearly pages that overlap our date range
        if let Some(files) = root_v
            .get("filings")
            .and_then(|f| f.get("files"))
            .and_then(|x| x.as_array())
        {
            for f in files {
                let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let ffrom = f.get("filingFrom").and_then(|v| v.as_str()).unwrap_or("");
                let fto = f.get("filingTo").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() || ffrom.is_empty() || fto.is_empty() {
                    continue;
                }

                if let (Ok(ff), Ok(tt)) = (parse_date_relaxed(ffrom), parse_date_relaxed(fto)) {
                    if !ranges_overlap(ff, tt, from_d, to_d) {
                        continue;
                    }
                } else {
                    continue;
                }

                let page_url = format!("https://data.sec.gov/submissions/{name}");
                let page_v: Value = self
                    .http
                    .get(&page_url)
                    .send()
                    .map_err(|e| format!("submissions page `{name}` request error: {e}"))?
                    .error_for_status()
                    .map_err(|e| format!("submissions page `{name}` http error: {e}"))?
                    .json()
                    .map_err(|e| format!("submissions page `{name}` decode error: {e}"))?;

                if let Some(recent) = page_v
                    .get("filings")
                    .and_then(|f| f.get("recent"))
                    .and_then(|r| r.as_object())
                {
                    collect_quarterlies_from_arrays_relaxed(
                        recent,
                        cik_no_leading,
                        &from_d,
                        &to_d,
                        &mut acc_v,
                        &mut form_v,
                        &mut fdate_v,
                        &mut rdate_v,
                        &mut pdoc_v,
                        &mut url_v,
                    )?;
                }
            }
        }

        // dedup on accession_number
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        let mut keep = Vec::new();
        for (i, a) in acc_v.iter().enumerate() {
            if seen.insert(a.clone()) {
                keep.push(i);
            }
        }

        let take =
            |v: &Vec<String>| -> Vec<String> { keep.iter().map(|&i| v[i].clone()).collect() };

        // If nothing matched, return an *empty* DF with the expected schema
        if keep.is_empty() {
            return Ok(polars::prelude::df![
                "accession_number" => Vec::<String>::new(),
                "form"             => Vec::<String>::new(),
                "filing_date"      => Vec::<String>::new(),
                "report_date"      => Vec::<String>::new(),
                "primary_doc"      => Vec::<String>::new(),
                "url"              => Vec::<String>::new(),
            ]
            .map_err(|e| format!("polars df build error: {e}"))?);
        }

        let df = polars::prelude::df![
            "accession_number" => take(&acc_v),
            "form"             => take(&form_v),
            "filing_date"      => take(&fdate_v),
            "report_date"      => take(&rdate_v),
            "primary_doc"      => take(&pdoc_v),
            "url"              => take(&url_v)
        ]
        .map_err(|e| format!("polars df build error: {e}"))?;

        Ok(df)
    }

    fn persist_df_as_entity(
        &mut self,
        ticker: &str,
        cik_10: &str,
        from: &str,
        to: &str,
        df: &DataFrame,
    ) -> Result<Entity, String> {
        let data_value = df_to_json_records(df)?;
        let data_str =
            serde_json::to_string(&data_value).map_err(|e| format!("serialize data: {e}"))?;
        let etag = make_etag(&data_str);

        let tags_vec = vec![
            format!("ticker={}", ticker.to_ascii_uppercase()),
            format!("cik={}", cik_10),
            format!("from={}", from),
            format!("to={}", to),
        ];
        let tags_str =
            serde_json::to_string(&tags_vec).map_err(|e| format!("serialize tags: {e}"))?;

        let now = OffsetDateTime::now_utc();
        let fetched_at = now
            .format(&Rfc3339)
            .unwrap_or_else(|_| now.unix_timestamp().to_string());
        let updated_at = fetched_at.clone();
        let refresh_after = (now + Duration::days(1))
            .format(&Rfc3339)
            .unwrap_or_else(|_| (now + Duration::days(1)).unix_timestamp().to_string());

        let id = Self::compute_id("sec_edgar", ticker, from, to);
        let entity = Entity {
            id: Some(id),
            source: "sec_edgar".to_string(),
            tags: tags_str,
            data: data_str,
            etag,
            fetched_at,
            refresh_after,
            state: "ready".to_string(),
            last_error: "".to_string(),
            updated_at,
        };

        self.store_entity(&entity)?;
        Ok(entity)
    }

    fn fetch_report_df(
        &self,
        url: &str,
    ) -> Result<(DataFrame, String, String, usize, bool), String> {
        const MAX_BYTES: usize = 2_000_000; // 2 MB cap to avoid huge rows

        let resp = self
            .http
            .get(url)
            .send()
            .map_err(|e| format!("report GET error: {e}"))?
            .error_for_status()
            .map_err(|e| format!("report http error: {e}"))?;

        let ctype = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let bytes = resp.bytes().map_err(|e| format!("read body error: {e}"))?;
        let mut data = bytes.as_ref();
        let mut truncated = false;

        if data.len() > MAX_BYTES {
            data = &data[..MAX_BYTES];
            truncated = true;
        }

        // text-ish? → utf8; otherwise base64
        let is_text = ctype.starts_with("text/") || ctype.contains("xml") || ctype.contains("json");

        let (content, encoding) = if is_text {
            (
                String::from_utf8_lossy(data).into_owned(),
                "utf8".to_string(),
            )
        } else {
            (base64::encode(data), "base64".to_string())
        };

        let df = df![
            "url"           => vec![url.to_string()],
            "content"       => vec![content],
            "content_type"  => vec![ctype.clone()],
            "encoding"      => vec![encoding.clone()],
            "bytes"         => vec![bytes.len() as u64],
            "truncated"     => vec![truncated]
        ]
        .map_err(|e| format!("polars df build error: {e}"))?;

        Ok((df, ctype, encoding, bytes.len(), truncated))
    }

    fn resolve_report_zip_url(&self, url: &str) -> Result<String, String> {
        if url.ends_with(".zip") {
            return Ok(url.to_string());
        }

        // Fetch directory HTML and find a *.zip (prefer *-xbrl.zip)
        let html = self
            .http
            .get(url)
            .send()
            .map_err(|e| format!("GET dir error: {e}"))?
            .error_for_status()
            .map_err(|e| format!("dir http error: {e}"))?
            .text()
            .map_err(|e| format!("dir read text error: {e}"))?;

        // Search quickly (no full HTML parser): prefer "-xbrl.zip", else first ".zip"
        let mut found: Option<String> = None;
        for pat in ["-xbrl.zip", ".zip"] {
            if let Some(idx) = html.find(pat) {
                // backtrack to the preceding quote after href=
                if let Some(href_start) = html[..idx].rfind("href=") {
                    // the next char should be quote
                    let quote =
                        html.as_bytes().get(href_start + 5).copied().unwrap_or(b'"') as char;
                    let q = if quote == '\'' { '\'' } else { '"' };
                    // find opening quote position
                    let open = if html.as_bytes().get(href_start + 5) == Some(&(q as u8)) {
                        href_start + 6
                    } else {
                        href_start + 5
                    };
                    if let Some(close) = html[open..].find(q) {
                        let rel = &html[open..open + close];
                        found = Some(rel.to_string());
                        break;
                    }
                }
            }
        }

        let rel = found.ok_or_else(|| {
            "could not locate a .zip link on the EDGAR directory page".to_string()
        })?;
        Ok(join_href(url, &rel))
    }

    fn fetch_zip_and_build_tables_df(&self, zip_url: &str) -> Result<DataFrame, String> {
        use std::io::{Cursor, Read};
        const MAX_BYTES_PER_FILE: usize = 4_000_000; // 4MB cap per HTML file

        // Download zip
        let resp = self
            .http
            .get(zip_url)
            .send()
            .map_err(|e| format!("zip GET error: {e}"))?
            .error_for_status()
            .map_err(|e| format!("zip http error: {e}"))?;

        let body = resp.bytes().map_err(|e| format!("zip read error: {e}"))?;
        let mut zip =
            zip::ZipArchive::new(Cursor::new(body)).map_err(|e| format!("zip open error: {e}"))?;

        // Accumulators for the long-form table DF
        let mut files: Vec<String> = Vec::new();
        let mut table_idx: Vec<u32> = Vec::new();
        let mut table_id: Vec<String> = Vec::new();
        let mut caption: Vec<String> = Vec::new();
        let mut row_idx: Vec<u32> = Vec::new();
        let mut col_idx: Vec<u32> = Vec::new();
        let mut headers: Vec<String> = Vec::new();
        let mut values: Vec<String> = Vec::new();

        let mut next_table_index: u32 = 0;

        for i in 0..zip.len() {
            let mut f = zip
                .by_index(i)
                .map_err(|e| format!("zip entry error: {e}"))?;
            let name = f.name().to_string();

            // only parse 10-Q/10Q-like HTMLs
            if !Self::is_10q_like_html_name(&name) {
                continue;
            }

            // Read (with cap)
            let mut buf = Vec::with_capacity((f.size() as usize).min(MAX_BYTES_PER_FILE));
            f.take(MAX_BYTES_PER_FILE as u64)
                .read_to_end(&mut buf)
                .map_err(|e| format!("zip read entry error: {e}"))?;

            // Parse HTML and extract tables
            let html = String::from_utf8_lossy(&buf);
            scrape_tables_into_long_df(
                &name,
                &html,
                &mut next_table_index,
                &mut files,
                &mut table_idx,
                &mut table_id,
                &mut caption,
                &mut row_idx,
                &mut col_idx,
                &mut headers,
                &mut values,
            )?;
        }

        if files.is_empty() {
            return Err("no 10-Q HTML tables found in report".to_string());
        }

        let df = polars::prelude::df![
            "file"        => files,
            "table_index" => table_idx,
            "table_id"    => table_id,
            "caption"     => caption,
            "row_index"   => row_idx,
            "col_index"   => col_idx,
            "header"      => headers,
            "value"       => values
        ]
        .map_err(|e| format!("polars df build error: {e}"))?;

        Ok(df)
    }

    // helper: treat names like "...10q....htm(l)" as quarterly HTMLs
    fn is_10q_like_html_name(name: &str) -> bool {
        let n = name.to_ascii_lowercase();
        let is_html = n.ends_with(".htm") || n.ends_with(".html");
        let looks_10q = n.contains("10q") || n.contains("10-q");
        is_html && looks_10q
    }

    /* ---------- Persist rows as Entity ---------- */

    fn persist_report_df_as_entity(
        &mut self,
        original_url: &str,
        zip_url: &str,
        df: &polars::prelude::DataFrame,
    ) -> Result<Entity, String> {
        let data_value = df_to_json_records(df)?;
        let data_str =
            serde_json::to_string(&data_value).map_err(|e| format!("serialize data: {e}"))?;
        let etag = make_etag(&data_str);

        let now = OffsetDateTime::now_utc();
        let fetched_at = now
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| now.unix_timestamp().to_string());
        let refresh_after = (now + time::Duration::days(7))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| (now + time::Duration::days(7)).unix_timestamp().to_string());

        let id = Self::compute_report_id(zip_url);
        let tags_vec = vec![format!("url={original_url}"), format!("zip_url={zip_url}")];
        let tags_str =
            serde_json::to_string(&tags_vec).map_err(|e| format!("serialize tags: {e}"))?;

        let entity = Entity {
            id: Some(id),
            source: "sec_edgar".to_string(),
            tags: tags_str,
            data: data_str,
            etag,
            fetched_at: fetched_at.clone(),
            refresh_after,
            state: "ready".to_string(),
            last_error: "".to_string(),
            updated_at: fetched_at,
        };

        self.store_entity(&entity)?;
        Ok(entity)
    }
}

/* ===================== ProviderTrait ===================== */

impl super::ProviderTrait for SecEdgarProvider {
    fn fetch_entities(&mut self, entity: EntityInProvider) -> Result<Vec<Entity>, String> {
        match entity {
            EntityInProvider::GetEntity { id } => {
                if let Some(e) = self.get_one(&id)? {
                    return Ok(vec![e]);
                }
                // If not found, treat `id` as ticker over default 365d window
                let (from, to) = default_last_days_rfc3339(365);
                let cik = self.resolve_ticker_to_cik(&id)?;
                let e = self.ensure_entity(&id, &cik, &from, &to)?;
                Ok(vec![e])
            }

            EntityInProvider::GetEntities { ids } => {
                let mut out = Vec::new();
                for i in ids {
                    if let Some(e) = self.get_one(&i)? {
                        out.push(e);
                    }
                }
                if out.is_empty() {
                    Err("No requested entities found in DB".into())
                } else {
                    Ok(out)
                }
            }

            EntityInProvider::GetAllEntities { .. } => self.get_all(),

            EntityInProvider::SearchEntities { query, .. } => {
                let Params {
                    ticker,
                    from,
                    to,
                    cik,
                } = Params::from_filters(&self.http, &self.user_agent, query, |t| {
                    self.resolve_ticker_to_cik(t)
                })?;
                let e = self.ensure_entity(&ticker, &cik, &from, &to)?;
                Ok(vec![e])
            }
            EntityInProvider::GetReport { url } => {
                let e = self.ensure_report_entity(&url)?;
                Ok(vec![e])
            }
        }
    }
}

/* =================== tiny params helper =================== */

struct Params {
    ticker: String,
    cik: String,
    from: String,
    to: String,
}

impl Params {
    /// Extract ticker & date range from filters; default 365d if no range provided.
    /// `resolve` maps ticker→CIK (10 digits). Keeps a light rate-limit gap.
    fn from_filters<F>(
        _http: &Client,
        _ua: &str,
        filters: Vec<EntityFilter>,
        mut resolve: F,
    ) -> Result<Self, String>
    where
        F: FnMut(&str) -> Result<String, String>,
    {
        let mut ticker: Option<String> = None;
        let mut from: Option<String> = None;
        let mut to: Option<String> = None;
        let mut cik: Option<String> = None;

        for f in filters {
            match f {
                EntityFilter::Ticker(t) => ticker = Some(t),
                EntityFilter::DateRange { start, end } => {
                    from = Some(start);
                    to = Some(end);
                }
                EntityFilter::ByTags(tags) => {
                    for tag in tags {
                        if let Some((k, v)) = tag.split_once('=') {
                            match k {
                                "ticker" => ticker = Some(v.to_string()),
                                "from" => from = Some(v.to_string()),
                                "to" => to = Some(v.to_string()),
                                "cik" => cik = Some(v.to_string()),
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let ticker = ticker.ok_or("Missing ticker for SEC query")?;
        let cik = match cik {
            Some(c) => zero_pad_cik(&c),
            None => resolve(&ticker)?,
        };
        let (from, to) = match (from, to) {
            (Some(f), Some(t)) => (f, t),
            _ => default_last_days_rfc3339(365),
        };
        Ok(Self {
            ticker,
            cik,
            from,
            to,
        })
    }
}

/* =================== (De)serialize helpers reused =================== */

fn _df_from_entity_data(data_json: &str) -> Result<DataFrame, String> {
    use polars::prelude::{JsonFormat, JsonReader};
    let cursor = std::io::Cursor::new(data_json.as_bytes());
    JsonReader::new(cursor)
        .with_json_format(JsonFormat::Json) // array-of-objects (not NDJSON)
        .finish()
        .map_err(|e| format!("polars json read error: {e}"))
}

fn df_to_json_records(df: &DataFrame) -> Result<Value, String> {
    let names: Vec<String> = df
        .get_column_names_owned()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let height = df.height();

    let mut out = Vec::with_capacity(height);
    for row_idx in 0..height {
        let mut obj = serde_json::Map::with_capacity(names.len());
        for name in &names {
            // robust: &Column → owned → Series
            let column = df
                .column(name)
                .map_err(|e| format!("column `{}` access error: {}", name, e))?
                .to_owned();
            let col = column.as_series();

            // Series::get(row_idx) → AnyValue
            let av = col
                .ok_or_else(|| format!("column `{}` is None", name))?
                .get(row_idx)
                .map_err(|e| format!("row {} value error in `{}`: {}", row_idx, name, e))?;
            obj.insert(name.clone(), anyvalue_to_json(av)?);
        }
        out.push(Value::Object(obj));
    }
    Ok(Value::Array(out))
}

fn anyvalue_to_json(v: AnyValue<'_>) -> Result<Value, String> {
    Ok(match v {
        AnyValue::Null => Value::Null,
        AnyValue::Boolean(b) => Value::Bool(b),

        AnyValue::Int8(x) => json!(x),
        AnyValue::Int16(x) => json!(x),
        AnyValue::Int32(x) => json!(x),
        AnyValue::Int64(x) => json!(x),

        AnyValue::UInt8(x) => json!(x),
        AnyValue::UInt16(x) => json!(x),
        AnyValue::UInt32(x) => json!(x),
        AnyValue::UInt64(x) => json!(x),

        AnyValue::Float32(x) => json!(x),
        AnyValue::Float64(x) => json!(x),

        // Strings / datetimes / structs can vary by Polars version; fall back to Debug.
        _ => json!(format!("{:?}", v)),
    })
}

/* =================== misc utils =================== */

fn zero_pad_cik(s: &str) -> String {
    let t = s.trim_start_matches('0');
    let n = t.parse::<u64>().unwrap_or(0);
    format!("{:010}", n)
}

fn arr_str(v: Option<&Value>) -> Result<Vec<Option<String>>, String> {
    let arr = v
        .and_then(|x| x.as_array())
        .ok_or("expected array in submissions JSON")?;
    Ok(arr
        .iter()
        .map(|x| x.as_str().map(|s| s.to_string()))
        .collect())
}
fn parse_date(s: &str) -> Result<Date, String> {
    Date::parse(s, &time::format_description::well_known::Iso8601::DEFAULT)
        .map_err(|e| format!("invalid date `{s}` (YYYY-MM-DD expected): {e}"))
}

fn default_last_days_rfc3339(days: i64) -> (String, String) {
    let now = OffsetDateTime::now_utc();
    let from = now - Duration::days(days);
    (
        from.format(&Rfc3339)
            .unwrap_or_else(|_| from.unix_timestamp().to_string()),
        now.format(&Rfc3339)
            .unwrap_or_else(|_| now.unix_timestamp().to_string()),
    )
}

fn make_etag(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn is_text_ext(ext: &str) -> bool {
    matches!(
        ext,
        "htm" | "html" | "xml" | "xsd" | "json" | "js" | "css" | "txt"
    )
}

/// Join an EDGAR directory URL and an href (handles absolute/relative).
fn join_href(base_url: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    // EDGAR directory pages live on https://www.sec.gov/Archives/...
    // If href starts with '/', join with origin; else join with base directory.
    let origin = "https://www.sec.gov";
    if href.starts_with('/') {
        format!("{origin}{href}")
    } else {
        // trim to directory
        let base = if base_url.ends_with('/') {
            base_url.to_string()
        } else {
            match base_url.rfind('/') {
                Some(i) => base_url[..=i].to_string(),
                None => base_url.to_string(),
            }
        };
        format!("{base}{href}")
    }
}

/// Relaxed collector that tolerates missing arrays and blanks.
/// Pushes only rows that are 10-Q-ish and within the date range.
/// Converts missing strings to "" and always pushes Strings (no Option columns).
fn collect_quarterlies_from_arrays_relaxed(
    recent_obj: &serde_json::Map<String, Value>,
    cik_no_leading: &str,
    from_d: &time::Date,
    to_d: &time::Date,
    out_acc: &mut Vec<String>,
    out_form: &mut Vec<String>,
    out_fdate: &mut Vec<String>,
    out_rdate: &mut Vec<String>,
    out_pdoc: &mut Vec<String>,
    out_url: &mut Vec<String>,
) -> Result<(), String> {
    let acc         = arr_opt_str(recent_obj.get("accessionNumber"));
    let form        = arr_opt_str(recent_obj.get("form"));
    let filing_date = arr_opt_str(recent_obj.get("filingDate"));
    // SEC differs: reportDate may be absent or empty
    let report_date = arr_opt_str(recent_obj.get("reportDate"));
    // Some pages use primaryDoc, others primaryDocument
    let primary_doc = {
        let pd  = arr_opt_str(recent_obj.get("primaryDoc"));
        let pdd = arr_opt_str(recent_obj.get("primaryDocument"));
        if !pd.is_empty() { pd } else { pdd }
    };

    let n = acc.len()
        .min(form.len())
        .min(filing_date.len());

    for i in 0..n {
        let f = form[i].as_deref().unwrap_or("");
        if !is_quarterly_form(f) { continue; }

        let fdate_str = filing_date[i].as_deref().unwrap_or("");
        if fdate_str.is_empty() { continue; }
        let fdate = match parse_date_relaxed(fdate_str) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if fdate < *from_d || fdate > *to_d { continue; }

        let a  = acc[i].as_deref().unwrap_or("");
        if a.is_empty() { continue; }

        let rd = report_date.get(i).and_then(|x| x.as_deref()).unwrap_or("");
        let pd = primary_doc.get(i).and_then(|x| x.as_deref()).unwrap_or("");

        // Build URL to primary doc (if present) else filing directory
        let acc_clean: String = a.chars().filter(|c| *c != '-').collect();
        let url = if pd.is_empty() {
            format!("https://www.sec.gov/Archives/edgar/data/{cik}/{acc}",
                cik = cik_no_leading, acc = acc_clean)
        } else {
            format!("https://www.sec.gov/Archives/edgar/data/{cik}/{acc}/{pd}",
                cik = cik_no_leading, acc = acc_clean, pd = pd)
        };

        out_acc.push(a.to_string());
        out_form.push(f.to_string());
        out_fdate.push(fdate_str[..10.min(fdate_str.len())].to_string()); // date-only
        out_rdate.push(rd.to_string());
        out_pdoc.push(pd.to_string());
        out_url.push(url);
    }
    Ok(())
}


/// Accepts "YYYY-MM-DD" or RFC3339, returns Date.
fn parse_date_relaxed(s: &str) -> Result<time::Date, String> {
    let t = s.trim();
    // If RFC3339, just take the date part
    let date_part = if t.len() >= 10 { &t[..10] } else { t };
    time::Date::parse(date_part, &time::format_description::well_known::Iso8601::DEFAULT)
        .map_err(|e| format!("invalid date `{s}`: {e}"))
}

/// Return Vec<Option<String>> for possibly-missing arrays; missing entries become None.
fn arr_opt_str(v: Option<&Value>) -> Vec<Option<String>> {
    v.and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn scrape_tables_into_long_df(
    file_name: &str,
    html: &str,
    next_table_index: &mut u32,
    files: &mut Vec<String>,
    table_idx: &mut Vec<u32>,
    table_id: &mut Vec<String>,
    captions: &mut Vec<String>,
    row_idx: &mut Vec<u32>,
    col_idx: &mut Vec<u32>,
    headers: &mut Vec<String>,
    values: &mut Vec<String>,
) -> Result<(), String> {
    let doc = Html::parse_document(html);

    let sel_table = Selector::parse("table").map_err(|_| "selector error")?;
    let sel_tr = Selector::parse("tr").map_err(|_| "selector error")?;
    let sel_th = Selector::parse("th").map_err(|_| "selector error")?;
    let sel_td = Selector::parse("td").map_err(|_| "selector error")?;
    let sel_caption = Selector::parse("caption").map_err(|_| "selector error")?;

    for table in doc.select(&sel_table) {
        let this_index = *next_table_index;
        *next_table_index += 1;

        // table id / caption (optional)
        let id_attr = table.value().attr("id").unwrap_or("").to_string();
        let cap_text = table
            .select(&sel_caption)
            .next()
            .map(|c| norm_text(c.text().collect::<String>().as_str()))
            .unwrap_or_default();

        // Try to get header cells (first row with any <th>)
        let mut header_row: Option<Vec<String>> = None;
        for tr in table.select(&sel_tr) {
            let hs: Vec<String> = tr
                .select(&sel_th)
                .map(|th| norm_text(&th.text().collect::<String>()))
                .filter(|s| !s.is_empty())
                .collect();
            if !hs.is_empty() {
                header_row = Some(hs);
                break;
            }
        }

        // Walk rows; use header_row (if present) to label columns
        let mut r = 0u32;
        for tr in table.select(&sel_tr) {
            // skip header-only row from data (optional)
            let has_th = tr.select(&sel_th).next().is_some();
            let mut cells: Vec<String> = tr
                .select(&sel_td)
                .map(|td| norm_text(&td.text().collect::<String>()))
                .collect();

            // Some tables put data in <th> cells too; include them as values when no <td>
            if cells.is_empty() && has_th {
                cells = tr
                    .select(&sel_th)
                    .map(|th| norm_text(&th.text().collect::<String>()))
                    .collect();
            }
            if cells.is_empty() {
                continue;
            }

            for (c, val) in cells.into_iter().enumerate() {
                let h = header_row
                    .as_ref()
                    .and_then(|hdrs| hdrs.get(c).cloned())
                    .unwrap_or_default();

                files.push(file_name.to_string());
                table_idx.push(this_index);
                table_id.push(id_attr.clone());
                captions.push(cap_text.clone());
                row_idx.push(r);
                col_idx.push(c as u32);
                headers.push(h);
                values.push(val);
            }
            r += 1;
        }
    }
    Ok(())
}

/// Collapse whitespace & trim
fn norm_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

fn is_quarterly_form(form: &str) -> bool {
    // Normalize and match 10-Q variants
    let f = form.trim().to_ascii_uppercase();
    matches!(f.as_str(), "10-Q" | "10-Q/A" | "10-QT" | "10-QT/A")
}

fn ranges_overlap(a_from: Date, a_to: Date, b_from: Date, b_to: Date) -> bool {
    !(a_to < b_from || b_to < a_from)
}

fn collect_quarterlies_from_arrays(
    recent_obj: &serde_json::Map<String, Value>,
    cik_no_leading: &str,
    from_d: &Date,
    to_d: &Date,
    out_acc: &mut Vec<String>,
    out_form: &mut Vec<String>,
    out_fdate: &mut Vec<String>,
    out_rdate: &mut Vec<String>,
    out_pdoc: &mut Vec<String>,
    out_url: &mut Vec<String>,
) -> Result<(), String> {
    // Arrays present on both root and yearly pages
    let acc = arr_str(recent_obj.get("accessionNumber"))?;
    let form = arr_str(recent_obj.get("form"))?;
    let filing_date = arr_str(recent_obj.get("filingDate"))?;
    // SEC sometimes uses "reportDate" as string array; may be empty
    let report_date = arr_opt_str(recent_obj.get("reportDate"));
    // Key can be `primaryDoc` *or* `primaryDocument` depending on page
    let primary_doc = {
        let pd = arr_opt_str(recent_obj.get("primaryDoc"));
        let pdd = arr_opt_str(recent_obj.get("primaryDocument"));
        if !pd.is_empty() { pd } else { pdd }
    };

    let n = acc.len().min(form.len()).min(filing_date.len());
    for i in 0..n {
        let f = form[i].as_deref().unwrap_or("");
        if !is_quarterly_form(f) {
            continue;
        }

        let fdate_str = filing_date[i].as_deref().unwrap_or("");
        let fdate = match Date::parse(
            fdate_str,
            &time::format_description::well_known::Iso8601::DEFAULT,
        ) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if fdate < *from_d || fdate > *to_d {
            continue;
        }

        let a = acc[i].as_deref().unwrap_or("");
        let rd = report_date.get(i).and_then(|x| x.as_deref()).unwrap_or("");
        let pd = primary_doc.get(i).and_then(|x| x.as_deref()).unwrap_or("");

        // Build URL to primary doc, fallback to directory
        let acc_clean: String = a.chars().filter(|c| *c != '-').collect();
        let url = if pd.is_empty() {
            format!(
                "https://www.sec.gov/Archives/edgar/data/{cik}/{acc}",
                cik = cik_no_leading,
                acc = acc_clean
            )
        } else {
            format!(
                "https://www.sec.gov/Archives/edgar/data/{cik}/{acc}/{pd}",
                cik = cik_no_leading,
                acc = acc_clean,
                pd = pd
            )
        };

        out_acc.push(a.to_string());
        out_form.push(f.to_string());
        out_fdate.push(fdate_str.to_string());
        out_rdate.push(rd.to_string());
        out_pdoc.push(pd.to_string());
        out_url.push(url);
    }
    Ok(())
}

#[derive(Debug)]
struct IndexItem {
    name: String,
    href: String,
}

fn dir_of_url(url: &str) -> String {
    if url.ends_with('/') {
        return url.to_string();
    }
    match url.rfind('/') {
        Some(i) => url[..=i].to_string(),
        None => url.to_string(),
    }
}

fn parse_index_items(
    index_json: &serde_json::Value,
    dir_url: &str,
) -> Result<(Vec<IndexItem>, String), String> {
    // index.json shape: { "directory": { "item": [ { "name": "...", "href": "...", ...}, ... ] } }
    let base = if dir_url.ends_with('/') {
        dir_url.to_string()
    } else {
        format!("{dir_url}/")
    };
    let items = index_json
        .get("directory")
        .and_then(|d| d.get("item"))
        .and_then(|a| a.as_array())
        .ok_or("unexpected index.json shape (missing directory.item)")?;

    let out = items
        .iter()
        .filter_map(|it| {
            let name = it.get("name")?.as_str()?.to_string();
            let href = it.get("href")?.as_str()?.to_string();
            Some(IndexItem { name, href })
        })
        .collect::<Vec<_>>();

    Ok((out, base))
}

fn is_quarterly_html_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    (n.ends_with(".htm") || n.ends_with(".html")) && (n.contains("10q") || n.contains("10-q"))
}

fn is_8k_html_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    (n.ends_with(".htm") || n.ends_with(".html")) && (n.contains("8k") || n.contains("8-k"))
}

fn is_quarterly_zip_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".zip") && (n.contains("10q") || n.contains("10-q"))
}

/// Prefer `*-10q-*.zip` / `*10q*.zip`, else `*-xbrl.zip` when a 10-Q HTML is present.
fn prefer_quarterly_zip(items: &[IndexItem]) -> Option<&IndexItem> {
    // 1) strong match: zip with 10q in name
    if let Some(it) = items.iter().find(|it| is_quarterly_zip_name(&it.name)) {
        return Some(it);
    }
    // 2) common case: "*-xbrl.zip"
    items.iter().find(|it| {
        let n = it.name.to_ascii_lowercase();
        n.ends_with(".zip") && n.contains("xbrl")
    })
}

impl SecEdgarProvider {
    fn read_filing_index_json(&self, dir_url: &str) -> Result<serde_json::Value, String> {
        // Ensure trailing slash, then append index.json
        let base = if dir_url.ends_with('/') {
            dir_url.to_string()
        } else {
            format!("{dir_url}/")
        };
        let url = format!("{base}index.json");
        let v: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .map_err(|e| format!("index.json GET error: {e}"))?
            .error_for_status()
            .map_err(|e| format!("index.json http error: {e}"))?
            .json()
            .map_err(|e| format!("index.json decode error: {e}"))?;
        Ok(v)
    }
}
