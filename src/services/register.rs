use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;
use std::io::{self, Read};
use std::path::Path;

use crate::config::Config;
use crate::db::{queries, Database};
use crate::models::result::{ScriptRegistration, UpdateResult};
use crate::services::parser::PythonParser;
use crate::utils::hash::sha256_string;
use crate::utils::paths::{script_identifier, script_storage_path};

pub struct RegisterService {
    config: Config,
}

impl RegisterService {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn register_file(
        &self,
        database: &Database,
        script_path: &str,
        desc: Option<String>,
        tags: Vec<String>,
    ) -> Result<ScriptRegistration> {
        let content = fs::read_to_string(script_path)?;
        self.register_content(database, content, desc.unwrap_or_default(), tags)
    }

    pub fn register_source(
        &self,
        database: &Database,
        script_file: Option<String>,
        stdin: bool,
        script_text: Option<String>,
        desc: String,
        tags: Vec<String>,
    ) -> Result<ScriptRegistration> {
        let content = match (script_file, stdin, script_text) {
            (Some(path), false, None) => fs::read_to_string(path)?,
            (None, true, None) => read_stdin()?,
            (None, false, Some(content)) => content,
            (None, false, None) => {
                bail!("one of --script-file, --stdin, or --script-text is required")
            }
            _ => bail!("script source options are mutually exclusive"),
        };

        self.register_content(database, content, desc, tags)
    }

