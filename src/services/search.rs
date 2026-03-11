use anyhow::Result;
use rusqlite::{params, params_from_iter, OptionalExtension};
use std::collections::{HashMap, HashSet};

use crate::config::Config;
use crate::db::Database;
use crate::models::result::{MatchDetails, ScriptCheck, ScriptInfo, SearchResult};

const SQLITE_PARAM_CHUNK_SIZE: usize = 900;

pub struct SearchService {
    config: Config,
}

impl SearchService {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn search(
        &self,
        database: &Database,
        query: &str,
        top_k: usize,
        threshold: f64,
    ) -> Result<Vec<SearchResult>> {
        let normalized_query = build_match_query(query);
        if normalized_query.is_empty() {
            return Ok(Vec::new());
        }

        let candidates = load_candidates(database, &normalized_query, top_k * 5)?;
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let max_raw = candidates
            .iter()
            .map(|candidate| candidate.raw_fts_score)
            .fold(f64::MIN, f64::max);
        let min_raw = candidates
            .iter()
            .map(|candidate| candidate.raw_fts_score)
            .fold(f64::MAX, f64::min);
        let max_use_count = candidates
            .iter()
            .map(|candidate| candidate.use_count)
            .max()
            .unwrap_or(0);
        let query_terms = tokenize(query);
        let query_set: HashSet<&str> = query_terms.iter().map(String::as_str).collect();

        let candidate_ids = candidates
            .iter()
            .map(|candidate| candidate.script_id.clone())
            .collect::<Vec<_>>();
        let tag_map = load_string_map(
            database,
            "SELECT script_id, tag FROM tags WHERE script_id IN ({placeholders}) ORDER BY tag ASC",
            &candidate_ids,
        )?;
        let function_map = load_string_map(
            database,
            "SELECT script_id, name FROM functions WHERE script_id IN ({placeholders}) ORDER BY name ASC",
            &candidate_ids,
        )?;

        let mut results = Vec::new();
        for candidate in candidates {
            let candidate_id = candidate.script_id.clone();
            let fts_score = normalize_fts(candidate.raw_fts_score, min_raw, max_raw);
            let script_tags = tag_map.get(&candidate_id).cloned().unwrap_or_default();
            let function_names = function_map.get(&candidate_id).cloned().unwrap_or_default();
            let tag_score = overlap_score(&query_set, &script_tags);
            let function_score = overlap_score(&query_set, &function_names);
            let usage_score = normalize_usage(candidate.use_count, max_use_count);
            let score = self.config.matching.fts_weight * fts_score
                + self.config.matching.tag_weight * tag_score
                + self.config.matching.function_weight * function_score
                + self.config.matching.usage_weight * usage_score;

            if score < threshold {
                continue;
            }

            results.push(SearchResult {
                script_id: candidate_id,
                path: candidate.path,
                score,
                description: candidate.description,
                match_details: MatchDetails {
                    fts_score,
                    tag_score,
                    function_score,
                    usage_score,
                },
            });
        }

        results.sort_by(|left, right| right.score.total_cmp(&left.score));
        results.truncate(top_k);
        Ok(results)
    }

    pub fn check(&self, database: &Database, query: &str, threshold: f64) -> Result<ScriptCheck> {
        let first = self
            .search(database, query, 1, threshold)?
            .into_iter()
            .next();
        Ok(match first {
            Some(result) if result.score >= threshold => {
                let interpreter = load_interpreter(database, &result.script_id)?
                    .unwrap_or_else(|| self.config.default_interpreter.clone());
                ScriptCheck {
                    exists: true,
                    script_id: Some(result.script_id),
                    path: Some(result.path.clone()),
                    score: Some(result.score),
                    action: Some("reuse".to_string()),
                    execute_command: Some(vec![interpreter, result.path]),
                }
            }
            _ => ScriptCheck {
                exists: false,
                script_id: None,
                path: None,
                score: None,
                action: Some("create".to_string()),
                execute_command: None,
            },
        })
    }

