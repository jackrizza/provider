// src/providers/yahoo_finance.rs

use crate::models::Entity;
use crate::query::{EntityFilter, EntityInProvider};
use crate::schema::entities::dsl::{
    data as col_data, entities as entities_table, etag as col_etag, fetched_at as col_fetched_at,
    id as col_id, last_error as col_last_error, refresh_after as col_refresh_after,
    source as col_source, state as col_state, tags as col_tags, updated_at as col_updated_at,
};
use crate::{establish_connection, schema::entities};
use diesel::prelude::*;
use diesel::{QueryDsl, RunQueryDsl, SqliteConnection};
use polars::prelude::*;
use serde_json::{Value, json};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use time::{Duration, OffsetDateTime};
use yahoo_finance_api as yahoo;

use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};

pub struct YahooFinanceProvider {
    db_connect: Pool<ConnectionManager<SqliteConnection>>,
}

impl YahooFinanceProvider {
    pub fn new(db_path: &str) -> Self {
        YahooFinanceProvider {
            db_connect: establish_connection(db_path),
        }
    }
    #[inline]
    fn conn(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, String> {
        self.db_connect
            .get()
            .map_err(|e| format!("db pool get error: {e}"))
    }

    /* ========================= DB helpers ========================= */
    // Replace your current store_entities_in_db with this UP SERT version:
    pub fn store_entities_in_db(&mut self, entity: Entity) -> Result<(), String> {
        use crate::schema::entities::dsl::*;
        let _ = entity.id.as_ref().ok_or("Entity.id is None".to_string())?;
        let mut conn = self.conn()?;

        diesel::insert_into(crate::schema::entities::table)
            .values(&entity)
            .on_conflict(id)
            .do_update()
            .set((
                source.eq(&entity.source),
                tags.eq(&entity.tags),
                data.eq(&entity.data),
                etag.eq(&entity.etag),
                fetched_at.eq(&entity.fetched_at),
                refresh_after.eq(&entity.refresh_after),
                state.eq(&entity.state),
                last_error.eq(&entity.last_error),
                updated_at.eq(&entity.updated_at),
            ))
            .execute(&mut *conn)
            .map_err(|e| format!("Database upsert error: {e}"))?;
        Ok(())
    }

    pub fn get_all_entities_from_db(&mut self) -> Result<Vec<Entity>, String> {
        use crate::schema::entities::dsl::*;
        let mut conn = self.conn()?;
        entities
            .load::<Entity>(&mut *conn)
            .map_err(|e| format!("Database query error: {e}"))
    }

    pub fn get_one_entity_from_db(&mut self, entity_id: &str) -> Result<Option<Entity>, String> {
        use crate::schema::entities::dsl::{entities, id};
        let mut conn = self.conn()?;
        match entities
            .filter(id.eq(entity_id))
            .first::<Entity>(&mut *conn)
        {
            Ok(result) => Ok(Some(result)),
            Err(diesel::result::Error::NotFound) => Ok(None),
            Err(e) => Err(format!("Database query error: {}", e)),
        }
    }

    fn compute_id(source: &str, ticker: &str, from: &str, to: &str) -> String {
        format!("{source}:{ticker}:{from}..{to}")
    }

    /// DB-first: ensure an Entity for (ticker,from,to) exists in DB. Returns it.
    fn ensure_entity_in_db(
        &mut self,
        source: &str,
        ticker: &str,
        from: &str,
        to: &str,
    ) -> Result<Entity, String> {
        let id = YahooFinanceProvider::compute_id(source, ticker, from, to);
        if let Some(e) = self.get_one_entity_from_db(&id)? {
            return Ok(e);
        }

        // Not in DB: fetch, persist, return
        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("Tokio runtime error: {e}"))?;
        let pull = YahooFinanceExternalData {
            ticker: ticker.to_string(),
            from: from.to_string(),
            to: to.to_string(),
        };
        let (_t, _f, _t2, df) = rt.block_on(pull.pull_data())?;
        let entity = persist_yahoo_df_as_entity("yahoo_finance", ticker, from, to, &df, self)?;
        Ok(entity)
    }