    pub fn update_metadata(
        &self,
        database: &Database,
        script_id: &str,
        desc: Option<String>,
        tags: Option<Vec<String>>,
    ) -> Result<Option<UpdateResult>> {
        let resolved_id = match database.resolve_script_id(script_id)? {
            Some(value) => value,
            None => return Ok(None),
        };
        let connection = database.connection();
        let existing = connection
            .query_row(
                "SELECT path, description FROM scripts WHERE id = ?1",
                params![&resolved_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?;

        let (path, current_description) = match existing {
            Some(value) => value,
            None => return Ok(None),
        };

        let final_description = match desc {
            Some(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            None => current_description,
        };

        run_transaction(connection, |connection| {
            connection.execute(
                "UPDATE scripts SET description = ?1 WHERE id = ?2",
                params![final_description.clone(), &resolved_id],
            )?;

            if let Some(tags) = &tags {
                connection.execute("DELETE FROM tags WHERE script_id = ?1", params![&resolved_id])?;
                for tag in tags {
                    connection.execute(
                        "INSERT OR IGNORE INTO tags (script_id, tag) VALUES (?1, ?2)",
                        params![&resolved_id, tag],
                    )?;
                }
            }

            let refreshed_tags = load_tags_for_update_with_connection(connection, &resolved_id)?;
            let refreshed_search_text = rebuild_search_text_with_connection(
                connection,
                &resolved_id,
                final_description.as_deref().unwrap_or_default(),
                &refreshed_tags,
            )?;
            connection.execute(
                "UPDATE scripts SET search_text = ?1 WHERE id = ?2",
                params![&refreshed_search_text, &resolved_id],
            )?;
            connection.execute(
                "DELETE FROM scripts_fts WHERE script_id = ?1",
                params![&resolved_id],
            )?;
            connection.execute(
                queries::INSERT_FTS,
                params![&resolved_id, &refreshed_search_text],
            )?;

            Ok(())
        })?;

        Ok(Some(UpdateResult {
            script_id: resolved_id.clone(),
            path,
            description: final_description,
            tags: load_tags_for_update(database, &resolved_id)?,
            updated: true,
        }))
    }

    fn register_content(
        &self,
        database: &Database,
        content: String,
        desc: String,
        tags: Vec<String>,
    ) -> Result<ScriptRegistration> {
        let hash = sha256_string(&content);
        if let Some(existing) = find_existing_by_hash(database, &hash)? {
            return Ok(existing);
        }

        let now = Utc::now();
        let destination = script_storage_path(&self.config.cache_dir, &hash, now)?;
        let functions = PythonParser::parse_functions(&content)?;
        let dependencies = PythonParser::parse_dependencies(&content)?;
        let script_id = script_identifier(&hash, now);
        let description = if desc.trim().is_empty() {
            None
        } else {
            Some(desc)
        };
        let function_names = functions
            .iter()
            .map(|item| item.name.clone())
            .collect::<Vec<_>>();
        let function_signatures = functions
            .iter()
            .filter_map(|item| item.signature.clone())
            .collect::<Vec<_>>();
        let search_text = build_search_text(
            description.as_deref().unwrap_or_default(),
            &tags,
            &function_names,
            &function_signatures,
            &dependencies,
            &[],
        );

        let connection = database.connection();
        let write_result = run_transaction(connection, |connection| {
            let inserted = connection.execute(
                queries::INSERT_SCRIPT,
                params![
                    script_id.to_string(),
                    destination.to_string_lossy().to_string(),
                    hash,
                    description.clone(),
                    "python",
                    "__main__",
                    self.config.default_interpreter.clone(),
                    now.to_rfc3339(),
                    "[]",
                    "[]",
                    "{}",
                    &search_text,
                ],
            )?;
            if inserted == 0 {
                return Ok(RegisterOutcome::Existing(load_existing_by_hash_with_connection(
                    connection, &hash,
                )?
                .ok_or_else(|| anyhow!("script with hash {hash} was not found after insert ignore"))?));
            }

            connection.execute(
                queries::INSERT_FTS,
                params![script_id.to_string(), &search_text],
            )?;

            for tag in &tags {
                connection.execute(
                    "INSERT OR IGNORE INTO tags (script_id, tag) VALUES (?1, ?2)",
                    params![script_id.to_string(), tag],
                )?;
            }
            for function in &functions {
                connection.execute(
                    "INSERT INTO functions (script_id, name, signature, description) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        script_id.to_string(),
                        &function.name,
                        &function.signature,
                        &function.description
                    ],
                )?;
            }
            for dependency in &dependencies {
                connection.execute(
                    "INSERT OR IGNORE INTO dependencies (script_id, dependency) VALUES (?1, ?2)",
                    params![script_id.to_string(), dependency],
                )?;
            }

            persist_script(&destination, &content)?;
            Ok(RegisterOutcome::Inserted)
        });

        match write_result {
            Ok(RegisterOutcome::Inserted) => {}
            Ok(RegisterOutcome::Existing(existing)) => return Ok(existing),
            Err(error) => {
                cleanup_persisted_script(&destination);
                return Err(error);
            }
        }

        enforce_max_scripts(database, self.config.max_scripts)?;

        Ok(ScriptRegistration {
            script_id,
            path: destination.to_string_lossy().to_string(),
        })
    }
}

enum RegisterOutcome {
    Inserted,
    Existing(ScriptRegistration),
}

fn persist_script(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

fn read_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    Ok(buffer)
}

fn run_transaction<T, F>(connection: &Connection, operation: F) -> Result<T>
where
    F: FnOnce(&Connection) -> Result<T>,
{
    connection.execute_batch("BEGIN IMMEDIATE TRANSACTION;")?;
    match operation(connection) {
        Ok(value) => {
            if let Err(error) = connection.execute_batch("COMMIT;") {
                let _ = connection.execute_batch("ROLLBACK;");
                Err(error.into())
            } else {
                Ok(value)
            }
        }
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK;");
            Err(error)
        }
    }
}

fn cleanup_persisted_script(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn find_existing_by_hash(database: &Database, hash: &str) -> Result<Option<ScriptRegistration>> {
    load_existing_by_hash_with_connection(database.connection(), hash)
}

fn load_existing_by_hash_with_connection(
    connection: &Connection,
    hash: &str,
) -> Result<Option<ScriptRegistration>> {
    let existing = connection
        .query_row(
            "SELECT id, path FROM scripts WHERE hash = ?1 LIMIT 1",
            params![hash],
            |row| {
                Ok(ScriptRegistration {
                    script_id: row.get(0)?,
                    path: row.get(1)?,
                })
            },
        )
        .optional()?;
    Ok(existing)
}

fn enforce_max_scripts(database: &Database, max_scripts: usize) -> Result<()> {
    if max_scripts == 0 {
        return Ok(());
    }

    let connection = database.connection();
    let total_scripts: usize =
        connection.query_row("SELECT COUNT(*) FROM scripts", [], |row| row.get(0))?;
    if total_scripts <= max_scripts {
        return Ok(());
    }

    let overflow = total_scripts - max_scripts;
    let mut statement = connection.prepare(
        "SELECT id, path
         FROM scripts
         ORDER BY COALESCE(last_used, created_at) ASC, created_at ASC, id ASC
         LIMIT ?1",
    )?;
    let rows = statement.query_map(params![overflow as i64], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut victims = Vec::new();
    for row in rows {
        victims.push(row?);
    }

    run_transaction(connection, |connection| {
        for (script_id, _) in &victims {
            connection.execute(
                "DELETE FROM scripts_fts WHERE script_id = ?1",
                params![script_id],
            )?;
            connection.execute("DELETE FROM scripts WHERE id = ?1", params![script_id])?;
        }
        Ok(())
    })?;

    for (_, path) in victims {
        let script_path = Path::new(&path);
        if script_path.exists() {
            let _ = fs::remove_file(script_path);
        }
    }

    Ok(())
}

fn build_search_text(
    description: &str,
    tags: &[String],
    function_names: &[String],
    function_signatures: &[String],
    dependencies: &[String],
    extra_terms: &[String],
) -> String {
    [
        description.to_string(),
        tags.join(" "),
        function_names.join(" "),
        function_signatures.join(" "),
        dependencies.join(" "),
        extra_terms.join(" "),
    ]
    .join(" ")
    .trim()
    .to_string()
}

fn rebuild_search_text_with_connection(
    connection: &Connection,
    script_id: &str,
    description: &str,
    tags: &[String],
) -> Result<String> {
    let mut function_statement =
        connection.prepare("SELECT name, signature FROM functions WHERE script_id = ?1")?;
    let functions = function_statement.query_map(params![script_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    })?;

    let mut function_names = Vec::new();
    let mut function_signatures = Vec::new();
    for row in functions {
        let (name, signature) = row?;
        function_names.push(name);
        if let Some(signature) = signature {
            function_signatures.push(signature);
        }
    }

    let mut dependency_statement =
        connection.prepare("SELECT dependency FROM dependencies WHERE script_id = ?1")?;
    let dependencies =
        dependency_statement.query_map(params![script_id], |row| row.get::<_, String>(0))?;

    let mut dependency_values = Vec::new();
    for row in dependencies {
        dependency_values.push(row?);
    }
    let mut extra_term_statement =
        connection.prepare("SELECT term FROM script_search_terms WHERE script_id = ?1 ORDER BY term ASC")?;
    let extra_terms = extra_term_statement.query_map(params![script_id], |row| row.get::<_, String>(0))?;
    let mut extra_term_values = Vec::new();
    for row in extra_terms {
        extra_term_values.push(row?);
    }

    Ok(build_search_text(
        description,
        tags,
        &function_names,
        &function_signatures,
        &dependency_values,
        &extra_term_values,
    ))
}

fn load_tags_for_update(database: &Database, script_id: &str) -> Result<Vec<String>> {
    load_tags_for_update_with_connection(database.connection(), script_id)
}

fn load_tags_for_update_with_connection(
    connection: &Connection,
    script_id: &str,
) -> Result<Vec<String>> {
    let mut statement =
        connection.prepare("SELECT tag FROM tags WHERE script_id = ?1 ORDER BY tag ASC")?;
    let rows = statement.query_map(params![script_id], |row| row.get::<_, String>(0))?;

    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::build_search_text;

    #[test]
    fn build_search_text_concatenates_indexable_fields() {
        let text = build_search_text(
            "demo script",
            &vec!["demo".to_string(), "json".to_string()],
            &vec!["run_demo".to_string()],
            &vec!["run_demo(arg: str)".to_string()],
            &vec!["json".to_string()],
            &vec!["legacy term".to_string()],
        );

        assert!(text.contains("demo script"));
        assert!(text.contains("demo json"));
        assert!(text.contains("run_demo"));
        assert!(text.contains("legacy term"));
    }
}
