use crate::http::AppState;
use crate::http::services::logs::NewLogStripped;
use crate::models::Auth;
use axum::{
    extract::{Json, Path, State},
    response::{Html, IntoResponse, Redirect},
};
