use pyo3::prelude::*;
use pyo3::types::{PyList, PyModule, PyString};

/// Thin adapter that wraps a Python provider object and implements ProviderTrait.
pub struct PyProviderAdapter {
    pub py_obj: Py<PyAny>,
    pub name: String,
}

impl PyProviderAdapter {
    /// Add `<project_base_dir>/provider` to `sys.path` (if not already present).
    pub fn add_provider_dir_to_syspath(project_base_dir: &str) -> Result<(), String> {
        let provider_dir = format!("{}/providers", project_base_dir);

        Python::with_gil(|py| {
            let sys = PyModule::import_bound(py, "sys")
                .map_err(|e| format!("import sys failed: {e:?}"))?;

            // sys.path as a PyList (Bound API)
            let path_any = sys
                .getattr("path")
                .map_err(|e| format!("getattr sys.path failed: {e:?}"))?;
            let path_list: Bound<'_, PyList> = path_any
                .downcast_into()
                .map_err(|e| format!("sys.path downcast_to PyList failed: {e:?}"))?;

            // Is provider_dir already on sys.path?
            let mut exists = false;
            for item in path_list.iter() {
                let s = item
                    .str()
                    .map_err(|e| format!("path entry str() failed: {e:?}"))?;
                if s.to_string_lossy() == provider_dir {
                    exists = true;
                    break;
                }
            }

            if !exists {
                let py_dir: Bound<'_, PyString> = PyString::new_bound(py, &provider_dir);
                // call insert(0, provider_dir)
                path_list
                    .call_method1("insert", (0usize, py_dir))
                    .map_err(|e| format!("sys.path.insert failed: {e:?}"))?;
            }
            Ok(())
        })
    }

    /// Core loader (assumes sys.path is already set).
    pub fn inner_load(module: &str, class_name: &str) -> Result<Self, String> {
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
            })
        })
    }

    /// Public helper: ensure `<project_base_dir>/provider` is on sys.path, then import.
    pub fn load_from_project_dir(
        project_base_dir: &str,
        module: &str,
        class_name: &str,
    ) -> Result<Self, String> {
        Self::add_provider_dir_to_syspath(project_base_dir)?;
        Self::inner_load(module, class_name)
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub fn add_dirs_to_syspath(project_base_dir: &str) -> Result<(), String> {
    let candidates = [
        project_base_dir.to_string(),
        format!("{}/provider", project_base_dir),
        format!("{}/providers", project_base_dir),
    ];

    Python::with_gil(|py| {
        let sys = PyModule::import_bound(py, "sys").map_err(|e| format!("import sys: {e:?}"))?;
        let path_any = sys
            .getattr("path")
            .map_err(|e| format!("sys.path: {e:?}"))?;
        let path_list: Bound<'_, PyList> = path_any
            .downcast_into()
            .map_err(|e| format!("downcast sys.path -> list: {e:?}"))?;

        for dir in candidates {
            if !std::fs::metadata(&dir).map(|m| m.is_dir()).unwrap_or(false) {
                continue;
            }
            let mut exists = false;
            for item in path_list.iter() {
                let s = item.str().map_err(|e| format!("str(): {e:?}"))?;
                if s.to_string_lossy() == dir {
                    exists = true;
                    break;
                }
            }
            if !exists {
                let py_dir = PyString::new_bound(py, &dir);
                path_list
                    .call_method1("insert", (0usize, py_dir))
                    .map_err(|e| format!("sys.path.insert: {e:?}"))?;
            }
        }
        Ok(())
    })
}
