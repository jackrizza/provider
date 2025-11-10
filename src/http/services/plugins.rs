use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use serde::{Deserialize, Serialize};

use crate::models::Auth;
use crate::models::{
    Blob, NewBlob, NewPlugin, NewPluginFileContent, NewPluginNode, Plugin, PluginNode,
    UpdatePlugin, UpdatePluginNode,
}; // ← note the semicolon

use crate::schema::*;
use chrono::Utc;

// Bring the table DSL modules into scope:
use super::logs::LogService;
use crate::schema::plugin_file_content::dsl as pfc;
use crate::schema::plugin_nodes::dsl as pn;
use crate::schema::plugins::dsl as pl;
use std::sync::{Arc, Mutex};

pub type DbPool = diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<SqliteConnection>>;

#[derive(Clone)]
pub struct PluginService {
    log_service: Arc<Mutex<LogService>>,
    pool: DbPool,
}

impl PluginService {
    pub fn new(pool: DbPool, log_service: Arc<Mutex<LogService>>) -> Self {
        Self { log_service, pool }
    }

    fn conn(
        &self,
    ) -> Result<
        diesel::r2d2::PooledConnection<diesel::r2d2::ConnectionManager<SqliteConnection>>,
        String,
    > {
        self.pool.get().map_err(|e| format!("pool.get(): {e}"))
    }

    pub fn list_plugins_for_owner(&self, owner_id: &str) -> Result<Vec<Plugin>, String> {
        use crate::schema::plugins::dsl as pl;
        let mut conn = self.conn()?;
        let rows = pl::plugins
            .filter(pl::owner_id.eq(owner_id))
            .select(Plugin::as_select()) // requires Selectable derive on Plugin
            .order(pl::created_at.desc())
            .load::<Plugin>(&mut conn)
            .map_err(|e| format!("db list plugins: {e}"))?;
        Ok(rows)
    }

    /// Authorization helper: ensure the caller owns the plugin
    fn assert_owner(
        &self,
        conn: &mut SqliteConnection,
        plugin_id: &str,
        user: &Auth,
    ) -> Result<(), String> {
        let uid = user.id.as_deref().ok_or("user has no id")?;
        let owner: Option<String> = pl::plugins
            .filter(pl::id.eq(plugin_id))
            .select(pl::owner_id)
            .first(conn)
            .optional()
            .map_err(|e| format!("db read plugin owner: {e}"))?;
        match owner {
            Some(o) if o == uid => Ok(()),
            Some(_) => Err("forbidden: not plugin owner".into()),
            None => Err("plugin not found".into()),
        }
    }

    /// Create a plugin row. Does not create any files yet.
    pub fn new_plugin(
        &self,
        auth: &Auth,
        project_id: &str,
        name: &str,
        entry_path: &str,
        runtime: &str,   // "python"
        plugin_id: &str, // you decide: Uuid or slug
    ) -> Result<Plugin, String> {
        use diesel::insert_into;
        let mut conn = self.conn()?;
        let uid = auth.id.as_deref().ok_or("user has no id")?;
        let now = Utc::now().to_rfc3339();

        let new = NewPlugin {
            id: plugin_id,
            project_id,
            owner_id: uid,
            name,
            entry_path,
            runtime,
            created_at: &now,
            updated_at: &now,
        };

        insert_into(crate::schema::plugins::table)
            .values(&new)
            .execute(&mut conn)
            .map_err(|e| format!("insert plugin: {e}"))?;

        // Return the created row
        let row = pl::plugins
            .filter(pl::id.eq(plugin_id))
            .select(Plugin::as_select())
            .first::<Plugin>(&mut conn)
            .map_err(|e| format!("load plugin after insert: {e}"))?;
        Ok(row)
    }

