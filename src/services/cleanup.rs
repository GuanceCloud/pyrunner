use anyhow::Result;
use chrono::{Duration, Utc};
use rusqlite::{params, OptionalExtension};
use std::fs;
use std::path::Path;

use crate::config::Config;
use crate::db::Database;
use crate::models::result::{CleanupCandidate, CleanupResult, DeleteResult, StatsSummary};

pub struct CleanupService {
    config: Config,
}

impl CleanupService {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn delete_script(
        &self,
        database: &Database,
        script_id: &str,
    ) -> Result<Option<DeleteResult>> {
        let resolved_id = match database.resolve_script_id(script_id)? {
            Some(value) => value,
            None => return Ok(None),
        };
        let connection = database.connection();
        let path = connection
            .query_row(
                "SELECT path FROM scripts WHERE id = ?1",
                params![&resolved_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        let path = match path {
            Some(path) => path,
            None => return Ok(None),
        };

        connection.execute(
            "DELETE FROM scripts_fts WHERE script_id = ?1",
            params![&resolved_id],
        )?;
        connection.execute("DELETE FROM scripts WHERE id = ?1", params![&resolved_id])?;

        let script_path = Path::new(&path);
        if script_path.exists() {
            let _ = fs::remove_file(script_path);
        }

        Ok(Some(DeleteResult {
            script_id: resolved_id,
            path,
            deleted: true,
        }))
    }

    pub fn stats(&self, database: &Database) -> Result<StatsSummary> {
        let connection = database.connection();
        let total_scripts =
            connection.query_row("SELECT COUNT(*) FROM scripts", [], |row| row.get(0))?;
        let total_tags = connection.query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))?;
        let total_dependencies =
            connection.query_row("SELECT COUNT(*) FROM dependencies", [], |row| row.get(0))?;
        let total_usage_events =
            connection.query_row("SELECT COUNT(*) FROM usage_history", [], |row| row.get(0))?;
        let most_used = connection
            .query_row(
                "SELECT id, use_count FROM scripts ORDER BY use_count DESC, created_at DESC LIMIT 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;

        let (most_used_script_id, most_used_count) = match most_used {
            Some((script_id, count)) => (Some(script_id), count),
            None => (None, 0),
        };

        Ok(StatsSummary {
            total_scripts,
            total_tags,
            total_dependencies,
            total_usage_events,
            most_used_script_id,
            most_used_count,
        })
    }

    pub fn clean(
        &self,
        database: &Database,
        older_than: Option<u32>,
        unused: bool,
        dry_run: bool,
    ) -> Result<CleanupResult> {
        let candidates = self.find_candidates(database, older_than, unused)?;
        if dry_run {
            return Ok(CleanupResult {
                dry_run: true,
                deleted_count: 0,
                candidates,
            });
        }

        let mut deleted_count = 0usize;
        for candidate in &candidates {
            if self
                .delete_script(database, &candidate.script_id)?
                .is_some()
            {
                deleted_count += 1;
            }
        }

        Ok(CleanupResult {
            dry_run: false,
            deleted_count,
            candidates,
        })
    }

    fn find_candidates(
        &self,
        database: &Database,
        older_than: Option<u32>,
        unused: bool,
    ) -> Result<Vec<CleanupCandidate>> {
        let connection = database.connection();
        let mut statement = connection.prepare(
            "SELECT id, path, use_count, created_at, last_used FROM scripts ORDER BY created_at ASC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;

        let effective_older_than = if older_than.is_none() && !unused {
            Some(self.config.max_age_days)
        } else {
            older_than
        };
        let cutoff = effective_older_than.map(|days| Utc::now() - Duration::days(days as i64));
        let mut candidates = Vec::new();
        for row in rows {
            let (script_id, path, use_count, created_at, last_used) = row?;
            let mut reasons = Vec::new();

            if let Some(cutoff) = cutoff {
                let stale_at = last_used.as_deref().unwrap_or(&created_at);
                if let Ok(stale_dt) = chrono::DateTime::parse_from_rfc3339(stale_at) {
                    if stale_dt.with_timezone(&Utc) < cutoff {
                        reasons.push(format!(
                            "older-than-{}-days",
                            effective_older_than.unwrap_or_default()
                        ));
                    }
                }
            }

            if unused && use_count == 0 {
                reasons.push("unused".to_string());
            }

            if reasons.is_empty() {
                continue;
            }

            candidates.push(CleanupCandidate {
                script_id,
                path,
                use_count,
                created_at,
                last_used,
                reasons,
            });
        }

        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::CleanupService;
    use crate::config::{Config, LoggingConfig, MatchingConfig};
    use crate::db::Database;
    use chrono::Utc;
    use rusqlite::params;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn clean_finds_unused_and_old_scripts() {
        let root = unique_dir("cleanup-unit");
        fs::create_dir_all(&root).expect("create root");
        let cache_dir = root.join(".pyrunner");
        let config = test_config(cache_dir.clone());
        let database = Database::new(&config.database_path()).expect("db");
        let service = CleanupService::new(config);
        let script_path = cache_dir.join("scripts/2026-03/demo.py");
        if let Some(parent) = script_path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&script_path, "print('demo')").expect("write script");

        database.connection().execute(
            "INSERT INTO scripts (id, path, hash, description, language, entrypoint, interpreter, created_at, last_used, use_count, input_types, output_types, parameters, search_text) VALUES (?1, ?2, ?3, ?4, 'python', '__main__', 'python3', ?5, NULL, 0, '[]', '[]', '{}', '')",
            params![
                "demo_1",
                script_path.to_string_lossy().to_string(),
                "hash-1",
                "demo",
                "2020-01-01T00:00:00+00:00"
            ],
        ).expect("insert script");

        let result = service
            .clean(&database, Some(30), true, true)
            .expect("dry run clean");
        assert_eq!(result.deleted_count, 0);
        assert_eq!(result.candidates.len(), 1);
        assert!(result.candidates[0]
            .reasons
            .iter()
            .any(|item| item == "unused"));
    }

    #[test]
    fn clean_respects_last_used_when_present() {
        let root = unique_dir("cleanup-last-used");
        fs::create_dir_all(&root).expect("create root");
        let cache_dir = root.join(".pyrunner");
        let config = test_config(cache_dir.clone());
        let database = Database::new(&config.database_path()).expect("db");
        let service = CleanupService::new(config);

        database.connection().execute(
            "INSERT INTO scripts (id, path, hash, description, language, entrypoint, interpreter, created_at, last_used, use_count, input_types, output_types, parameters, search_text) VALUES (?1, ?2, ?3, ?4, 'python', '__main__', 'python3', ?5, ?6, 5, '[]', '[]', '{}', '')",
            params![
                "demo_2",
                cache_dir.join("scripts/demo.py").to_string_lossy().to_string(),
                "hash-2",
                "demo",
                "2020-01-01T00:00:00+00:00",
                Utc::now().to_rfc3339(),
            ],
        ).expect("insert script");

        let result = service
            .clean(&database, Some(30), false, true)
            .expect("dry run clean");
        assert!(result.candidates.is_empty());
    }

    fn unique_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        env::temp_dir().join(format!("{prefix}-{nanos}"))
    }

    fn test_config(cache_dir: PathBuf) -> Config {
        Config {
            cache_dir: cache_dir.clone(),
            default_interpreter: "python3".to_string(),
            max_scripts: 1000,
            max_age_days: 90,
            default_timeout_secs: 60,
            logging: LoggingConfig {
                level: "info".to_string(),
                file: cache_dir.join("logs").join("pyrunner.log"),
            },
            matching: MatchingConfig {
                similarity_threshold: 0.85,
                fts_weight: 0.50,
                tag_weight: 0.20,
                function_weight: 0.20,
                usage_weight: 0.10,
            },
        }
    }
}