    pub fn get(&self, database: &Database, script_id: &str) -> Result<Option<ScriptInfo>> {
        let resolved_id = match database.resolve_script_id(script_id)? {
            Some(value) => value,
            None => return Ok(None),
        };
        let connection = database.connection();
        let script = connection
            .query_row(
                "SELECT id, path, description FROM scripts WHERE id = ?1",
                params![resolved_id],
                |row| {
                    Ok(ScriptInfo {
                        script_id: row.get(0)?,
                        path: row.get(1)?,
                        description: row.get(2)?,
                        tags: Vec::new(),
                    })
                },
            )
            .optional()?;

        let mut script = match script {
            Some(script) => script,
            None => return Ok(None),
        };

        let script_id = script.script_id.clone();
        script.tags = load_tags(database, script_id)?;
        Ok(Some(script))
    }

    pub fn list(
        &self,
        database: &Database,
        tags: Option<Vec<String>>,
        limit: usize,
    ) -> Result<Vec<ScriptInfo>> {
        let connection = database.connection();
        let mut results = if let Some(filter_tags) = tags {
            load_list_rows_with_tag_filter(connection, &filter_tags, limit)?
        } else {
            load_list_rows(connection, limit)?
        };

        let script_ids = results
            .iter()
            .map(|item| item.script_id.clone())
            .collect::<Vec<_>>();
        let tag_map = load_string_map(
            database,
            "SELECT script_id, tag FROM tags WHERE script_id IN ({placeholders}) ORDER BY tag ASC",
            &script_ids,
        )?;
        for item in &mut results {
            item.tags = tag_map.get(&item.script_id).cloned().unwrap_or_default();
        }

        Ok(results)
    }
}

struct CandidateRow {
    script_id: String,
    path: String,
    description: Option<String>,
    use_count: i64,
    raw_fts_score: f64,
}

fn build_match_query(query: &str) -> String {
    tokenize(query)
        .into_iter()
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokenize(input: &str) -> Vec<String> {
    input
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.to_lowercase())
        .collect()
}

