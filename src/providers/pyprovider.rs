/*
SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza
*/

use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;
use diesel::{RunQueryDsl, insert_into};
use pyo3::types::PyModule;
use pyo3::{Py, Python, types::PyAnyMethods};

use super::ProviderTrait;
use crate::models::Entity;
use crate::query::{EntityFilter, EntityInProvider};
use crate::schema::entities;
use crate::schema::entities::dsl as E;

// Safety: we confine all Python interaction within GIL sections.
// The struct only holds a GIL-independent `Py<PyAny>` handle, which is Send+Sync-safe with these bounds.
pub struct PyProviderAdapter {
    pub py_obj: Py<pyo3::PyAny>,
    pub name: String,
    pub db_connect: Pool<ConnectionManager<SqliteConnection>>,
}

// Allow sharing across threads (pool is thread-safe; py_obj is only used under GIL)
unsafe impl Send for PyProviderAdapter {}
unsafe impl Sync for PyProviderAdapter {}

impl PyProviderAdapter {
    /// Create from DB path (uses your pooled `establish_connection`)
    pub fn new(db_path: &str, module: &str, class_name: &str) -> Result<Self, String> {
        let pool = crate::establish_connection(db_path); // <- must return Pool<ConnectionManager<_>>
        Self::load_with_pool(pool, module, class_name)
    }

    /// Create from an existing Diesel pool
    pub fn load_with_pool(
        pool: Pool<ConnectionManager<SqliteConnection>>,
        module: &str,
        class_name: &str,
    ) -> Result<Self, String> {
        Python::with_gil(|py| {
            let m = PyModule::import_bound(py, module)
                .map_err(|e| format!("python import error: {e:?}"))?;
            let cls = m
                .getattr(class_name)
                .map_err(|e| format!("python getattr class error: {e:?}"))?;

            let obj = cls
                .call0()
                .map_err(|e| format!("python class() ctor error: {e:?}"))?;

            let name_py = obj
                .call_method0("name")
                .map_err(|e| format!("python name() error: {e:?}"))?;
            let name: String = name_py
                .extract()
                .map_err(|e| format!("python name() extract str error: {e:?}"))?;

            Ok(Self {
                py_obj: obj.unbind(),
                name,
                db_connect: pool,
            })
        })
    }