    /// DB-first for DataFrame path: returns a DataFrame from DB if present, else fetches & persists.
    #[allow(dead_code)]
    fn _ensure_df_for(
        &mut self,
        source: &str,
        ticker: &str,
        from: &str,
        to: &str,
    ) -> Result<DataFrame, String> {
        let id = YahooFinanceProvider::compute_id(source, ticker, from, to);
        if let Some(e) = self.get_one_entity_from_db(&id)? {
            return _df_from_entity_data(&e.data)
                .map_err(|e| format!("Failed to build DataFrame from cached entity: {e}"));
        }

        // Not in DB: fetch, persist, return df
        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("Tokio runtime error: {e}"))?;
        let pull = YahooFinanceExternalData {
            ticker: ticker.to_string(),
            from: from.to_string(),
            to: to.to_string(),
        };
        let (_t, _f, _t2, df) = rt.block_on(pull.pull_data())?;
        let _ = persist_yahoo_df_as_entity("yahoo_finance", ticker, from, to, &df, self)?;
        Ok(df)
    }

    /* ========================= Stitch helpers ========================= */

    /// Get all entities for a given source+ticker (coarse LIKE on tags; refine in Rust).
    fn db_entities_for_ticker(
        &mut self,
        source_name: &str,
        ticker: &str,
    ) -> Result<Vec<Entity>, String> {
        use crate::schema::entities::dsl as E;
        let like = format!("%\"ticker={}\"%", ticker);
        let mut conn = self.conn()?;
        E::entities
            .filter(E::source.eq(source_name))
            .filter(E::tags.like(like))
            .load::<Entity>(&mut *conn)
            .map_err(|e| format!("DB query error: {e}"))
    }
}

/* =================== ProviderTrait impl =================== */

impl super::ProviderTrait for YahooFinanceProvider {
    /// DB-first semantics everywhere to avoid duplicate inserts.
    fn fetch_entities(&mut self, entity: EntityInProvider) -> Result<Vec<Entity>, String> {
        match entity {
            EntityInProvider::GetEntity { id } => {
                // Treat `id` as an entity id first.
                if let Some(e) = self.get_one_entity_from_db(&id)? {
                    return Ok(vec![e]);
                }
                // If not in DB, interpret as ticker with default 30d window.
                let (from, to) = default_last_days_rfc3339(30);
                let e = self.ensure_entity_in_db("yahoo_finance", &id, &from, &to)?;
                Ok(vec![e])
            }

            EntityInProvider::GetEntities { ids } => {
                // DB-first per id; skip missing (or add ensure if desired)
                let mut out = Vec::new();
                for i in ids {
                    if let Some(e) = self.get_one_entity_from_db(&i)? {
                        out.push(e);
                    }
                }
                if out.is_empty() {
                    Err("No requested entities found in DB".to_string())
                } else {
                    Ok(out)
                }
            }

            EntityInProvider::GetAllEntities { .. } => self.get_all_entities_from_db(),

            EntityInProvider::SearchEntities { query, .. } => {
                // If filters include both Ticker and DateRange, use stitch (gap-aware).
                let have_ticker = query.iter().any(|f| matches!(f, EntityFilter::Ticker(_)));
                let have_range = query
                    .iter()
                    .any(|f| matches!(f, EntityFilter::DateRange { .. }));
                if have_ticker && have_range {
                    let stitched = self.stitch(query)?;
                    Ok(vec![stitched])
                } else {
                    // Fall back: ensure whole window (default 30d if absent)
                    let params = YahooFinanceExternalData::from_filters(query)?;
                    let e = self.ensure_entity_in_db(
                        "yahoo_finance",
                        &params.ticker,
                        &params.from,
                        &params.to,
                    )?;
                    Ok(vec![e])
                }
            }

            _ => Err("Unsupported operation for YahooFinanceProvider".to_string()),
        }
    }

