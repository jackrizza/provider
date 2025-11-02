/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use chrono::{Duration, Utc};
use rand::rngs::OsRng;
use uuid::Uuid;

pub fn hash_password(plain: &str) -> Result<String, ()> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hashed = argon2
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|_| ())?
        .to_string();
    Ok(hashed)
}

pub fn verify_password(plain: &str, hashed: &str) -> Result<bool, ()> {
    let parsed = PasswordHash::new(hashed).map_err(|_| ())?;
    Ok(Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok())
}

pub fn new_access_token() -> String {
    Uuid::new_v4().to_string()
}

pub fn new_refresh_token() -> String {
    Uuid::new_v4().to_string()
}

/// returns (access_expires_at, refresh_expires_at) as RFC3339 strings
pub fn token_expirations() -> (String, String) {
    let now = Utc::now();
    // tweak as needed
    let access = now + Duration::minutes(30);
    let refresh = now + Duration::days(7);
    (access.to_rfc3339(), refresh.to_rfc3339())
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}
