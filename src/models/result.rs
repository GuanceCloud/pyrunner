use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchDetails {
    pub fts_score: f64,
    pub tag_score: f64,
    pub function_score: f64,
    pub usage_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub script_id: String,
    pub path: String,
    pub score: f64,
    pub description: Option<String>,
    pub match_details: MatchDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptRegistration {
    pub script_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub script_id: String,
    pub path: String,
}

impl From<ScriptRegistration> for RegisterResponse {
    fn from(value: ScriptRegistration) -> Self {
        Self {
            script_id: value.script_id,
            path: value.path,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptCheck {
    pub exists: bool,
    pub script_id: Option<String>,
    pub path: Option<String>,
    pub score: Option<f64>,
    pub action: Option<String>,
    pub execute_command: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResponse {
    pub exists: bool,
    pub script_id: Option<String>,
    pub path: Option<String>,
    pub score: Option<f64>,
    pub action: Option<String>,
    pub execute_command: Option<Vec<String>>,
}

impl From<ScriptCheck> for CheckResponse {
    fn from(value: ScriptCheck) -> Self {
        Self {
            exists: value.exists,
            script_id: value.script_id,
            path: value.path,
            score: value.score,
            action: value.action,
            execute_command: value.execute_command,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPlan {
    pub script_id: String,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub script_id: String,
    pub command: Vec<String>,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptInfo {
    pub script_id: String,
    pub path: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteResult {
    pub script_id: String,
    pub path: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResult {
    pub script_id: String,
    pub path: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub updated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsSummary {
    pub total_scripts: i64,
    pub total_tags: i64,
    pub total_dependencies: i64,
    pub total_usage_events: i64,
    pub most_used_script_id: Option<String>,
    pub most_used_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupCandidate {
    pub script_id: String,
    pub path: String,
    pub use_count: i64,
    pub created_at: String,
    pub last_used: Option<String>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupResult {
    pub dry_run: bool,
    pub deleted_count: usize,
    pub candidates: Vec<CleanupCandidate>,
}
