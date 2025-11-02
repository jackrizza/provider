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

    pub fn find_by_access_token(&mut self, token: &str) -> Result<Option<Auth>, AuthError> {
        let res = auth_dsl::auth
            .filter(auth_dsl::access_token.eq(token))
            .select(Auth::as_select())
            .first::<Auth>(self.conn)
            .optional()?;
        Ok(res)
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
}
