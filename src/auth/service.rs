/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use diesel::r2d2::{ConnectionManager, PooledConnection};
use diesel::sqlite::SqliteConnection;
use diesel::{QueryDsl, RunQueryDsl};

use crate::DbPool;
use crate::auth::errors::AuthError;
use crate::auth::repo::AuthRepo;
use crate::auth::utils;
use crate::models::{Auth, NewAuth};

pub struct AuthService {
    pool: DbPool,
    // whether auth is enabled (from your CLI flag)
    enabled: bool,
}

impl AuthService {
    pub fn new(pool: DbPool, enabled: bool) -> Self {
        Self { pool, enabled }
    }

    fn conn(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, AuthError> {
        self.pool
            .get()
            .map_err(|e| AuthError::Other(format!("pool error: {e}")))
    }

    /// Check if there are any users in the database
    pub fn has_any_users(&self) -> Result<bool, AuthError> {
        let mut conn = self.conn()?;
        use crate::schema::auth::dsl::*;
        let count: i64 = auth.count().get_result(&mut conn).map_err(AuthError::Db)?;
        Ok(count > 0)
    }

    /// Register a new user (for admin dashboard, initial setup, etc.)
    pub fn register(&self, email: &str, password: &str) -> Result<Auth, AuthError> {
        let mut conn = self.conn()?;
        let mut repo = AuthRepo::new(&mut conn);

        if repo.find_by_email(email)?.is_some() {
            return Err(AuthError::UserExists);
        }

        let hashed = utils::hash_password(password).map_err(|_| AuthError::HashingError)?;

        let id = uuid::Uuid::new_v4().to_string();
        let access_token = utils::new_access_token();
        let refresh_token = utils::new_refresh_token();
        let (access_exp, refresh_exp) = utils::token_expirations();
        let now = utils::now_rfc3339();

        let new_auth = NewAuth {
            id: id.clone(),
            email: email.to_string(),
            password: hashed,
            refresh_token,
            access_token,
            refresh_token_expires_at: refresh_exp,
            access_token_expires_at: access_exp,
            state: "active".to_string(),
            last_error: "".to_string(),
            updated_at: now,
        };

        repo.insert(&new_auth)?;

        // return the inserted row
        let user = repo
            .find_by_email(email)?
            .ok_or_else(|| AuthError::Other("inserted user not found".into()))?;

        Ok(user)
    }

    /// Login and get tokens
    pub fn login(&self, email: &str, password: &str) -> Result<Auth, AuthError> {
        let mut conn = self.conn()?;
        let mut repo = AuthRepo::new(&mut conn);

        let mut user = match repo.find_by_email(email)? {
            Some(u) => u,
            None => return Err(AuthError::InvalidCredentials),
        };

        if !utils::verify_password(password, &user.password).map_err(|_| AuthError::HashingError)? {
            return Err(AuthError::InvalidCredentials);
        }

        // rotate tokens on every login
        let access_token = utils::new_access_token();
        let refresh_token = utils::new_refresh_token();
        let (access_exp, refresh_exp) = utils::token_expirations();

        repo.update_tokens(
            &user.id.unwrap(),
            &access_token,
            &access_exp,
            &refresh_token,
            &refresh_exp,
        )?;

        // refresh user
        user = repo
            .find_by_email(email)?
            .ok_or_else(|| AuthError::Other("user vanished".into()))?;
        Ok(user)
    }

    /// Validate access token (for TCP commands and web requests)
    pub fn validate_access_token(&self, token: &str) -> Result<Auth, AuthError> {
        if !self.enabled {
            // auth is off -> allow everything
            // you might return a dummy user here instead
            return Err(AuthError::Other("auth disabled".into()));
        }

        let mut conn = self.conn()?;
        let mut repo = AuthRepo::new(&mut conn);

        let user = repo
            .find_by_access_token(token)?
            .ok_or(AuthError::TokenNotFound)?;

        // check expiry
        let now = chrono::Utc::now();
        let exp = chrono::DateTime::parse_from_rfc3339(&user.access_token_expires_at)
            .map_err(|e| AuthError::Other(format!("bad date: {e}")))?
            .with_timezone(&chrono::Utc);

        if now > exp {
            return Err(AuthError::TokenExpired);
        }

        Ok(user)
    }

    /// For refresh-token flows (for web admin dashboard)
    pub fn rotate_with_refresh(&self, email: &str, refresh_token: &str) -> Result<Auth, AuthError> {
        let mut conn = self.conn()?;
        let mut repo = AuthRepo::new(&mut conn);

        let user = repo
            .find_by_email(email)?
            .ok_or(AuthError::InvalidCredentials)?;

        if user.refresh_token != refresh_token {
            return Err(AuthError::InvalidCredentials);
        }

        // check refresh expiry
        let now = chrono::Utc::now();
        let exp = chrono::DateTime::parse_from_rfc3339(&user.refresh_token_expires_at)
            .map_err(|e| AuthError::Other(format!("bad date: {e}")))?
            .with_timezone(&chrono::Utc);
        if now > exp {
            return Err(AuthError::TokenExpired);
        }

        let access_token = utils::new_access_token();
        let refresh_token = utils::new_refresh_token();
        let (access_exp, refresh_exp) = utils::token_expirations();

        repo.update_tokens(
            &user.id.unwrap(),
            &access_token,
            &access_exp,
            &refresh_token,
            &refresh_exp,
        )?;

        let user = repo
            .find_by_email(email)?
            .ok_or(AuthError::Other("user vanished".into()))?;

        Ok(user)
    }

    /// Whether auth is enabled at runtime
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}
