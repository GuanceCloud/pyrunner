use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Script {
    pub id: String,
    pub path: String,
    pub hash: String,
    pub description: Option<String>,
    pub language: String,
    pub entrypoint: Option<String>,
    pub interpreter: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub use_count: i64,
    pub tags: Vec<String>,
    pub functions: Vec<Function>,
    pub dependencies: Vec<String>,
    pub input_types: Vec<String>,
    pub output_types: Vec<String>,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub signature: Option<String>,
    pub description: Option<String>,
}
