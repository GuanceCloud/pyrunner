use anyhow::Result;
use serde::Serialize;

use crate::models::result::{ScriptInfo, SearchResult};

pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub fn print_message(message: &str) -> Result<()> {
    println!("{message}");
    Ok(())
}

pub fn print_search_results(results: &[SearchResult]) -> Result<()> {
    if results.is_empty() {
        return print_message("no results");
    }

    for (index, item) in results.iter().enumerate() {
        println!("{}. {}  score={:.3}", index + 1, item.script_id, item.score);
        println!("   path: {}", item.path);
        if let Some(description) = &item.description {
            println!("   desc: {}", description);
        }
        println!(
            "   match: fts={:.3} tag={:.3} fn={:.3} usage={:.3}",
            item.match_details.fts_score,
            item.match_details.tag_score,
            item.match_details.function_score,
            item.match_details.usage_score
        );
    }

    Ok(())
}

pub fn print_script_infos(items: &[ScriptInfo]) -> Result<()> {
    if items.is_empty() {
        return print_message("no scripts");
    }

    for (index, item) in items.iter().enumerate() {
        println!("{}. {}", index + 1, item.script_id);
        println!("   path: {}", item.path);
        if let Some(description) = &item.description {
            println!("   desc: {}", description);
        }
        if !item.tags.is_empty() {
            println!("   tags: {}", item.tags.join(", "));
        }
    }

    Ok(())
}
