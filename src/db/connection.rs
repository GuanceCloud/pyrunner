use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::db::migrations;

pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn new(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let connection = Connection::open(path)?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        migrations::run(&connection)?;

        Ok(Self { connection })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn resolve_script_id(&self, script_id: &str) -> Result<Option<String>> {
        let mut current = script_id.to_string();
        let mut seen = HashSet::new();

        loop {
            if !seen.insert(current.clone()) {
                return Ok(None);
            }

            let direct = self
                .connection
                .query_row(
                    "SELECT id FROM scripts WHERE id = ?1",
                    params![&current],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if direct.is_some() {
                return Ok(direct);
            }

            let alias = self
                .connection
                .query_row(
                    "SELECT script_id FROM script_aliases WHERE alias_id = ?1",
                    params![&current],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            match alias {
                Some(next) => current = next,
                None => return Ok(None),
            }
        }
    }
}
