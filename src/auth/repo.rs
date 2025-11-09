/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

use crate::auth::errors::AuthError;
use crate::models::{Auth, NewAuth};
use crate::schema::auth::dsl as auth_dsl;

pub struct AuthRepo<'a> {
    conn: &'a mut SqliteConnection,
}

impl<'a> AuthRepo<'a> {
    pub fn new(conn: &'a mut SqliteConnection) -> Self {
        Self { conn }
    }

    pub fn find_by_email(&mut self, email: &str) -> Result<Option<Auth>, AuthError> {
        let res = auth_dsl::auth
            .filter(auth_dsl::email.eq(email))
            .select(Auth::as_select())
            .first::<Auth>(self.conn)
            .optional()?;
        Ok(res)
    }

    pub fn find_email_by_id(&mut self, user_id: &str) -> Result<Option<String>, AuthError> {
        let res = auth_dsl::auth
            .filter(auth_dsl::id.eq(user_id))
            .select(auth_dsl::email)
            .first::<String>(self.conn)
            .optional()?;
        Ok(res)
    }

    pub fn find_by_access_token(&mut self, token: &str) -> Result<Option<Auth>, AuthError> {
        let res = auth_dsl::auth
            .filter(auth_dsl::access_token.eq(token))
            .select(Auth::as_select())
            .first::<Auth>(self.conn)
            .optional()?;
        Ok(res)
    }

    pub fn list_all(&mut self) -> diesel::QueryResult<Vec<Auth>> {
        use crate::schema::auth::dsl::*;
        auth.order(email.asc()).load::<Auth>(self.conn)
    }

    pub fn delete_by_id(&mut self, user_id: &str) -> diesel::QueryResult<usize> {
        use crate::schema::auth::dsl::*;
        diesel::delete(auth.filter(id.eq(user_id))).execute(self.conn)
    }

    pub fn insert(&mut self, new_auth: &NewAuth) -> Result<(), AuthError> {
        diesel::insert_into(auth_dsl::auth)
            .values(new_auth)
            .execute(self.conn)?;
        Ok(())
    }

    pub fn update_tokens(
        &mut self,
        user_id: &str,
        access_token: &str,
        access_expires_at: &str,
        refresh_token: &str,
        refresh_expires_at: &str,
    ) -> Result<(), AuthError> {
        diesel::update(auth_dsl::auth.filter(auth_dsl::id.eq(user_id)))
            .set((
                auth_dsl::access_token.eq(access_token),
                auth_dsl::access_token_expires_at.eq(access_expires_at),
                auth_dsl::refresh_token.eq(refresh_token),
                auth_dsl::refresh_token_expires_at.eq(refresh_expires_at),
                auth_dsl::updated_at.eq(crate::auth::utils::now_rfc3339()),
                auth_dsl::last_error.eq(""),
            ))
            .execute(self.conn)?;
        Ok(())
    }

    pub fn set_last_error(&mut self, user_id: &str, msg: &str) -> Result<(), AuthError> {
        diesel::update(auth_dsl::auth.filter(auth_dsl::id.eq(user_id)))
            .set((
                auth_dsl::last_error.eq(msg),
                auth_dsl::updated_at.eq(crate::auth::utils::now_rfc3339()),
            ))
            .execute(self.conn)?;
        Ok(())
    }

    pub fn update_role_by_email(
        &mut self,
        target_email: &str,
        new_role: &str,
    ) -> diesel::QueryResult<usize> {
        use crate::schema::auth::dsl::*;
        diesel::update(auth.filter(email.eq(target_email)))
            .set((
                role.eq(new_role),
                updated_at.eq(chrono::Utc::now().to_rfc3339()),
            ))
            .execute(self.conn)
    }
}
