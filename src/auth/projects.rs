/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{Auth, NewProject, NewProjectProvider, NewProjectUser, Project, ProjectUser}; // your auth model
use crate::schema::{project_providers, projects};

pub type DbPool = diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<SqliteConnection>>;

#[derive(Clone)]
pub struct ProjectService {
    pool: DbPool,
}

impl ProjectService {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    fn conn(
        &self,
    ) -> Result<
        diesel::r2d2::PooledConnection<diesel::r2d2::ConnectionManager<SqliteConnection>>,
        String,
    > {
        // if your pool.get() returns a PooledConnection, change this accordingly
        self.pool.get().map_err(|e| format!("pool.get(): {e}"))
    }

    /// List projects the user owns (simple version)
    pub fn list_projects_for_user(&self, user: &Auth) -> Result<Vec<Project>, String> {
        use crate::schema::projects::dsl as p;

        let mut conn = self.conn()?;
        let uid = user.id.as_deref().ok_or("user has no id")?;
        let out = p::projects
            .filter(p::owner_id.eq(uid))
            .order(p::created_at.desc())
            .load::<Project>(&mut conn)
            .map_err(|e| format!("db load projects: {e}"))?;
        Ok(out)
    }

    /// Create project and attach providers
    pub fn create_project_with_providers(
        &self,
        project_id: &str,
        name: &str,
        description: &str,
        owner_id: &str,
        providers: &[String],
    ) -> Result<(), String> {
        use diesel::insert_into;

        let mut conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();

        let new = NewProject {
            id: project_id,
            name,
            description,
            owner_id,
            visibility: "private",
            created_at: &now,
            updated_at: &now,
        };

        conn.transaction::<(), diesel::result::Error, _>(|conn| {
            insert_into(projects::table).values(&new).execute(conn)?;

            if !providers.is_empty() {
                let rows: Vec<NewProjectProvider> = providers
                    .iter()
                    .map(|pname| NewProjectProvider {
                        project_id,
                        provider_name: pname.as_str(),
                    })
                    .collect();
                insert_into(project_providers::table)
                    .values(&rows)
                    .execute(conn)?;
            }

            Ok(())
        })
        .map_err(|e| format!("create project tx failed: {e}"))?;

        Ok(())
    }
    /// NEW: get one project by id
    pub fn get_project(&self, project_id: &str) -> Result<Option<Project>, String> {
        use crate::schema::projects::dsl as p;
        let mut conn = self.conn()?;
        let proj = p::projects
            .filter(p::id.eq(project_id))
            .first::<Project>(&mut conn)
            .optional()
            .map_err(|e| format!("db get project: {e}"))?;
        Ok(proj)
    }

    /// NEW: list project members
    pub fn list_project_users(&self, project_id: &str) -> Result<Vec<ProjectUser>, String> {
        use crate::schema::project_users::dsl as pu;
        let mut conn = self.conn()?;
        let rows = pu::project_users
            .filter(pu::project_id.eq(project_id))
            .order(pu::user_id.asc())
            .load::<ProjectUser>(&mut conn)
            .map_err(|e| format!("db list project users: {e}"))?;
        Ok(rows)
    }

    /// NEW: add a user to a project
    pub fn add_user_to_project(
        &self,
        project_id: &str,
        user_id: &str,
        role: &str,
    ) -> Result<(), String> {
        use diesel::insert_into;
        let mut conn = self.conn()?;
        let row = NewProjectUser {
            project_id,
            user_id,
            role,
        };
        insert_into(crate::schema::project_users::table)
            .values(&row)
            .execute(&mut conn)
            .map_err(|e| format!("db insert project user: {e}"))?;
        Ok(())
    }
}