    /// Gap-aware fetch that stitches cached + newly-fetched slices and persists a super-entity.
    fn stitch(&mut self, filters: Vec<EntityFilter>) -> Result<Entity, String> {
        let params = YahooFinanceExternalData::from_filters(filters)?;
        let source_name = "yahoo_finance";

        let want_from = parse_rfc3339(&params.from)?;
        let want_to = parse_rfc3339(&params.to)?;
        if want_from >= want_to {
            return Err("invalid range: from >= to".into());
        }

        // 1) pull cached slices for ticker
        let cached = self.db_entities_for_ticker(source_name, &params.ticker)?;

        // 2) keep overlapping slices and build coverage + frames
        let mut have_intervals: Vec<I> = Vec::new();
        let mut frames_for_overlap: Vec<DataFrame> = Vec::new();

        for e in cached.iter() {
            let t = match parse_entity_tags(e) {
                Ok(t) => t,
                Err(_) => continue,
            };
            if t.ticker != params.ticker {
                continue;
            }
            let ef = parse_rfc3339(&t.from)?;
            let et = parse_rfc3339(&t.to)?;
            if let Some((s, e_)) = clamp_overlap(ef, et, want_from, want_to) {
                have_intervals.push(to_i(s, e_));
                let df = _df_from_entity_data(&e.data)?;
                frames_for_overlap.push(df);
            }
        }

        // 3) compute gaps and fetch only the missing sub-ranges
        let want = to_i(want_from, want_to);
        let gaps = missing(want, &have_intervals);

        if !gaps.is_empty() {
            let rt =
                tokio::runtime::Runtime::new().map_err(|e| format!("Tokio runtime error: {e}"))?;
            for I(s, e) in gaps {
                let (gs, ge) = from_i(I(s, e));
                let pull = YahooFinanceExternalData {
                    ticker: params.ticker.clone(),
                    from: gs
                        .format(&time::format_description::well_known::Rfc3339)
                        .unwrap(),
                    to: ge
                        .format(&time::format_description::well_known::Rfc3339)
                        .unwrap(),
                };

                // Capture copies BEFORE moving `pull` into `pull_data`
                let gap_from = pull.from.clone();
                let gap_to = pull.to.clone();

                let (_t, _f, _t2, df_gap) = rt.block_on(pull.pull_data())?;

                // Persist using the captured copies
                let entity_gap = persist_yahoo_df_as_entity(
                    source_name,
                    &params.ticker,
                    &gap_from,
                    &gap_to,
                    &df_gap,
                    self,
                )?;

                let df2 = _df_from_entity_data(&entity_gap.data)?;
                frames_for_overlap.push(df2);
            }
        }

        if frames_for_overlap.is_empty() {
            return Err("no data available or retrieved for requested window".into());
        }

        // 4) concatenate, de-dup, and clamp to exact [from..to)
        let stitched_df = concat_and_trim(frames_for_overlap, want_from, want_to)?;

        // 5) persist the stitched super-entity (optional but useful for future single-hit)
        let stitched_entity = persist_yahoo_df_as_entity(
            source_name,
            &params.ticker,
            &params.from,
            &params.to,
            &stitched_df,
            self,
        )?;

        Ok(stitched_entity)
    }
}

/* =================== Yahoo pull & parameterization =================== */

pub struct YahooFinanceExternalData {
    pub ticker: String,
    pub from: String, // RFC3339
    pub to: String,   // RFC3339
}

impl YahooFinanceExternalData {
    /// Build from filters; requires Ticker and optional DateRange.
    /// If DateRange absent, uses default last 30 days.
    pub fn from_filters(filters: Vec<EntityFilter>) -> Result<Self, String> {
        let mut ticker: Option<String> = None;
        let mut from: Option<String> = None;
        let mut to: Option<String> = None;

        for f in filters {
            match f {
                EntityFilter::Ticker(t) => ticker = Some(t),
                EntityFilter::DateRange { start, end } => {
                    from = Some(start);
                    to = Some(end);
                }
                // ignore others for Yahoo fetch
                _ => {}
            }
        }

        let ticker = ticker.ok_or("Missing ticker")?;
        let (from, to) = match (from, to) {
            (Some(f), Some(t)) => (f, t),
            _ => default_last_days_rfc3339(30),
        };

        Ok(Self { ticker, from, to })
    }

    /// Pull OHLCV history into a Polars DataFrame and return identifiers too.
    async fn pull_data(self) -> Result<(String, String, String, DataFrame), String> {
        let connector = yahoo::YahooConnector::new()
            .map_err(|e| format!("Failed to create Yahoo connector: {e}"))?;

        let tkr = self.ticker.clone();
        let from_s = self.from.clone();
        let to_s = self.to.clone();

        let from =
            OffsetDateTime::parse(&self.from, &time::format_description::well_known::Rfc3339)
                .map_err(|e| format!("Invalid 'from' date: {}", e))?;
        let to = OffsetDateTime::parse(&self.to, &time::format_description::well_known::Rfc3339)
            .map_err(|e| format!("Invalid 'to' date: {}", e))?;

        let response = connector
            .get_quote_history(&tkr, from, to)
            .await
            .map_err(|e| format!("API error: {}", e))?;

        let quotes = response
            .quotes()
            .map_err(|e| format!("Failed to parse quotes: {}", e))?;

        if quotes.is_empty() {
            return Err("No data received".into());
        }

        // columns
        let timestamps: Vec<_> = quotes.iter().map(|q| q.timestamp).collect();
        let closes: Vec<_> = quotes.iter().map(|q| q.close).collect();
        let opens: Vec<_> = quotes.iter().map(|q| q.open).collect();
        let highs: Vec<_> = quotes.iter().map(|q| q.high).collect();
        let lows: Vec<_> = quotes.iter().map(|q| q.low).collect();
        let volumes: Vec<u64> = quotes.iter().map(|q| q.volume as u64).collect();

        // DataFrame
        let df = df![
            "timestamp" => timestamps,
            "open"      => opens,
            "high"      => highs,
            "low"       => lows,
            "close"     => closes,
            "volume"    => volumes
        ]
        .map_err(|e| format!("Failed to create DataFrame: {}", e))?;

        Ok((tkr, from_s, to_s, df))
    }
}