fn load_candidates(database: &Database, query: &str, limit: usize) -> Result<Vec<CandidateRow>> {
    let connection = database.connection();
    let mut statement = connection.prepare(
        "SELECT s.id, s.path, s.description, s.use_count, -bm25(scripts_fts) AS score
         FROM scripts_fts
         JOIN scripts s ON s.id = scripts_fts.script_id
         WHERE scripts_fts MATCH ?1
         ORDER BY score DESC, s.use_count DESC, s.created_at DESC
         LIMIT ?2",
    )?;
    let rows = statement.query_map(params![query, limit as i64], |row| {
        Ok(CandidateRow {
            script_id: row.get(0)?,
            path: row.get(1)?,
            description: row.get(2)?,
            use_count: row.get(3)?,
            raw_fts_score: row.get(4)?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

fn load_interpreter(database: &Database, script_id: &str) -> Result<Option<String>> {
    let connection = database.connection();
    let interpreter = connection
        .query_row(
            "SELECT interpreter FROM scripts WHERE id = ?1",
            params![script_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    Ok(interpreter)
}

fn load_tags(database: &Database, script_id: String) -> Result<Vec<String>> {
    load_string_column(
        database,
        "SELECT tag FROM tags WHERE script_id = ?1 ORDER BY tag ASC",
        script_id,
    )
}

fn load_string_column(database: &Database, sql: &str, script_id: String) -> Result<Vec<String>> {
    let connection = database.connection();
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params![script_id], |row| row.get::<_, String>(0))?;

    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }
    Ok(values)
}

fn load_list_rows(connection: &rusqlite::Connection, limit: usize) -> Result<Vec<ScriptInfo>> {
    let mut statement =
        connection.prepare("SELECT id, path, description FROM scripts ORDER BY created_at DESC LIMIT ?1")?;
    let rows = statement.query_map(params![limit as i64], |row| {
        Ok(ScriptInfo {
            script_id: row.get(0)?,
            path: row.get(1)?,
            description: row.get(2)?,
            tags: Vec::new(),
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

fn load_list_rows_with_tag_filter(
    connection: &rusqlite::Connection,
    filter_tags: &[String],
    limit: usize,
) -> Result<Vec<ScriptInfo>> {
    if filter_tags.is_empty() {
        return Ok(Vec::new());
    }

    let mut merged = HashMap::new();
    for chunk in filter_tags.chunks(SQLITE_PARAM_CHUNK_SIZE) {
        let placeholders = placeholders(chunk.len());
        let sql = format!(
            "SELECT DISTINCT s.id, s.path, s.description, s.created_at
             FROM scripts s
             JOIN tags t ON t.script_id = s.id
             WHERE t.tag IN ({placeholders})
             ORDER BY s.created_at DESC"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(chunk.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                ScriptInfo {
                    script_id: row.get(0)?,
                    path: row.get(1)?,
                    description: row.get(2)?,
                    tags: Vec::new(),
                },
                row.get::<_, String>(3)?,
            ))
        })?;

        for row in rows {
            let (script_id, item, created_at) = row?;
            merged.entry(script_id).or_insert((item, created_at));
        }
    }

    let mut results = merged.into_values().collect::<Vec<_>>();
    results.sort_by(|left, right| right.1.cmp(&left.1));
    results.truncate(limit);
    Ok(results.into_iter().map(|(item, _)| item).collect())
}

fn load_string_map(
    database: &Database,
    sql_template: &str,
    script_ids: &[String],
) -> Result<HashMap<String, Vec<String>>> {
    if script_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let connection = database.connection();
    let mut values = HashMap::new();
    for chunk in script_ids.chunks(SQLITE_PARAM_CHUNK_SIZE) {
        let sql = sql_template.replace("{placeholders}", &placeholders(chunk.len()));
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(chunk.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (script_id, value) = row?;
            values.entry(script_id).or_insert_with(Vec::new).push(value);
        }
    }
    Ok(values)
}

fn placeholders(count: usize) -> String {
    std::iter::repeat("?")
        .take(count)
        .collect::<Vec<_>>()
        .join(", ")
}

fn overlap_score(query_terms: &HashSet<&str>, values: &[String]) -> f64 {
    if query_terms.is_empty() || values.is_empty() {
        return 0.0;
    }

    let value_terms: HashSet<String> = values.iter().flat_map(|value| tokenize(value)).collect();
    if value_terms.is_empty() {
        return 0.0;
    }

    let matched = query_terms
        .iter()
        .filter(|term| value_terms.contains(**term))
        .count();
    matched as f64 / query_terms.len() as f64
}

fn normalize_fts(raw_score: f64, min_raw: f64, max_raw: f64) -> f64 {
    if (max_raw - min_raw).abs() < f64::EPSILON {
        1.0
    } else {
        (raw_score - min_raw) / (max_raw - min_raw)
    }
}

fn normalize_usage(use_count: i64, max_use_count: i64) -> f64 {
    if use_count <= 0 || max_use_count <= 0 {
        return 0.0;
    }

    let numerator = (1.0 + use_count as f64).ln();
    let denominator = (1.0 + max_use_count as f64).ln();
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

#[cfg(test)]
mod tests {
    use super::{build_match_query, normalize_fts, normalize_usage, overlap_score, tokenize};
    use std::collections::HashSet;

    #[test]
    fn tokenize_and_build_match_query_lowercase_words() {
        assert_eq!(
            tokenize("Hello, JSON-demo_world!"),
            vec!["hello".to_string(), "json-demo_world".to_string()]
        );
        assert_eq!(build_match_query("Hello, JSON"), "\"hello\" \"json\"");
    }

    #[test]
    fn build_match_query_quotes_hyphenated_terms() {
        assert_eq!(build_match_query("json-demo"), "\"json-demo\"");
    }

    #[test]
    fn overlap_score_measures_query_term_matches() {
        let query_terms = vec!["demo".to_string(), "json".to_string(), "run".to_string()];
        let query_set: HashSet<&str> = query_terms.iter().map(String::as_str).collect();
        let score = overlap_score(
            &query_set,
            &vec!["demo".to_string(), "json tool".to_string()],
        );

        assert!((score - 0.6666666).abs() < 0.01);
    }

    #[test]
    fn normalize_fts_handles_single_and_range() {
        assert_eq!(normalize_fts(3.0, 3.0, 3.0), 1.0);
        assert!((normalize_fts(5.0, 1.0, 9.0) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn normalize_usage_handles_zero_and_scale() {
        assert_eq!(normalize_usage(0, 10), 0.0);
        assert_eq!(normalize_usage(1, 0), 0.0);
        let score = normalize_usage(10, 10);
        assert!((score - 1.0).abs() < f64::EPSILON);
    }
}