    #[inline]
    fn conn(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, String> {
        self.db_connect
            .get()
            .map_err(|e| format!("db pool get error: {e}"))
    }

    /* ===================== DB helpers ===================== */

    /// DB: try one entity by id (primary key)
    fn db_get_one(&self, entity_id: &str) -> Result<Option<Entity>, String> {
        let mut conn = self.conn()?;
        match E::entities
            .filter(E::id.eq(entity_id))
            .first::<Entity>(&mut conn)
        {
            Ok(e) => Ok(Some(e)),
            Err(diesel::result::Error::NotFound) => Ok(None),
            Err(e) => Err(format!("db get_one error: {e}")),
        }
    }

    /// DB: upsert single entity by id
    fn db_upsert_one(&self, entity: &Entity) -> Result<(), String> {
        let mut conn = self.conn()?;
        insert_into(entities::table)
            .values(entity)
            .on_conflict(entities::id)
            .do_update()
            .set((
                E::source.eq(&entity.source),
                E::tags.eq(&entity.tags),
                E::data.eq(&entity.data),
                E::etag.eq(&entity.etag),
                E::fetched_at.eq(&entity.fetched_at),
                E::refresh_after.eq(&entity.refresh_after),
                E::state.eq(&entity.state),
                E::last_error.eq(&entity.last_error),
                E::updated_at.eq(&entity.updated_at),
            ))
            .execute(&mut conn)
            .map_err(|e| format!("db upsert error: {e}"))?;
        Ok(())
    }

    fn db_upsert_many(&self, xs: &[Entity]) -> Result<(), String> {
        for e in xs {
            self.db_upsert_one(e)?;
        }
        Ok(())
    }

    /* ===================== Python bridges ===================== */

    /// Call Python `fetch_entities(request)` and decode JSON -> Vec<Entity>
    fn py_fetch(&self, request: &EntityInProvider) -> Result<Vec<Entity>, String> {
        Python::with_gil(|py| {
            let obj = self.py_obj.bind(py);
            let json_mod = PyModule::import_bound(py, "json")
                .map_err(|e| format!("import json failed: {e:?}"))?;

            let payload_json = serde_json::to_string(request)
                .map_err(|e| format!("serde to_string error: {e}"))?;
            let payload_py = json_mod
                .call_method1("loads", (payload_json,))
                .map_err(|e| format!("json.loads failed: {e:?}"))?;

            let result_py = obj
                .call_method1("fetch_entities", (payload_py,))
                .map_err(|e| format!("python fetch_entities failed: {e:?}"))?;

            let result_json_py = json_mod
                .call_method1("dumps", (result_py,))
                .map_err(|e| format!("json.dumps on result failed: {e:?}"))?;

            let result_json: String = result_json_py
                .extract()
                .map_err(|e| format!("extract str from dumps failed: {e:?}"))?;

            serde_json::from_str::<Vec<Entity>>(&result_json)
                .map_err(|e| format!("serde from_str<Vec<Entity>> failed: {e}"))
        })
    }

    /// Call Python `stitch(filters)` -> Entity
    fn py_stitch(&self, filters: &[EntityFilter]) -> Result<Entity, String> {
        Python::with_gil(|py| {
            let obj = self.py_obj.bind(py);

            if !obj
                .hasattr("stitch")
                .map_err(|e| format!("hasattr('stitch') failed: {e:?}"))?
            {
                return Err("stitch not supported by this python provider".to_string());
            }

            let json_mod = PyModule::import_bound(py, "json")
                .map_err(|e| format!("import json failed: {e:?}"))?;
            let payload_json = serde_json::to_string(filters)
                .map_err(|e| format!("serde to_string error: {e}"))?;
            let payload_py = json_mod
                .call_method1("loads", (payload_json,))
                .map_err(|e| format!("json.loads failed: {e:?}"))?;

            let result_py = obj
                .call_method1("stitch", (payload_py,))
                .map_err(|e| format!("python stitch failed: {e:?}"))?;

            let result_json_py = json_mod
                .call_method1("dumps", (result_py,))
                .map_err(|e| format!("json.dumps on result failed: {e:?}"))?;

            let result_json: String = result_json_py
                .extract()
                .map_err(|e| format!("extract str from dumps failed: {e:?}"))?;

            serde_json::from_str::<Entity>(&result_json)
                .map_err(|e| format!("serde from_str<Entity> failed: {e}"))
        })
    }

    /// Core loader (assumes sys.path is already set).
    pub fn inner_load(module: &str, class_name: &str, db_path: &str) -> Result<Self, String> {
        let pool = crate::establish_connection(db_path); // <- must return Pool<ConnectionManager<_>>
        Python::with_gil(|py| {
            let m = PyModule::import_bound(py, module)
                .map_err(|e| format!("python import error: {e:?}"))?;
            let cls = m
                .getattr(class_name)
                .map_err(|e| format!("python getattr class error: {e:?}"))?;

            let obj = cls
                .call0()
                .map_err(|e| format!("python class() ctor error: {e:?}"))?;

            let name_py = obj
                .call_method0("name")
                .map_err(|e| format!("python name() error: {e:?}"))?;
            let name: String = name_py
                .extract()
                .map_err(|e| format!("python name() extract str error: {e:?}"))?;

            Ok(Self {
                py_obj: obj.unbind(), // GIL-independent handle for cross-thread storage
                name,
                db_connect: pool,
            })
        })
    }
}

/* ===================== ProviderTrait impl ===================== */

impl ProviderTrait for PyProviderAdapter {
    /// DB-first + write-through:
    /// - GetEntity: try DB by id; on miss call Python, upsert, then return the requested id if present (else all).
    /// - GetEntities: gather DB hits; if any missing, call Python once, upsert, then merge.
    /// - GetAllEntities/SearchEntities: pass-through to Python, upsert everything.
    fn fetch_entities(&mut self, req: EntityInProvider) -> Result<Vec<Entity>, String> {
        match &req {
            EntityInProvider::GetEntity { id } => {
                if let Some(e) = self.db_get_one(id)? {
                    return Ok(vec![e]);
                }
                let mut fetched = self.py_fetch(&req)?;
                if !fetched.is_empty() {
                    self.db_upsert_many(&fetched)?;
                }
                if let Some(e) = fetched
                    .iter()
                    .find(|e| e.id.as_deref() == Some(id.as_str()))
                {
                    return Ok(vec![e.clone()]);
                }
                Ok(fetched)
            }

            EntityInProvider::GetEntities { ids } => {
                let mut have: Vec<Entity> = Vec::new();
                let mut missing: Vec<String> = Vec::new();

                for id in ids {
                    match self.db_get_one(id)? {
                        Some(e) => have.push(e),
                        None => missing.push(id.clone()),
                    }
                }

                if missing.is_empty() {
                    return Ok(have);
                }

                let fetched = self.py_fetch(&req)?;
                if !fetched.is_empty() {
                    self.db_upsert_many(&fetched)?;
                }

                let mut by_id = std::collections::HashMap::<String, Entity>::new();
                for e in fetched.into_iter() {
                    if let Some(k) = e.id.clone() {
                        by_id.insert(k, e);
                    }
                }
                for id in missing {
                    if let Some(e) = by_id.remove(&id) {
                        have.push(e);
                    }
                }
                Ok(have)
            }

            EntityInProvider::GetAllEntities { .. } | EntityInProvider::SearchEntities { .. } => {
                let fetched = self.py_fetch(&req)?;
                if !fetched.is_empty() {
                    self.db_upsert_many(&fetched)?;
                }
                Ok(fetched)
            }

            EntityInProvider::GetReport { .. } => {
                Err("GetReport not supported by PyProviderAdapter".to_string())
            }
        }
    }

    fn stitch(&mut self, filters: Vec<EntityFilter>) -> Result<Entity, String> {
        let stitched = self.py_stitch(&filters)?;
        // Optional: persist the stitched result too
        // let _ = self.db_upsert_one(&stitched);
        Ok(stitched)
    }
}
