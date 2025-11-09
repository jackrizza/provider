/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/
use crate::models::Auth;
use crate::schema::{
    group_members, group_providers, project_providers, project_users, projects, user_providers,
};
use diesel::prelude::*;
use std::collections::HashSet;

pub struct AclService<'a> {
    pub conn: &'a mut diesel::sqlite::SqliteConnection,
}

impl<'a> AclService<'a> {
    pub fn new(conn: &'a mut diesel::sqlite::SqliteConnection) -> Self {
        Self { conn }
    }

    /// Get all provider names the user is allowed to use, regardless of project.
    pub fn providers_for_user(&mut self, user: &Auth) -> diesel::QueryResult<Vec<String>> {
        use crate::schema::group_members::dsl as gm;
        use crate::schema::group_providers::dsl as gp;
        use crate::schema::user_providers::dsl as up;

        let mut set = HashSet::new();

        // 1) user_providers (manual)
        let ups: Vec<(String, String)> = up::user_providers
            .filter(up::user_id.eq(user.id.as_ref().unwrap()))
            .select((up::user_id, up::provider_name))
            .load(self.conn)?;
        for (_, p) in ups {
            set.insert(p);
        }

        // 2) group-based
        let groups_for_user: Vec<String> = gm::group_members
            .filter(gm::user_id.eq(user.id.as_ref().unwrap()))
            .select(gm::group_id)
            .load(self.conn)?;

        if !groups_for_user.is_empty() {
            let gprovs: Vec<(String, String)> = gp::group_providers
                .filter(gp::group_id.eq_any(&groups_for_user))
                .select((gp::group_id, gp::provider_name))
                .load(self.conn)?;
            for (_, p) in gprovs {
                set.insert(p);
            }
        }

        Ok(set.into_iter().collect())
    }

    /// Get providers allowed for THIS user IN THIS project.
    /// = intersection(user_allowed, project_allowed)
    pub fn providers_for_user_in_project(
        &mut self,
        user: &Auth,
        project_id: &str,
    ) -> diesel::QueryResult<Vec<String>> {
        use crate::schema::project_providers::dsl as pp;

        // project allows:
        let project_allows: Vec<String> = pp::project_providers
            .filter(pp::project_id.eq(project_id))
            .select(pp::provider_name)
            .load(self.conn)?;

        // user allows:
        let user_allows = self.providers_for_user(user)?;

        let pset: HashSet<_> = project_allows.into_iter().collect();
        let uset: HashSet<_> = user_allows.into_iter().collect();

        let final_set: Vec<String> = pset.intersection(&uset).cloned().collect();
        Ok(final_set)
    }
}
