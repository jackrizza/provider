/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryEnvelope<T = QueryEnvelopePayload> {
    pub request_id: String,
    pub return_address: Option<String>,
    pub v: Option<u16>,
    pub auth: Option<String>,
    pub query: T,
    pub project_id: Option<String>,
    pub ts_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum QueryEnvelopePayload {
    ProviderRequest {
        provider: String,
        request: EntityInProvider,
    },
    ProviderList,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntityInProvider {
    GetEntity {
        id: String,
    },
    SearchEntities {
        query: Vec<EntityFilter>,
        limit: Option<u32>,
    },
    GetReport {
        url: String,
    },
    GetEntities {
        ids: Vec<String>,
    },
    GetAllEntities {
        limit: Option<u32>,
        offset: Option<u32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntityFilter {
    ById(String),
    BySource(String),
    ByState(String),
    ByTags(Vec<String>),
    Ticker(String),
    DateRange { start: String, end: String },
    ByUpdatedAtRange { start: String, end: String },
    ByUrl(String),
}
