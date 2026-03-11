use anyhow::Result;
use rusqlite::{params, Connection};

pub fn run(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS scripts (
            id TEXT PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            hash TEXT NOT NULL,
            description TEXT,
            language TEXT NOT NULL DEFAULT 'python',
            entrypoint TEXT,
            interpreter TEXT,
            created_at TEXT NOT NULL,
            last_used TEXT,
            use_count INTEGER NOT NULL DEFAULT 0,
            input_types TEXT NOT NULL DEFAULT '[]',
            output_types TEXT NOT NULL DEFAULT '[]',
            parameters TEXT NOT NULL DEFAULT '{}',
            search_text TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS tags (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            script_id TEXT NOT NULL,
            tag TEXT NOT NULL,
            FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE,
            UNIQUE(script_id, tag)
        );

        CREATE TABLE IF NOT EXISTS functions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            script_id TEXT NOT NULL,
            name TEXT NOT NULL,
            signature TEXT,
            description TEXT,
            FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS dependencies (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            script_id TEXT NOT NULL,
            dependency TEXT NOT NULL,
            FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE,
            UNIQUE(script_id, dependency)
        );

        CREATE TABLE IF NOT EXISTS usage_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            script_id TEXT NOT NULL,
            used_at TEXT NOT NULL,
            context TEXT,
            exit_code INTEGER,
            duration_ms INTEGER,
            FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS script_aliases (
            alias_id TEXT PRIMARY KEY,
            script_id TEXT NOT NULL,
            FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS script_search_terms (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            script_id TEXT NOT NULL,
            term TEXT NOT NULL,
            FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE,
            UNIQUE(script_id, term)
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS scripts_fts USING fts5(
            script_id UNINDEXED,
            search_text
        );
        "#,
    )?;

    merge_duplicate_hashes(connection)?;

    connection.execute_batch(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_scripts_hash_unique ON scripts(hash);
        CREATE INDEX IF NOT EXISTS idx_scripts_last_used ON scripts(last_used);
        CREATE INDEX IF NOT EXISTS idx_tags_script ON tags(script_id);
        CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags(tag);
        CREATE INDEX IF NOT EXISTS idx_functions_script ON functions(script_id);
        CREATE INDEX IF NOT EXISTS idx_functions_name ON functions(name);
        CREATE INDEX IF NOT EXISTS idx_dependencies_script ON dependencies(script_id);
        CREATE INDEX IF NOT EXISTS idx_dependencies_dep ON dependencies(dependency);
        CREATE INDEX IF NOT EXISTS idx_script_aliases_script ON script_aliases(script_id);
        CREATE INDEX IF NOT EXISTS idx_script_search_terms_script ON script_search_terms(script_id);
        "#,
    )?;

    Ok(())
}

#[derive(Clone)]
struct ScriptRow {
    id: String,
    description: Option<String>,
    interpreter: Option<String>,
    created_at: String,
    last_used: Option<String>,
    use_count: i64,
}

fn merge_duplicate_hashes(connection: &Connection) -> Result<()> {
    let mut statement =
        connection.prepare("SELECT hash FROM scripts GROUP BY hash HAVING COUNT(*) > 1")?;
    let hashes = statement.query_map([], |row| row.get::<_, String>(0))?;

    let mut values = Vec::new();
    for row in hashes {
        values.push(row?);
    }

    for hash in values {
        merge_hash_group(connection, &hash)?;
    }

    Ok(())
}

fn merge_hash_group(connection: &Connection, hash: &str) -> Result<()> {
    let mut statement = connection.prepare(
        "SELECT id, description, interpreter, created_at, last_used, use_count
         FROM scripts
         WHERE hash = ?1
         ORDER BY COALESCE(last_used, created_at) DESC, use_count DESC, rowid ASC",
    )?;
    let rows = statement.query_map(params![hash], |row| {
        Ok(ScriptRow {
            id: row.get(0)?,
            description: row.get(1)?,
            interpreter: row.get(2)?,
            created_at: row.get(3)?,
            last_used: row.get(4)?,
            use_count: row.get(5)?,
        })
    })?;

    let mut scripts = Vec::new();
    for row in rows {
        scripts.push(row?);
    }
    if scripts.len() <= 1 {
        return Ok(());
    }

    let survivor = scripts[0].clone();
    let duplicate_ids = scripts
        .iter()
        .skip(1)
        .map(|row| row.id.clone())
        .collect::<Vec<_>>();

    let merged_descriptions =
        unique_non_empty_strings(scripts.iter().filter_map(|row| row.description.clone()));
    let merged_description = merged_descriptions
        .iter()
        .max_by_key(|value| value.len())
        .cloned();
    let merged_interpreter = scripts.iter().find_map(|row| row.interpreter.clone());
    let merged_created_at = scripts
        .iter()
        .map(|row| row.created_at.clone())
        .min()
        .unwrap_or_else(|| survivor.created_at.clone());
    let merged_last_used = scripts.iter().filter_map(|row| row.last_used.clone()).max();
    let merged_use_count = scripts.iter().map(|row| row.use_count).sum::<i64>();

    connection.execute_batch("BEGIN IMMEDIATE TRANSACTION;")?;
    let merge_result = (|| -> Result<()> {
        for duplicate_id in &duplicate_ids {
            connection.execute(
                "UPDATE OR IGNORE tags SET script_id = ?1 WHERE script_id = ?2",
                params![&survivor.id, duplicate_id],
            )?;
            connection.execute(
                "UPDATE OR IGNORE dependencies SET script_id = ?1 WHERE script_id = ?2",
                params![&survivor.id, duplicate_id],
            )?;
            connection.execute(
                "UPDATE functions SET script_id = ?1 WHERE script_id = ?2",
                params![&survivor.id, duplicate_id],
            )?;
            connection.execute(
                "UPDATE usage_history SET script_id = ?1 WHERE script_id = ?2",
                params![&survivor.id, duplicate_id],
            )?;
            connection.execute(
                "UPDATE script_aliases SET script_id = ?1 WHERE script_id = ?2",
                params![&survivor.id, duplicate_id],
            )?;
            connection.execute(
                "INSERT OR REPLACE INTO script_aliases (alias_id, script_id) VALUES (?1, ?2)",
                params![duplicate_id, &survivor.id],
            )?;
        }
        for term in merged_descriptions.iter().filter(|term| {
            merged_description
                .as_deref()
                .map(|description| description.trim() != term.as_str())
                .unwrap_or(true)
        }) {
            connection.execute(
                "INSERT OR IGNORE INTO script_search_terms (script_id, term) VALUES (?1, ?2)",
                params![&survivor.id, term],
            )?;
        }
        deduplicate_functions(connection, &survivor.id)?;

        let merged_search_text =
            build_merged_search_text(connection, &survivor.id, &merged_descriptions)?;
        connection.execute(
            "UPDATE scripts
             SET description = ?1,
                 interpreter = ?2,
                 created_at = ?3,
                 last_used = ?4,
                 use_count = ?5,
                 search_text = ?6
             WHERE id = ?7",
            params![
                merged_description,
                merged_interpreter,
                merged_created_at,
                merged_last_used,
                merged_use_count,
                merged_search_text,
                &survivor.id,
            ],
        )?;

        connection.execute(
            "DELETE FROM scripts_fts WHERE script_id = ?1",
            params![&survivor.id],
        )?;
        for duplicate_id in &duplicate_ids {
            connection.execute("DELETE FROM scripts_fts WHERE script_id = ?1", params![duplicate_id])?;
            connection.execute("DELETE FROM scripts WHERE id = ?1", params![duplicate_id])?;
        }

        let search_text: String = connection.query_row(
            "SELECT search_text FROM scripts WHERE id = ?1",
            params![&survivor.id],
            |row| row.get(0),
        )?;
        connection.execute(
            "INSERT INTO scripts_fts (script_id, search_text) VALUES (?1, ?2)",
            params![&survivor.id, search_text],
        )?;

        Ok(())
    })();

    match merge_result {
        Ok(()) => connection.execute_batch("COMMIT;")?,
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK;");
            return Err(error);
        }
    }

    Ok(())
}

fn build_merged_search_text(
    connection: &Connection,
    script_id: &str,
    descriptions: &[String],
) -> Result<String> {
    let tags = load_column(connection, "SELECT tag FROM tags WHERE script_id = ?1 ORDER BY tag ASC", script_id)?;
    let function_names =
        load_column(connection, "SELECT name FROM functions WHERE script_id = ?1 ORDER BY name ASC", script_id)?;
    let function_signatures = load_optional_column(
        connection,
        "SELECT signature FROM functions WHERE script_id = ?1 ORDER BY name ASC",
        script_id,
    )?;
    let dependencies = load_column(
        connection,
        "SELECT dependency FROM dependencies WHERE script_id = ?1 ORDER BY dependency ASC",
        script_id,
    )?;
    let extra_terms = load_column(
        connection,
        "SELECT term FROM script_search_terms WHERE script_id = ?1 ORDER BY term ASC",
        script_id,
    )?;

    Ok([
        descriptions.join(" "),
        tags.join(" "),
        function_names.join(" "),
        function_signatures.join(" "),
        dependencies.join(" "),
        extra_terms.join(" "),
    ]
    .join(" ")
    .trim()
    .to_string())
}

fn load_column(connection: &Connection, sql: &str, script_id: &str) -> Result<Vec<String>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params![script_id], |row| row.get::<_, String>(0))?;
    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }
    Ok(values)
}

