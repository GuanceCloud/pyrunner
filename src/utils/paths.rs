use anyhow::Result;
use chrono::{DateTime, Datelike, Utc};
use std::path::{Path, PathBuf};

pub fn script_storage_path(cache_dir: &Path, hash: &str, now: DateTime<Utc>) -> Result<PathBuf> {
    let month_dir = format!("{:04}-{:02}", now.year(), now.month());
    let file_name = format!("{}_{}.py", shorten_hash(hash), now.timestamp());
    Ok(cache_dir.join("scripts").join(month_dir).join(file_name))
}

pub fn script_identifier(hash: &str, now: DateTime<Utc>) -> String {
    format!("{}_{}", shorten_hash(hash), now.timestamp())
}

fn shorten_hash(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

#[cfg(test)]
mod tests {
    use super::{script_identifier, script_storage_path};
    use chrono::{TimeZone, Utc};
    use std::path::Path;

    #[test]
    fn storage_path_uses_year_month_and_short_hash() {
        let now = Utc
            .timestamp_opt(1773230401, 0)
            .single()
            .expect("valid timestamp");
        let path = script_storage_path(Path::new("/tmp/cache"), "abcdef1234567890", now)
            .expect("path should be created");

        assert_eq!(
            path.to_string_lossy(),
            "/tmp/cache/scripts/2026-03/abcdef123456_1773230401.py"
        );
    }

    #[test]
    fn identifier_uses_short_hash_and_timestamp() {
        let now = Utc
            .timestamp_opt(1773230401, 0)
            .single()
            .expect("valid timestamp");
        assert_eq!(
            script_identifier("abcdef1234567890", now),
            "abcdef123456_1773230401"
        );
    }
}