    /// Update plugin metadata (name, entry_path, runtime)
    pub fn update_plugin(
        &self,
        auth: &Auth,
        plugin_id: &str,
        name: Option<&str>,
        entry_path: Option<&str>,
        runtime: Option<&str>,
    ) -> Result<Plugin, String> {
        use diesel::update;
        let mut conn = self.conn()?;
        self.assert_owner(&mut conn, plugin_id, auth)?;

        let now = Utc::now().to_rfc3339();
        let changes = UpdatePlugin {
            name,
            entry_path,
            runtime,
            updated_at: Some(&now),
        };

        update(pl::plugins.filter(pl::id.eq(plugin_id)))
            .set(&changes)
            .execute(&mut conn)
            .map_err(|e| format!("update plugin: {e}"))?;

        let row = pl::plugins
            .filter(pl::id.eq(plugin_id))
            .first::<Plugin>(&mut conn)
            .map_err(|e| format!("load plugin after update: {e}"))?;
        Ok(row)
    }

    /// Delete plugin and its tree in a single transaction
    pub fn delete_plugin(&self, auth: &Auth, plugin_id: &str) -> Result<(), String> {
        use diesel::{delete, result::Error as DbErr};
        let mut conn = self.conn()?;
        self.assert_owner(&mut conn, plugin_id, auth)?;

        conn.transaction::<(), DbErr, _>(|conn| {
            // Delete file content rows for nodes in this plugin
            let node_ids: Vec<i32> = pn::plugin_nodes
                .filter(pn::plugin_id.eq(plugin_id))
                .select(pn::id.assume_not_null()) // <-- key line
                .load(conn)?;

            if !node_ids.is_empty() {
                delete(pfc::plugin_file_content.filter(pfc::node_id.eq_any(&node_ids)))
                    .execute(conn)?;
                // Delete path cache if you store it separately
                diesel::delete(
                    crate::schema::plugin_path_cache::dsl::plugin_path_cache
                        .filter(crate::schema::plugin_path_cache::dsl::node_id.eq_any(&node_ids)),
                )
                .execute(conn)?;
                // Delete nodes (children first via ON DELETE CASCADE or do manual order)
                delete(pn::plugin_nodes.filter(pn::plugin_id.eq(plugin_id))).execute(conn)?;
            }

            // Finally delete plugin row
            delete(pl::plugins.filter(pl::id.eq(plugin_id))).execute(conn)?;

            Ok(())
        })
        .map_err(|e| format!("tx delete plugin: {e}"))?;

        Ok(())
    }

    /// Return the folder/file layout as a nested tree (by plugin_id)
    pub fn get_folder_file_layout(
        &self,
        auth: &Auth,
        plugin_id: &str,
    ) -> Result<Vec<TreeNode>, String> {
        let mut conn = self.conn()?;
        self.assert_owner(&mut conn, plugin_id, auth)?;

        let mut rows = pn::plugin_nodes
            .filter(pn::plugin_id.eq(plugin_id))
            .order((pn::parent_id.asc(), pn::kind.desc(), pn::name.asc()))
            .load::<PluginNode>(&mut conn)
            .map_err(|e| format!("list plugin nodes: {e}"))?;

        // Build adjacency lists
        use std::collections::HashMap;
        let mut by_parent: HashMap<Option<i32>, Vec<PluginNode>> = HashMap::new();
        for n in rows.drain(..) {
            by_parent.entry(n.parent_id).or_default().push(n);
        }

        fn build(
            parent: Option<i32>,
            map: &mut HashMap<Option<i32>, Vec<PluginNode>>,
        ) -> Vec<TreeNode> {
            let mut v = Vec::new();
            if let Some(mut children) = map.remove(&parent) {
                // dirs first (kind = "dir"), then files
                children.sort_by(|a, b| {
                    (b.kind.clone(), a.name.clone()).cmp(&(a.kind.clone(), b.name.clone()))
                });
                for n in children {
                    let kids = if n.kind == "dir" {
                        build(n.id, map)
                    } else {
                        vec![]
                    };
                    v.push(TreeNode {
                        id: n.id.unwrap(),
                        name: n.name,
                        kind: n.kind,
                        children: kids,
                    });
                }
            }
            v
        }

        Ok(build(None, &mut by_parent))
    }
}

/// Structure returned by get_folder_file_layout
#[derive(Debug, Clone, Serialize)]
pub struct TreeNode {
    pub id: i32,
    pub name: String,
    pub kind: String, // "dir" | "file"
    pub children: Vec<TreeNode>,
}