fn unique_non_empty_strings<I>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut result = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !result.iter().any(|item| item == trimmed) {
            result.push(trimmed.to_string());
        }
    }
    result
}

fn deduplicate_functions(connection: &Connection, script_id: &str) -> Result<()> {
    connection.execute(
        "DELETE FROM functions
         WHERE id IN (
             SELECT duplicate.id
             FROM functions duplicate
             JOIN functions survivor
               ON survivor.script_id = duplicate.script_id
              AND survivor.name = duplicate.name
              AND COALESCE(survivor.signature, '') = COALESCE(duplicate.signature, '')
              AND survivor.id < duplicate.id
             WHERE duplicate.script_id = ?1
         )",
        params![script_id],
    )?;
    Ok(())
}

fn load_optional_column(connection: &Connection, sql: &str, script_id: &str) -> Result<Vec<String>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params![script_id], |row| row.get::<_, Option<String>>(0))?;
    let mut values = Vec::new();
    for row in rows {
        if let Some(value) = row? {
            values.push(value);
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::run;
    use rusqlite::{params, Connection};

    #[test]
    fn migration_merges_duplicate_hash_metadata_before_creating_unique_index() {
        let connection = Connection::open_in_memory().expect("open db");
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;
                CREATE TABLE scripts (
                    id TEXT PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    hash TEXT NOT NULL,
                    description TEXT,
                    language TEXT NOT NULL DEFAULT 'python',
                    entrypoint TEXT,
                    interpreter TEXT,
                    created_at TEXT NOT NULL,
                    last_used TEXT,
                    use_count INTEGER NOT NULL DEFAULT 0,
                    input_types TEXT NOT NULL DEFAULT '[]',
                    output_types TEXT NOT NULL DEFAULT '[]',
                    parameters TEXT NOT NULL DEFAULT '{}',
                    search_text TEXT NOT NULL DEFAULT ''
                );
                CREATE TABLE tags (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    script_id TEXT NOT NULL,
                    tag TEXT NOT NULL,
                    FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE,
                    UNIQUE(script_id, tag)
                );
                CREATE TABLE functions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    script_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    signature TEXT,
                    description TEXT,
                    FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE
                );
                CREATE TABLE dependencies (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    script_id TEXT NOT NULL,
                    dependency TEXT NOT NULL,
                    FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE,
                    UNIQUE(script_id, dependency)
                );
                CREATE TABLE usage_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    script_id TEXT NOT NULL,
                    used_at TEXT NOT NULL,
                    context TEXT,
                    exit_code INTEGER,
                    duration_ms INTEGER,
                    FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE
                );
                CREATE TABLE script_aliases (
                    alias_id TEXT PRIMARY KEY,
                    script_id TEXT NOT NULL,
                    FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE
                );
                CREATE TABLE script_search_terms (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    script_id TEXT NOT NULL,
                    term TEXT NOT NULL,
                    FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE,
                    UNIQUE(script_id, term)
                );
                CREATE VIRTUAL TABLE scripts_fts USING fts5(script_id UNINDEXED, search_text);
                "#,
            )
            .expect("seed schema");

        connection.execute(
            "INSERT INTO scripts (id, path, hash, description, interpreter, created_at, last_used, use_count, search_text) VALUES (?1, ?2, ?3, NULL, 'python3', ?4, NULL, 1, '')",
            params!["old", "/tmp/old.py", "dup-hash", "2024-01-01T00:00:00+00:00"],
        ).expect("insert old");
        connection.execute(
            "INSERT INTO scripts (id, path, hash, description, interpreter, created_at, last_used, use_count, search_text) VALUES (?1, ?2, ?3, 'new desc', NULL, ?4, ?5, 4, '')",
            params!["new", "/tmp/new.py", "dup-hash", "2024-02-01T00:00:00+00:00", "2024-03-01T00:00:00+00:00"],
        ).expect("insert new");
        connection.execute("INSERT INTO tags (script_id, tag) VALUES (?1, ?2)", params!["old", "alpha"]).expect("tag old");
        connection.execute("INSERT INTO tags (script_id, tag) VALUES (?1, ?2)", params!["new", "beta"]).expect("tag new");
        connection.execute("INSERT INTO functions (script_id, name, signature, description) VALUES (?1, ?2, ?3, NULL)", params!["old", "do_work", "do_work(x)"]).expect("fn old dup");
        connection.execute("INSERT INTO functions (script_id, name, signature, description) VALUES (?1, ?2, ?3, NULL)", params!["new", "do_work", "do_work(x)"]).expect("fn");
        connection.execute("INSERT INTO dependencies (script_id, dependency) VALUES (?1, ?2)", params!["old", "json"]).expect("dep");
        connection.execute("INSERT INTO usage_history (script_id, used_at, context, exit_code, duration_ms) VALUES (?1, ?2, 'run', 0, 12)", params!["new", "2024-03-01T00:00:00+00:00"]).expect("usage");
        connection.execute("INSERT INTO scripts_fts (script_id, search_text) VALUES (?1, ?2)", params!["old", "alpha"]).expect("fts old");
        connection.execute("INSERT INTO scripts_fts (script_id, search_text) VALUES (?1, ?2)", params!["new", "beta"]).expect("fts new");

        run(&connection).expect("run migration");

        let remaining: i64 = connection
            .query_row("SELECT COUNT(*) FROM scripts WHERE hash = 'dup-hash'", [], |row| row.get(0))
            .expect("count scripts");
        assert_eq!(remaining, 1);

        let merged = connection
            .query_row(
                "SELECT id, description, interpreter, created_at, last_used, use_count, search_text FROM scripts WHERE hash = ?1",
                params!["dup-hash"],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .expect("merged row");
        assert_eq!(merged.1.as_deref(), Some("new desc"));
        assert_eq!(merged.2.as_deref(), Some("python3"));
        assert_eq!(merged.3, "2024-01-01T00:00:00+00:00");
        assert_eq!(merged.4.as_deref(), Some("2024-03-01T00:00:00+00:00"));
        assert_eq!(merged.5, 5);
        assert!(merged.6.contains("new desc"));
        assert!(merged.6.contains("alpha"));
        assert!(merged.6.contains("beta"));
        assert!(merged.6.contains("do_work"));
        assert!(merged.6.contains("json"));

        let tag_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM tags WHERE script_id = ?1", params![merged.0.clone()], |row| row.get(0))
            .expect("tag count");
        let usage_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM usage_history WHERE script_id = ?1", params![merged.0.clone()], |row| row.get(0))
            .expect("usage count");
        let function_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM functions WHERE script_id = ?1", params![merged.0.clone()], |row| row.get(0))
            .expect("function count");
        let surviving_ids = connection
            .prepare("SELECT id FROM scripts WHERE hash = 'dup-hash'")
            .expect("prepare ids")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query ids")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("collect ids");
        let fts_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM scripts_fts WHERE script_id = ?1", params![merged.0], |row| row.get(0))
            .expect("fts count");
        let alias_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM script_aliases", [], |row| row.get(0))
            .expect("alias count");
        let extra_term_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM script_search_terms", [], |row| row.get(0))
            .expect("extra term count");
        assert_eq!(tag_count, 2);
        assert_eq!(usage_count, 1);
        assert_eq!(function_count, 1);
        assert_eq!(surviving_ids.len(), 1);
        assert_eq!(fts_count, 1);
        assert_eq!(alias_count, 1);
        assert_eq!(extra_term_count, 0);
    }
}
