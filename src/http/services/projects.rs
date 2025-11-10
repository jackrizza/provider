/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::logs::{LogService, NewLogCategory, NewLogLevel, NewLogStripped, NewLogSubcategory};
use crate::models::{Auth, NewProject, NewProjectProvider, NewProjectUser, Project, ProjectUser}; // your auth model
use crate::schema::{project_providers, projects};
use std::sync::{Arc, Mutex};
pub type DbPool = diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<SqliteConnection>>;

#[derive(Clone)]
pub struct ProjectService {
    log_service: Arc<Mutex<LogService>>,
    pool: DbPool,
}

impl ProjectService {
    pub fn new(pool: DbPool, log_service: Arc<Mutex<LogService>>) -> Self {
        Self { log_service, pool }
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
    pub fn list_projects_for_user(
        &self,
        user: &Auth,
    ) -> Result<(Vec<Project>, Vec<Project>), String> {
        use crate::schema::project_users::dsl as pu;
        use crate::schema::projects::dsl as p;

        let mut conn = self.conn()?;
        let uid = user.id.as_deref().ok_or("user has no id")?;
        let users_projects = p::projects
            .filter(p::owner_id.eq(uid))
            .order(p::created_at.desc())
            .load::<Project>(&mut conn)
            .map_err(|e| format!("db load projects: {e}"))?;

        let shared_project_ids = pu::project_users
            .filter(pu::user_id.eq(uid))
            .load::<ProjectUser>(&mut conn)
            .map_err(|e| format!("db load projects: {e}"))?;
        let shared_projects = p::projects
            .filter(p::id.eq_any(shared_project_ids.into_iter().map(|pu| pu.project_id)))
            .load::<Project>(&mut conn)
            .map_err(|e| format!("db load projects: {e}"))?;

        self.log_service.lock().unwrap().new_log(
            &user,
            NewLogStripped {
                category: NewLogCategory::Project,
                subcategory: NewLogSubcategory::Request,
                message: "Project listed".to_string(),
                level: NewLogLevel::Info,
                timestamp: chrono::Utc::now().naive_utc(),
            },
        );

        Ok((users_projects, shared_projects))
    }

    /// Create project and attach providers
    pub fn create_project_with_providers(
        &self,
        user: &Auth,
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

        self.log_service.lock().unwrap().new_log(
            &user,
            NewLogStripped {
                category: NewLogCategory::Project,
                subcategory: NewLogSubcategory::Add,
                message: format!("Project {} created", project_id),
                level: NewLogLevel::Info,
                timestamp: chrono::Utc::now().naive_utc(),
            },
        );
        Ok(())
    }
    /// NEW: get one project by id
    pub fn get_project(&self, user: &Auth, project_id: &str) -> Result<Option<Project>, String> {
        use crate::schema::projects::dsl as p;
        let mut conn = self.conn()?;
        let proj = p::projects
            .filter(p::owner_id.eq(user.id.clone().unwrap_or("".into())))
            .filter(p::id.eq(project_id))
            .first::<Project>(&mut conn)
            .optional()
            .map_err(|e| format!("db get project: {e}"))?;
        self.log_service.lock().unwrap().new_log(
            &user,
            NewLogStripped {
                category: NewLogCategory::Project,
                subcategory: NewLogSubcategory::Request,
                message: format!("Project {} retrieved", project_id),
                level: NewLogLevel::Info,
                timestamp: chrono::Utc::now().naive_utc(),
            },
        );
        Ok(proj)
    }

    /// NEW: list project members
    pub fn list_project_users(
        &self,
        user: &Auth,
        project_id: &str,
    ) -> Result<Vec<ProjectUser>, String> {
        use crate::schema::project_users::dsl as pu;
        let mut conn = self.conn()?;
        let rows = pu::project_users
            .filter(pu::project_id.eq(project_id))
            .order(pu::user_id.asc())
            .load::<ProjectUser>(&mut conn)
            .map_err(|e| format!("db list project users: {e}"))?;

        self.log_service.lock().unwrap().new_log(
            &user,
            NewLogStripped {
                category: NewLogCategory::Project,
                subcategory: NewLogSubcategory::Request,
                message: format!("Listing {} projects", rows.len()),
                level: NewLogLevel::Info,
                timestamp: chrono::Utc::now().naive_utc(),
            },
        );
        Ok(rows)
    }

    /// NEW: add a user to a project
    pub fn add_user_to_project(
        &self,
        user: &Auth,
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
        self.log_service.lock().unwrap().new_log(
            &user,
            NewLogStripped {
                category: NewLogCategory::Project,
                subcategory: NewLogSubcategory::Request,
                message: format!("Adding user {} to project {}", user_id, project_id),
                level: NewLogLevel::Info,
                timestamp: chrono::Utc::now().naive_utc(),
            },
        );
        Ok(())
    }
}
