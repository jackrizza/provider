/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("database error: {0}")]
    Db(#[from] diesel::result::Error),

    #[error("user already exists")]
    UserExists,

    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("token expired")]
    TokenExpired,

    #[error("token not found")]
    TokenNotFound,

    #[error("hashing error")]
    HashingError,

    #[error("other: {0}")]
    Other(String),
}