/* =================== Persisting & (de)serializing =================== */

/// Persist the fetched frame as an Entity with:
/// - source: "yahoo_finance"
/// - tags: JSON string of ["ticker=..","from=..","to=.."]
/// - data: JSON string of records
/// - etag: stable hash of `data`
/// - fetched_at / updated_at: now (RFC3339)
/// - refresh_after: now + 1 day
/// - state: "ready", last_error: ""
/// - id: "yahoo_finance:{ticker}:{from}..{to}"
fn persist_yahoo_df_as_entity(
    source_name: &str,
    ticker: &str,
    from_rfc3339: &str,
    to_rfc3339: &str,
    df: &DataFrame,
    this: &mut YahooFinanceProvider,
) -> Result<Entity, String> {
    let records_value = df_to_json_records(df)?; // Value::Array([...])
    let data_str =
        serde_json::to_string(&records_value).map_err(|e| format!("serialize data: {e}"))?;

    let tags_vec = vec![
        format!("ticker={ticker}"),
        format!("from={from}", from = from_rfc3339),
        format!("to={to}", to = to_rfc3339),
    ];
    let tags_str = serde_json::to_string(&tags_vec).map_err(|e| format!("serialize tags: {e}"))?;

    let now = OffsetDateTime::now_utc();
    let fetched_at = now_rfc3339_from(now);
    let updated_at = fetched_at.clone();
    let refresh_after = now_rfc3339_from(now + Duration::days(1));
    let etag = make_etag(&data_str);

    let entity = Entity {
        id: Some(YahooFinanceProvider::compute_id(
            source_name,
            ticker,
            from_rfc3339,
            to_rfc3339,
        )),
        source: source_name.to_string(),
        tags: tags_str,
        data: data_str,
        etag,
        fetched_at,
        refresh_after,
        state: "ready".to_string(),
        last_error: "".to_string(),
        updated_at,
    };

    this.store_entities_in_db(entity.clone())?;
    Ok(entity)
}

/// Convert DataFrame → JSON "records" (Vec<Object>)
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
            let col = df
                .column(name)
                .map_err(|e| format!("column `{}` access error: {}", name, e))?;
            let s: &Series = col
                .as_series()
                .ok_or_else(|| format!("Failed to convert column `{}` to Series", name))?;
            let av = s
                .get(row_idx)
                .map_err(|e| format!("row {} value error in `{}`: {}", row_idx, name, e))?;
            let j = anyvalue_to_json(av)?;
            obj.insert(name.clone(), j);
        }
        out.push(Value::Object(obj));
    }
    Ok(Value::Array(out))
}

/// Parse `Entity.data` JSON array into a Polars DataFrame.
fn _df_from_entity_data(data_json: &str) -> Result<DataFrame, String> {
    let cursor = Cursor::new(data_json.as_bytes());
    JsonReader::new(cursor)
        .with_json_format(JsonFormat::Json) // array-of-objects
        .finish()
        .map_err(|e| format!("polars json read error: {e}"))
}

/// Convert a single AnyValue to JSON. Handle common primitives; fallback to Debug.
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

        _ => json!(format!("{:?}", v)),
    })
}

/* =================== time + utils =================== */

fn default_last_days_rfc3339(days: i64) -> (String, String) {
    let now = OffsetDateTime::now_utc();
    let from = now - Duration::days(days);
    (now_rfc3339_from(from), now_rfc3339_from(now))
}

fn now_rfc3339_from(t: OffsetDateTime) -> String {
    t.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| t.unix_timestamp().to_string())
}

