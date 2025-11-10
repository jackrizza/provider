use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use serde::{Deserialize, Serialize};

use crate::models::Auth;
use crate::models::{Log, NewLog};
use crate::schema::*;
use chrono::Utc;

pub type DbPool = diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<SqliteConnection>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewLogLevel {
    Info,
    Warning,
    Error,
}

impl NewLogLevel {
    pub fn as_string(&self) -> String {
        match self {
            NewLogLevel::Info => "INFO".into(),
            NewLogLevel::Warning => "WARNING".into(),
            NewLogLevel::Error => "ERROR".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewLogCategory {
    Provider,
    User,
    Project,
    Auth,
    None,
}

impl NewLogCategory {
    pub fn from_string(s: &str) -> Self {
        match s {
            "PROVIDER" => NewLogCategory::Provider,
            "USER" => NewLogCategory::User,
            "PROJECT" => NewLogCategory::Project,
            "AUTH" => NewLogCategory::Auth,
            _ => NewLogCategory::None,
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            NewLogCategory::Provider => "PROVIDER".into(),
            NewLogCategory::User => "USER".into(),
            NewLogCategory::Project => "PROJECT".into(),
            NewLogCategory::Auth => "AUTH".into(),
            NewLogCategory::None => "NONE".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewLogSubcategory {
    Add,
    Delete,
    Update,
    Request,
    Specific(String),
}

impl NewLogSubcategory {
    pub fn as_string(&self) -> String {
        match self {
            NewLogSubcategory::Add => "ADD".into(),
            NewLogSubcategory::Delete => "DELETE".into(),
            NewLogSubcategory::Update => "UPDATE".into(),
            NewLogSubcategory::Request => "REQUEST".into(),
            NewLogSubcategory::Specific(s) => format!("SPECIFIC::{}", s),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewLogStripped {
    pub category: NewLogCategory,
    pub subcategory: NewLogSubcategory,
    pub message: String,
    pub level: NewLogLevel,
    pub timestamp: chrono::NaiveDateTime,
}

#[derive(Clone)]
pub struct LogService {
    pool: DbPool,
}

impl LogService {
    pub fn new(pool: DbPool) -> Self {
        LogService { pool }
    }
    fn conn(
        &self,
    ) -> Result<
        diesel::r2d2::PooledConnection<diesel::r2d2::ConnectionManager<SqliteConnection>>,
        String,
    > {
        self.pool.get().map_err(|e| format!("pool.get(): {e}"))
    }

    pub fn new_log(&self, auth: &Auth, log: NewLogStripped) -> Result<(), String> {
        use diesel::prelude::*;
        let mut conn = self.conn()?;
        let user_id = auth.id.clone().unwrap(); //TODO: THIS WILL CRASH SOMEHOW
        let id = uuid::Uuid::new_v4().to_string();
        let log = NewLog {
            id,
            user_id,
            category: log.category.as_string(),
            subcategory: log.subcategory.as_string(),
            message: log.message,
            level: log.level.as_string(),
            timestamp: log.timestamp,
        };
        diesel::insert_into(logs::table)
            .values(&log)
            .execute(&mut conn)
            .map_err(|e| format!("insert_into(logs::table): {e}"))?;
        Ok(())
    }

    pub fn get_logs(&self, auth: &Auth, category: NewLogCategory) -> Result<Vec<Log>, String> {
        use diesel::prelude::*;
        let mut conn = self.conn()?;
        let user_id = auth.id.clone().unwrap(); //TODO: THIS WILL CRASH SOMEHOW
        let logs = logs::table
            .filter(logs::user_id.eq(user_id))
            .filter(logs::category.eq(category.as_string()))
            .load::<Log>(&mut conn)
            .map_err(|e| format!("load::<Log>(): {e}"))?;
        Ok(logs)
    }
}
