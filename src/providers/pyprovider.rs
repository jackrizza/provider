/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use pyo3::Python;
use pyo3::types::{PyAnyMethods, PyModule};

use super::ProviderTrait;
use crate::pyadapter::PyProviderAdapter;
use crate::query::EntityInProvider;

// Safety: we confine all Python interaction within GIL sections.
// The struct only holds a GIL-independent `Py<PyAny>` handle.
unsafe impl Send for PyProviderAdapter {}
unsafe impl Sync for PyProviderAdapter {}

impl PyProviderAdapter {
    pub fn load(module: &str, class_name: &str) -> Result<Self, String> {
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
            })
        })
    }
}

impl ProviderTrait for PyProviderAdapter {
    fn fetch_entities(
        &mut self,
        entity: EntityInProvider,
    ) -> Result<Vec<crate::models::Entity>, String> {
        Python::with_gil(|py| {
            let obj = self.py_obj.bind(py);
            let json_mod = PyModule::import_bound(py, "json")
                .map_err(|e| format!("import json failed: {e:?}"))?;
            let payload_json = serde_json::to_string(&entity)
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

            serde_json::from_str::<Vec<crate::models::Entity>>(&result_json)
                .map_err(|e| format!("serde from_str<Vec<Entity>> failed: {e}"))
        })
    }

    // NEW: implement stitch to satisfy the trait
    fn stitch(
        &mut self,
        filters: Vec<crate::query::EntityFilter>,
    ) -> Result<crate::models::Entity, String> {
        Python::with_gil(|py| {
            let obj = self.py_obj.bind(py);
            // Try to call Python `stitch` if it exists; otherwise return an error
            if !obj
                .hasattr("stitch")
                .map_err(|e| format!("hasattr('stitch') failed: {e:?}"))?
            {
                return Err("stitch not supported by this python provider".to_string());
            }
            let json_mod = PyModule::import_bound(py, "json")
                .map_err(|e| format!("import json failed: {e:?}"))?;
            let payload_json = serde_json::to_string(&filters)
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
            serde_json::from_str::<crate::models::Entity>(&result_json)
                .map_err(|e| format!("serde from_str<Entity> failed: {e}"))
        })
    }
}