fn make_etag(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

/* =================== tag parsing =================== */

#[derive(Debug, Clone)]
struct TagTriple {
    ticker: String,
    from: String, // RFC3339
    to: String,   // RFC3339
}

fn parse_entity_tags(e: &Entity) -> Result<TagTriple, String> {
    let v: Vec<String> =
        serde_json::from_str(&e.tags).map_err(|er| format!("parse tags json: {er}"))?;
    let mut t = TagTriple {
        ticker: String::new(),
        from: String::new(),
        to: String::new(),
    };
    for s in v {
        if let Some((k, v)) = s.split_once('=') {
            match k {
                "ticker" => t.ticker = v.to_string(),
                "from" => t.from = v.to_string(),
                "to" => t.to = v.to_string(),
                _ => {}
            }
        }
    }
    if t.ticker.is_empty() || t.from.is_empty() || t.to.is_empty() {
        return Err("tags missing one of ticker/from/to".into());
    }
    Ok(t)
}

/* =================== interval math =================== */

use time::format_description::well_known::Rfc3339;

#[inline]
fn parse_rfc3339(s: &str) -> Result<OffsetDateTime, String> {
    OffsetDateTime::parse(s, &Rfc3339).map_err(|e| format!("rfc3339 parse: {e}"))
}

#[inline]
fn clamp_overlap(
    a0: OffsetDateTime,
    a1: OffsetDateTime,
    b0: OffsetDateTime,
    b1: OffsetDateTime,
) -> Option<(OffsetDateTime, OffsetDateTime)> {
    let start = a0.max(b0);
    let end = a1.min(b1);
    if start < end {
        Some((start, end))
    } else {
        None
    }
}

// Half-open interval in epoch seconds [start, end)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct I(i64, i64);

fn to_i(a: OffsetDateTime, b: OffsetDateTime) -> I {
    I(a.unix_timestamp(), b.unix_timestamp())
}

fn from_i(i: I) -> (OffsetDateTime, OffsetDateTime) {
    (
        OffsetDateTime::from_unix_timestamp(i.0).unwrap(),
        OffsetDateTime::from_unix_timestamp(i.1).unwrap(),
    )
}

/// Merge/normalize coverage intervals (assumes half-open)
fn merge(mut xs: Vec<I>) -> Vec<I> {
    xs.sort_unstable();
    let mut out: Vec<I> = Vec::new();
    for I(s, e) in xs {
        if let Some(last) = out.last_mut() {
            if s <= last.1 {
                last.1 = last.1.max(e);
            } else {
                out.push(I(s, e));
            }
        } else {
            out.push(I(s, e));
        }
    }
    out
}

/// Compute missing sub-intervals: want \ coverage
fn missing(want: I, have: &[I]) -> Vec<I> {
    let mut cur = want.0;
    let mut out = Vec::new();
    for I(s0, e0) in merge(have.to_vec()) {
        if e0 <= want.0 || s0 >= want.1 {
            continue;
        }
        let s = s0.max(want.0);
        let e = e0.min(want.1);
        if cur < s {
            out.push(I(cur, s));
        }
        cur = cur.max(e);
        if cur >= want.1 {
            break;
        }
    }
    if cur < want.1 {
        out.push(I(cur, want.1));
    }
    out
}

/* =================== concat & trim =================== */

fn concat_and_trim(
    frames: Vec<DataFrame>,
    want_from: OffsetDateTime,
    want_to: OffsetDateTime,
) -> Result<DataFrame, String> {
    if frames.is_empty() {
        return Err("no frames to stitch".into());
    }

    // Manual vertical concat without requiring the `diagonal_concat` feature.
    let mut iter = frames.into_iter();
    let mut df = iter
        .next()
        .ok_or_else(|| "no frames to stitch".to_string())?;

    for f in iter {
        // This requires compatible schemas. If schemas might differ, align columns first.
        df.vstack_mut(&f)
            .map_err(|e| format!("concat/vstack error: {e}"))?;
    }

    // Sort, drop duplicates, clamp to [want_from, want_to)
    let out = df
        .lazy()
        .sort(["timestamp"], Default::default())
        .unique(None, UniqueKeepStrategy::First)
        .filter(
            col("timestamp")
                .gt_eq(lit(want_from.unix_timestamp()))
                .and(col("timestamp").lt(lit(want_to.unix_timestamp()))),
        )
        .collect()
        .map_err(|e| format!("trim/sort unique collect: {e}"))?;

    Ok(out)
}

fn align_columns(mut dfs: Vec<DataFrame>) -> Result<Vec<DataFrame>, String> {
    use std::collections::BTreeSet;
    let mut all: BTreeSet<String> = BTreeSet::new();
    for df in &dfs {
        for name in df.get_column_names() {
            all.insert(name.to_string());
        }
    }
    let all: Vec<String> = all.into_iter().collect();

    for df in dfs.iter_mut() {
        for name in &all {
            if df.column(name).is_err() {
                // fill missing column with Nulls
                let s = Series::new_null(name.into(), df.height());
                df.with_column(s)
                    .map_err(|e| format!("add col {name}: {e}"))?;
            }
        }
        // reorder to match the union schema
        *df = df
            .select(&all)
            .map_err(|e| format!("reorder select: {e}"))?;
    }
    Ok(dfs)
}
