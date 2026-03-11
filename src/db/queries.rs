pub const INSERT_SCRIPT: &str = "INSERT OR IGNORE INTO scripts (id, path, hash, description, language, entrypoint, interpreter, created_at, input_types, output_types, parameters, search_text) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)";
pub const INSERT_FTS: &str = "INSERT INTO scripts_fts (script_id, search_text) VALUES (?1, ?2)";
