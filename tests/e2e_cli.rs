use rusqlite::{params, Connection};
use serde_json::Value;
use std::process::Command;
use std::{
    env, fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn help_command_works() {
    let output = Command::new(env!("CARGO_BIN_EXE_pyrunner"))
        .arg("--help")
        .output()
        .expect("failed to run pyrunner");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8");
    assert!(stdout.contains("Local Python script cache CLI"));
}

#[test]
fn register_search_run_flow_works() {
    let env_ctx = TestEnv::new("register-search-run");
    let script_path = env_ctx.write_script(
        "demo.py",
        "import sys\n\nif __name__ == '__main__':\n    arg = sys.argv[1] if len(sys.argv) > 1 else 'world'\n    print(f'hello:{arg}')\n",
    );

    let register = env_ctx.run_json(&[
        "register",
        script_path.to_string_lossy().as_ref(),
        "--desc",
        "demo run script",
        "--tags",
        "demo,run",
    ]);
    let script_id = register["script_id"]
        .as_str()
        .expect("script_id")
        .to_string();

    let search = env_ctx.run_text(&["search", "demo run"]);
    assert!(search.contains(&script_id));

    let run = env_ctx.run_json(&["run", &script_id, "--", "codex"]);
    assert_eq!(run["exit_code"].as_i64(), Some(0));
    assert_eq!(run["success"].as_bool(), Some(true));
    assert!(run["stdout"]
        .as_str()
        .expect("stdout")
        .contains("hello:codex"));
}

#[test]
fn duplicate_register_returns_same_script_id() {
    let env_ctx = TestEnv::new("duplicate-register");
    let script_path = env_ctx.write_script("demo.py", "print('same')\n");

    let first = env_ctx.run_json(&[
        "register",
        script_path.to_string_lossy().as_ref(),
        "--desc",
        "same script",
    ]);
    let second = env_ctx.run_json(&[
        "register",
        script_path.to_string_lossy().as_ref(),
        "--desc",
        "same script again",
    ]);

    assert_eq!(first["script_id"], second["script_id"]);
}

#[test]
fn database_enforces_unique_hash_for_registered_scripts() {
    let env_ctx = TestEnv::new("unique-hash");
    let script_path = env_ctx.write_script("demo.py", "print('same')\n");

    let register = env_ctx.run_json(&[
        "register",
        script_path.to_string_lossy().as_ref(),
        "--desc",
        "same script",
    ]);
    let script_id = register["script_id"].as_str().expect("script_id");
    let db = Connection::open(env_ctx.home_dir.join(".pyrunner").join("metadata.db")).expect("open db");
    let hash: String = db
        .query_row(
            "SELECT hash FROM scripts WHERE id = ?1",
            params![script_id],
            |row| row.get(0),
        )
        .expect("load hash");

    let insert_result = db.execute(
        "INSERT INTO scripts (id, path, hash, description, language, entrypoint, interpreter, created_at, input_types, output_types, parameters, search_text) VALUES (?1, ?2, ?3, ?4, 'python', '__main__', 'python3', ?5, '[]', '[]', '{}', '')",
        params![
            "manual_duplicate",
            env_ctx.workspace_dir.join("manual-duplicate.py").to_string_lossy().to_string(),
            hash,
            "duplicate",
            "2026-03-11T00:00:00+00:00"
        ],
    );

    assert!(insert_result.is_err(), "duplicate hash insert should fail");
}

#[test]
fn search_and_list_handle_large_result_sets() {
    let env_ctx = TestEnv::new("large-result-sets");
    let _ = env_ctx.run_json(&["stats"]);
    let db = Connection::open(env_ctx.home_dir.join(".pyrunner").join("metadata.db")).expect("open db");

    for index in 0..1100 {
        let script_id = format!("bulk_{index:04}");
        let path = env_ctx.workspace_dir.join(format!("{script_id}.py"));
        db.execute(
            "INSERT INTO scripts (id, path, hash, description, language, entrypoint, interpreter, created_at, input_types, output_types, parameters, search_text) VALUES (?1, ?2, ?3, ?4, 'python', '__main__', 'python3', ?5, '[]', '[]', '{}', ?6)",
            params![
                script_id,
                path.to_string_lossy().to_string(),
                format!("hash-{index:04}"),
                format!("bulk script {index}"),
                format!("2026-03-11T00:{:02}:00+00:00", index % 60),
                format!("bulk script {index} bulk-tag")
            ],
        )
        .expect("insert script");
        db.execute(
            "INSERT INTO scripts_fts (script_id, search_text) VALUES (?1, ?2)",
            params![format!("bulk_{index:04}"), format!("bulk script {index} bulk-tag")],
        )
        .expect("insert fts");
        db.execute(
            "INSERT INTO tags (script_id, tag) VALUES (?1, ?2)",
            params![format!("bulk_{index:04}"), "bulk-tag"],
        )
        .expect("insert tag");
    }

    let search = env_ctx.run_json(&[
        "search",
        "bulk",
        "--top-k",
        "1000",
        "--threshold",
        "0.0",
        "--json",
    ]);
    assert_eq!(search.as_array().expect("results").len(), 1000);

    let list = env_ctx.run_text(&["list", "--tags", "bulk-tag", "--limit", "1000"]);
    assert_eq!(list.matches("\n   path: ").count(), 1000);
    assert!(list.contains("bulk_1099"));
}

#[test]
fn migrated_alias_ids_continue_to_work() {
    let env_ctx = TestEnv::new("migrated-alias");
    let config_dir = env_ctx.home_dir.join(".pyrunner");
    fs::create_dir_all(&config_dir).expect("create config dir");
    let db = Connection::open(config_dir.join("metadata.db")).expect("open db");
    db.execute_batch(
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
        CREATE VIRTUAL TABLE scripts_fts USING fts5(script_id UNINDEXED, search_text);
        "#,
    )
    .expect("seed old schema");

    let script_path = env_ctx.write_script("legacy.py", "print('legacy')\n");
    let survivor_path = env_ctx.write_script("legacy-copy.py", "print('legacy')\n");
    db.execute(
        "INSERT INTO scripts (id, path, hash, description, interpreter, created_at, last_used, use_count, search_text) VALUES (?1, ?2, ?3, 'old desc', 'python3', ?4, NULL, 1, 'old desc')",
        params!["old-id", script_path.to_string_lossy().to_string(), "dup-hash", "2024-01-01T00:00:00+00:00"],
    )
    .expect("insert old");
    db.execute(
        "INSERT INTO scripts (id, path, hash, description, interpreter, created_at, last_used, use_count, search_text) VALUES (?1, ?2, ?3, 'new desc', 'python3', ?4, ?5, 3, 'new desc')",
        params!["new-id", survivor_path.to_string_lossy().to_string(), "dup-hash", "2024-02-01T00:00:00+00:00", "2024-03-01T00:00:00+00:00"],
    )
    .expect("insert new");
    db.execute("INSERT INTO scripts_fts (script_id, search_text) VALUES (?1, ?2)", params!["old-id", "old desc"]).expect("fts old");
    db.execute("INSERT INTO scripts_fts (script_id, search_text) VALUES (?1, ?2)", params!["new-id", "new desc"]).expect("fts new");

    let get = env_ctx.run_json(&["get", "old-id"]);
    assert_eq!(get["script_id"].as_str(), Some("new-id"));

    let run = env_ctx.run_json(&["run", "old-id"]);
    assert_eq!(run["script_id"].as_str(), Some("new-id"));
    assert_eq!(run["exit_code"].as_i64(), Some(0));

    let delete = env_ctx.run_json(&["delete", "old-id", "--yes"]);
    assert_eq!(delete["script_id"].as_str(), Some("new-id"));
}

#[test]
fn list_handles_large_tag_filters() {
    let env_ctx = TestEnv::new("large-tag-filter");
    let script_path = env_ctx.write_script("tagged.py", "print('tagged')\n");
    let register = env_ctx.run_json(&[
        "register",
        script_path.to_string_lossy().as_ref(),
        "--desc",
        "many tags script",
        "--tags",
        "tag-0000,tag-0001",
    ]);
    let script_id = register["script_id"].as_str().expect("script_id").to_string();

    let db = Connection::open(env_ctx.home_dir.join(".pyrunner").join("metadata.db")).expect("open db");
    for index in 2..1100 {
        db.execute(
            "INSERT OR IGNORE INTO tags (script_id, tag) VALUES (?1, ?2)",
            params![&script_id, format!("tag-{index:04}")],
        )
        .expect("insert extra tag");
    }

    let filter = (0..1100)
        .map(|index| format!("tag-{index:04}"))
        .collect::<Vec<_>>()
        .join(",");
    let list = env_ctx.run_text(&["list", "--tags", &filter, "--limit", "10"]);
    assert!(list.contains(&script_id));
}

#[test]
fn migrated_extra_description_terms_survive_updates() {
    let env_ctx = TestEnv::new("migrated-search-terms");
    let config_dir = env_ctx.home_dir.join(".pyrunner");
    fs::create_dir_all(&config_dir).expect("create config dir");
    let db = Connection::open(config_dir.join("metadata.db")).expect("open db");
    db.execute_batch(
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
        CREATE VIRTUAL TABLE scripts_fts USING fts5(script_id UNINDEXED, search_text);
        "#,
    )
    .expect("seed old schema");

    let survivor_path = env_ctx.write_script("survivor.py", "print('legacy')\n");
    let other_path = env_ctx.write_script("other.py", "print('legacy')\n");
    db.execute(
        "INSERT INTO scripts (id, path, hash, description, interpreter, created_at, last_used, use_count, search_text) VALUES (?1, ?2, ?3, 'alpha phrase', 'python3', ?4, NULL, 1, 'alpha phrase')",
        params!["old-id", other_path.to_string_lossy().to_string(), "dup-hash", "2024-01-01T00:00:00+00:00"],
    )
    .expect("insert old");
    db.execute(
        "INSERT INTO scripts (id, path, hash, description, interpreter, created_at, last_used, use_count, search_text) VALUES (?1, ?2, ?3, 'beta phrase', 'python3', ?4, ?5, 3, 'beta phrase')",
        params!["new-id", survivor_path.to_string_lossy().to_string(), "dup-hash", "2024-02-01T00:00:00+00:00", "2024-03-01T00:00:00+00:00"],
    )
    .expect("insert new");
    db.execute("INSERT INTO scripts_fts (script_id, search_text) VALUES (?1, ?2)", params!["old-id", "alpha phrase"]).expect("fts old");
    db.execute("INSERT INTO scripts_fts (script_id, search_text) VALUES (?1, ?2)", params!["new-id", "beta phrase"]).expect("fts new");

    let update = env_ctx.run_json(&["update", "new-id", "--tags", "migrated"]);
    assert_eq!(update["script_id"].as_str(), Some("new-id"));

    let search_old = env_ctx.run_json(&["search", "alpha phrase", "--threshold", "0.0", "--json"]);
    let search_new = env_ctx.run_json(&["search", "beta phrase", "--threshold", "0.0", "--json"]);
    assert_eq!(search_old[0]["script_id"].as_str(), Some("new-id"));
    assert_eq!(search_new[0]["script_id"].as_str(), Some("new-id"));
}

#[test]
fn alias_resolution_follows_multiple_hops() {
    let env_ctx = TestEnv::new("alias-chain");
    let config_dir = env_ctx.home_dir.join(".pyrunner");
    fs::create_dir_all(&config_dir).expect("create config dir");
    let db = Connection::open(config_dir.join("metadata.db")).expect("open db");
    db.execute_batch(
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
        CREATE VIRTUAL TABLE scripts_fts USING fts5(script_id UNINDEXED, search_text);
        "#,
    )
    .expect("seed old schema");

    let survivor_path = env_ctx.write_script("chain.py", "print('chain')\n");
    let merged_path = env_ctx.write_script("merged.py", "print('chain')\n");
    db.execute(
        "INSERT INTO scripts (id, path, hash, description, interpreter, created_at, last_used, use_count, search_text) VALUES (?1, ?2, ?3, 'mid desc', 'python3', ?4, NULL, 1, 'mid desc')",
        params!["mid-id", merged_path.to_string_lossy().to_string(), "dup-hash", "2024-01-01T00:00:00+00:00"],
    )
    .expect("insert mid");
    db.execute(
        "INSERT INTO scripts (id, path, hash, description, interpreter, created_at, last_used, use_count, search_text) VALUES (?1, ?2, ?3, 'new desc', 'python3', ?4, ?5, 3, 'new desc')",
        params!["new-id", survivor_path.to_string_lossy().to_string(), "dup-hash", "2024-02-01T00:00:00+00:00", "2024-03-01T00:00:00+00:00"],
    )
    .expect("insert new");
    db.execute(
        "INSERT INTO script_aliases (alias_id, script_id) VALUES (?1, ?2)",
        params!["old-id", "mid-id"],
    )
    .expect("insert alias");
    db.execute("INSERT INTO scripts_fts (script_id, search_text) VALUES (?1, ?2)", params!["mid-id", "mid desc"]).expect("fts mid");
    db.execute("INSERT INTO scripts_fts (script_id, search_text) VALUES (?1, ?2)", params!["new-id", "new desc"]).expect("fts new");

    let get = env_ctx.run_json(&["get", "old-id"]);
    assert_eq!(get["script_id"].as_str(), Some("new-id"));
}

#[test]
fn update_get_list_and_delete_flow_works() {
    let env_ctx = TestEnv::new("update-delete");
    let script_path = env_ctx.write_script("demo.py", "print('demo')\n");

    let register = env_ctx.run_json(&[
        "register",
        script_path.to_string_lossy().as_ref(),
        "--desc",
        "before update",
        "--tags",
        "demo,old",
    ]);
    let script_id = register["script_id"]
        .as_str()
        .expect("script_id")
        .to_string();

    let update = env_ctx.run_json(&[
        "update",
        &script_id,
        "--desc",
        "after update",
        "--tags",
        "demo,new",
    ]);
    assert_eq!(update["description"].as_str(), Some("after update"));

    let get = env_ctx.run_json(&["get", &script_id]);
    assert_eq!(get["description"].as_str(), Some("after update"));
    assert!(get["tags"]
        .as_array()
        .expect("tags")
        .iter()
        .any(|item| item == "new"));

    let list = env_ctx.run_text(&["list"]);
    assert!(list.contains("after update"));
    assert!(list.contains("tags: demo, new") || list.contains("tags: demo,new"));

    let delete_without_yes = env_ctx.run_failure(&["delete", &script_id]);
    assert!(delete_without_yes.contains("refusing delete without --yes"));

    let delete = env_ctx.run_json(&["delete", &script_id, "--yes"]);
    assert_eq!(delete["deleted"].as_bool(), Some(true));
    let list_after = env_ctx.run_text(&["list"]);
    assert!(list_after.contains("no scripts"));
}

#[test]
fn clean_and_stats_flow_works() {
    let env_ctx = TestEnv::new("clean-stats");
    let script_path = env_ctx.write_script("demo.py", "print('demo')\n");
    let register = env_ctx.run_json(&[
        "register",
        script_path.to_string_lossy().as_ref(),
        "--desc",
        "cleanup candidate",
        "--tags",
        "cleanup",
    ]);
    let script_id = register["script_id"]
        .as_str()
        .expect("script_id")
        .to_string();

    env_ctx.rewrite_created_at(&script_id, "2020-01-01T00:00:00+00:00");

    let dry_run = env_ctx.run_json(&["clean", "--older-than", "30", "--dry-run"]);
    let candidates = dry_run["candidates"].as_array().expect("candidates");
    assert_eq!(candidates.len(), 1);

    let stats_before = env_ctx.run_json(&["stats"]);
    assert_eq!(stats_before["total_scripts"].as_i64(), Some(1));

    let clean = env_ctx.run_json(&["clean", "--older-than", "30"]);
    assert_eq!(clean["deleted_count"].as_u64(), Some(1));

    let stats_after = env_ctx.run_json(&["stats"]);
    assert_eq!(stats_after["total_scripts"].as_i64(), Some(0));
}

#[test]
fn ai_flow_works() {
    let env_ctx = TestEnv::new("ai-flow");
    let register = env_ctx.run_json(&[
        "ai",
        "register",
        "--script-text",
        "import json\nprint(json.dumps({'ok': True}))\n",
        "--desc",
        "ai inline json script",
        "--tags",
        "ai,json",
    ]);
    let script_id = register["script_id"]
        .as_str()
        .expect("script_id")
        .to_string();

    let search = env_ctx.run_json(&[
        "ai",
        "search",
        "--query",
        "json ai script",
        "--threshold",
        "0.5",
    ]);
    assert_eq!(search["total"].as_u64(), Some(1));

    let check = env_ctx.run_json(&[
        "ai",
        "check",
        "--query",
        "json ai script",
        "--threshold",
        "0.5",
    ]);
    assert_eq!(check["exists"].as_bool(), Some(true));
    assert_eq!(check["script_id"].as_str(), Some(script_id.as_str()));

    let get = env_ctx.run_json(&["ai", "get", &script_id]);
    assert_eq!(get["script_id"].as_str(), Some(script_id.as_str()));
}

#[test]
fn ai_check_uses_registered_interpreter() {
    let env_ctx = TestEnv::new("ai-check-interpreter");
    env_ctx.write_config("python3", 1000, 60);

    let register = env_ctx.run_json(&[
        "ai",
        "register",
        "--script-text",
        "print('ok')\n",
        "--desc",
        "interpreter-sensitive script",
    ]);
    let script_id = register["script_id"]
        .as_str()
        .expect("script_id")
        .to_string();

    env_ctx.write_config("python-does-not-exist", 1000, 60);

    let check = env_ctx.run_json(&[
        "ai",
        "check",
        "--query",
        "interpreter-sensitive script",
        "--threshold",
        "0.5",
    ]);
    assert_eq!(check["script_id"].as_str(), Some(script_id.as_str()));
    assert_eq!(check["execute_command"][0].as_str(), Some("python3"));
}

#[test]
fn search_supports_hyphenated_queries() {
    let env_ctx = TestEnv::new("search-hyphen");
    let script_path = env_ctx.write_script("demo.py", "print('demo')\n");

    let register = env_ctx.run_json(&[
        "register",
        script_path.to_string_lossy().as_ref(),
        "--desc",
        "json demo script",
        "--tags",
        "json-demo",
    ]);
    let script_id = register["script_id"]
        .as_str()
        .expect("script_id")
        .to_string();

    let search = env_ctx.run_json(&["search", "json-demo", "--json"]);
    assert_eq!(search.as_array().expect("results").len(), 1);
    assert_eq!(search[0]["script_id"].as_str(), Some(script_id.as_str()));
}

#[test]
fn search_top_k_prefers_more_used_script_when_fts_score_ties() {
    let env_ctx = TestEnv::new("search-top-k");
    let mut boosted_script_id = String::new();

    for index in 1..=6 {
        let script_path = env_ctx.write_script(
            &format!("demo-{index}.py"),
            &format!("print('demo-{index}')\n"),
        );
        let register = env_ctx.run_json(&[
            "register",
            script_path.to_string_lossy().as_ref(),
            "--desc",
            "shared demo script",
        ]);
        let script_id = register["script_id"]
            .as_str()
            .expect("script_id")
            .to_string();

        if index == 6 {
            boosted_script_id = script_id.clone();
            for _ in 0..5 {
                let run = env_ctx.run_json(&["run", &script_id]);
                assert_eq!(run["success"].as_bool(), Some(true));
            }
        }
    }

    let search = env_ctx.run_json(&["search", "shared demo script", "--top-k", "1", "--json"]);
    assert_eq!(
        search[0]["script_id"].as_str(),
        Some(boosted_script_id.as_str())
    );
}

#[test]
fn update_can_clear_description_and_tags() {
    let env_ctx = TestEnv::new("update-clear");
    let script_path = env_ctx.write_script("demo.py", "print('demo')\n");

    let register = env_ctx.run_json(&[
        "register",
        script_path.to_string_lossy().as_ref(),
        "--desc",
        "to be cleared",
        "--tags",
        "demo,clear",
    ]);
    let script_id = register["script_id"]
        .as_str()
        .expect("script_id")
        .to_string();

    let update = env_ctx.run_json(&["update", &script_id, "--desc", "", "--tags", ""]);
    assert!(update["description"].is_null());
    assert_eq!(update["tags"].as_array().expect("tags").len(), 0);

    let get = env_ctx.run_json(&["get", &script_id]);
    assert!(get["description"].is_null());
    assert_eq!(get["tags"].as_array().expect("tags").len(), 0);
}

#[test]
fn missing_script_commands_fail_with_non_zero_exit() {
    let env_ctx = TestEnv::new("missing-script");

    let get = env_ctx.run_failure(&["get", "does-not-exist"]);
    assert!(get.contains("script not found: does-not-exist"));

    let update = env_ctx.run_failure(&["update", "does-not-exist", "--desc", "demo"]);
    assert!(update.contains("script not found: does-not-exist"));

    let delete = env_ctx.run_failure(&["delete", "does-not-exist", "--yes"]);
    assert!(delete.contains("script not found: does-not-exist"));

    let ai_get = env_ctx.run_failure(&["ai", "get", "does-not-exist"]);
    assert!(ai_get.contains("script not found: does-not-exist"));
}

#[test]
fn list_tag_filter_applies_before_limit() {
    let env_ctx = TestEnv::new("list-tags-before-limit");

    for index in 1..=3 {
        let script_path =
            env_ctx.write_script(&format!("demo-{index}.py"), &format!("print('{index}')\n"));
        let tags = if index == 1 { "target" } else { "other" };
        env_ctx.run_json(&[
            "register",
            script_path.to_string_lossy().as_ref(),
            "--desc",
            &format!("script {index}"),
            "--tags",
            tags,
        ]);
    }

    let list = env_ctx.run_text(&["list", "--tags", "target", "--limit", "1"]);
    assert!(list.contains("script 1"));
}

#[test]
fn run_enforces_configured_timeout() {
    let env_ctx = TestEnv::new("run-timeout");
    env_ctx.write_config("python3", 1000, 1);

    let script_path =
        env_ctx.write_script("slow.py", "import time\ntime.sleep(2)\nprint('done')\n");
    let register = env_ctx.run_json(&[
        "register",
        script_path.to_string_lossy().as_ref(),
        "--desc",
        "slow script",
    ]);
    let script_id = register["script_id"].as_str().expect("script_id");

    let started = SystemTime::now();
    let run = env_ctx.run_json(&["run", script_id]);
    let elapsed = SystemTime::now()
        .duration_since(started)
        .expect("duration")
        .as_secs_f64();

    assert_eq!(run["success"].as_bool(), Some(false));
    assert_eq!(run["exit_code"].as_i64(), Some(-1));
    assert!(run["stderr"]
        .as_str()
        .expect("stderr")
        .contains("process timed out after 1s"));
    assert!(
        elapsed < 2.0,
        "run should have timed out, elapsed={elapsed}"
    );
}

#[test]
fn run_timeout_kills_child_processes() {
    let env_ctx = TestEnv::new("run-timeout-process-tree");
    env_ctx.write_config("python3", 1000, 1);
    let marker_path = env_ctx.workspace_dir.join("child-finished.txt");
    let script_content = format!(
        "import subprocess, sys, time\nsubprocess.Popen([sys.executable, '-c', \"import pathlib,time; time.sleep(2); pathlib.Path(r'{}').write_text('done')\"])\ntime.sleep(5)\n",
        path_literal(&marker_path)
    );
    let script_path = env_ctx.write_script("spawn_child.py", &script_content);

    let register = env_ctx.run_json(&[
        "register",
        script_path.to_string_lossy().as_ref(),
        "--desc",
        "spawns child process",
    ]);
    let script_id = register["script_id"].as_str().expect("script_id");

    let run = env_ctx.run_json(&["run", script_id]);
    assert_eq!(run["success"].as_bool(), Some(false));
    thread::sleep(Duration::from_secs(3));
    assert!(
        !marker_path.exists(),
        "timed out child process should not outlive the runner"
    );
}

#[test]
fn run_handles_non_utf8_output() {
    let env_ctx = TestEnv::new("run-non-utf8");
    let script_path = env_ctx.write_script(
        "bytes.py",
        "import sys\nsys.stdout.buffer.write(b'\\xff\\n')\nsys.stderr.buffer.write(b'\\xfe\\n')\n",
    );
    let register = env_ctx.run_json(&[
        "register",
        script_path.to_string_lossy().as_ref(),
        "--desc",
        "binary output",
    ]);
    let script_id = register["script_id"].as_str().expect("script_id");

    let run = env_ctx.run_json(&["run", script_id]);
    assert_eq!(run["success"].as_bool(), Some(true));
    assert_eq!(run["exit_code"].as_i64(), Some(0));
    assert!(run["stdout"].as_str().expect("stdout").contains('\u{fffd}'));
    assert!(run["stderr"].as_str().expect("stderr").contains('\u{fffd}'));
}

#[test]
fn clean_older_than_uses_last_used_instead_of_created_at() {
    let env_ctx = TestEnv::new("clean-last-used");
    let script_path = env_ctx.write_script("demo.py", "print('demo')\n");
    let register = env_ctx.run_json(&[
        "register",
        script_path.to_string_lossy().as_ref(),
        "--desc",
        "cleanup candidate",
    ]);
    let script_id = register["script_id"]
        .as_str()
        .expect("script_id")
        .to_string();

    env_ctx.rewrite_usage_fields(
        &script_id,
        "2020-01-01T00:00:00+00:00",
        Some("2026-03-10T00:00:00+00:00"),
        5,
    );

    let dry_run = env_ctx.run_json(&["clean", "--older-than", "30", "--dry-run"]);
    let candidates = dry_run["candidates"].as_array().expect("candidates");
    assert!(candidates.is_empty());
}

#[test]
fn register_enforces_max_scripts_limit() {
    let env_ctx = TestEnv::new("register-max-scripts");
    env_ctx.write_config("python3", 2, 60);

    let first_script = env_ctx.write_script("first.py", "print('first')\n");
    let first = env_ctx.run_json(&[
        "register",
        first_script.to_string_lossy().as_ref(),
        "--desc",
        "first script",
    ]);
    let first_id = first["script_id"].as_str().expect("first script id").to_string();

    let second_script = env_ctx.write_script("second.py", "print('second')\n");
    env_ctx.run_json(&[
        "register",
        second_script.to_string_lossy().as_ref(),
        "--desc",
        "second script",
    ]);

    let third_script = env_ctx.write_script("third.py", "print('third')\n");
    env_ctx.run_json(&[
        "register",
        third_script.to_string_lossy().as_ref(),
        "--desc",
        "third script",
    ]);

    let stats = env_ctx.run_json(&["stats"]);
    assert_eq!(stats["total_scripts"].as_i64(), Some(2));

    let get_first = env_ctx.run_failure(&["get", &first_id]);
    assert!(get_first.contains(&format!("script not found: {first_id}")));
}

struct TestEnv {
    home_dir: PathBuf,
    workspace_dir: PathBuf,
}

impl TestEnv {
    fn new(prefix: &str) -> Self {
        let home_dir = unique_test_dir(&format!("{prefix}-home"));
        let workspace_dir = unique_test_dir(&format!("{prefix}-work"));
        fs::create_dir_all(&home_dir).expect("create home");
        fs::create_dir_all(&workspace_dir).expect("create workspace");

        Self {
            home_dir,
            workspace_dir,
        }
    }

    fn write_script(&self, name: &str, content: &str) -> PathBuf {
        let path = self.workspace_dir.join(name);
        fs::write(&path, content).expect("write script");
        path
    }

    fn write_config(&self, default_interpreter: &str, max_scripts: usize, timeout_secs: u64) {
        let config_dir = self.home_dir.join(".pyrunner");
        fs::create_dir_all(&config_dir).expect("create config dir");
        let cache_dir = config_dir.to_string_lossy();
        let log_file = config_dir.join("logs").join("pyrunner.log");
        let content = format!(
            "cache_dir = \"{cache_dir}\"\ndefault_interpreter = \"{default_interpreter}\"\nmax_scripts = {max_scripts}\nmax_age_days = 90\ndefault_timeout_secs = {timeout_secs}\n\n[logging]\nlevel = \"info\"\nfile = \"{}\"\n\n[matching]\nsimilarity_threshold = 0.85\nfts_weight = 0.5\ntag_weight = 0.2\nfunction_weight = 0.2\nusage_weight = 0.1\n",
            log_file.to_string_lossy()
        );
        fs::write(config_dir.join("config.toml"), content).expect("write config");
    }

    fn run_text(&self, args: &[&str]) -> String {
        let output = self.run_command(args);

        assert!(
            output.status.success(),
            "command failed: {:?}\nstdout={}\nstderr={}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("stdout utf8")
    }

    fn run_failure(&self, args: &[&str]) -> String {
        let output = self.run_command(args);

        assert!(
            !output.status.success(),
            "command unexpectedly succeeded: {:?}\nstdout={}\nstderr={}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stderr).expect("stderr utf8")
    }

    fn run_command(&self, args: &[&str]) -> std::process::Output {
        let output = Command::new(env!("CARGO_BIN_EXE_pyrunner"))
            .env("HOME", &self.home_dir)
            .args(args)
            .output()
            .expect("run pyrunner");
        output
    }

    fn run_json(&self, args: &[&str]) -> Value {
        let stdout = self.run_text(args);
        serde_json::from_str(&stdout).expect("stdout should be json")
    }

    fn rewrite_created_at(&self, script_id: &str, created_at: &str) {
        self.rewrite_usage_fields(script_id, created_at, None, 0);
    }

    fn rewrite_usage_fields(
        &self,
        script_id: &str,
        created_at: &str,
        last_used: Option<&str>,
        use_count: i64,
    ) {
        let db_path = self.home_dir.join(".pyrunner").join("metadata.db");
        let output = Command::new("python3")
            .arg("-c")
            .arg(
                "import sqlite3, sys; conn = sqlite3.connect(sys.argv[1]); conn.execute('update scripts set created_at=?, last_used=?, use_count=? where id=?', (sys.argv[2], None if sys.argv[3] == '__NONE__' else sys.argv[3], int(sys.argv[4]), sys.argv[5])); conn.commit()",
            )
            .arg(db_path)
            .arg(created_at)
            .arg(last_used.unwrap_or("__NONE__"))
            .arg(use_count.to_string())
            .arg(script_id)
            .output()
            .expect("rewrite sqlite date");
        assert!(output.status.success(), "python sqlite update failed");
    }
}

fn path_literal(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}

fn unique_test_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    env::temp_dir().join(format!("{prefix}-{nanos}"))
}
