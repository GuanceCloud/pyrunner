use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};

use crate::cli::output::{print_json, print_script_infos, print_search_results};
use crate::models::result::{CheckResponse, RegisterResponse};
use crate::AppContext;

#[derive(Debug, Parser)]
#[clap(name = "pyrunner")]
#[clap(about = "Local Python script cache CLI for AI agents")]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Register a Python script into the local cache
    Register {
        script_path: String,
        #[clap(short, long)]
        desc: Option<String>,
        #[clap(short, long)]
        tags: Option<String>,
    },
    /// Search cached scripts by natural-language query
    Search {
        query: String,
        #[clap(short = 'k', long, default_value_t = 5)]
        top_k: usize,
        #[clap(short, long, default_value_t = 0.5)]
        threshold: f64,
        #[clap(short, long)]
        json: bool,
    },
    /// Get a cached script by script_id
    Get { script_id: String },
    /// List cached scripts
    List {
        #[clap(short, long)]
        tags: Option<String>,
        #[clap(short, long, default_value_t = 20)]
        limit: usize,
    },
    /// Update script description or tags
    Update {
        script_id: String,
        #[clap(short, long)]
        desc: Option<String>,
        #[clap(short, long)]
        tags: Option<String>,
    },
    /// Delete a cached script and its metadata
    Delete {
        script_id: String,
        #[clap(long)]
        yes: bool,
    },
    /// Execute a cached script as a local debugging helper
    Run {
        script_id: String,
        #[clap(last = true)]
        args: Vec<String>,
    },
    /// Show cache usage statistics
    Stats,
    /// Preview or remove stale cached scripts
    Clean {
        #[clap(long)]
        older_than: Option<u32>,
        #[clap(long)]
        unused: bool,
        #[clap(long)]
        dry_run: bool,
    },
    /// AI-oriented structured commands
    Ai {
        #[clap(subcommand)]
        command: AiCommands,
    },
}

#[derive(Debug, Subcommand)]
pub enum AiCommands {
    /// Search scripts and return JSON results for agents
    Search {
        #[clap(long)]
        query: String,
        #[clap(short = 'k', long, default_value_t = 5)]
        top_k: usize,
        #[clap(short, long, default_value_t = 0.8)]
        threshold: f64,
    },
    /// Check whether a reusable script already exists
    Check {
        #[clap(long)]
        query: String,
        #[clap(short, long, default_value_t = 0.85)]
        threshold: f64,
    },
    /// Get script metadata and path in JSON form
    Get { script_id: String },
    /// Register script content from file, stdin, or inline text
    Register {
        #[clap(long)]
        script_file: Option<String>,
        #[clap(long)]
        stdin: bool,
        #[clap(long)]
        script_text: Option<String>,
        #[clap(short, long)]
        desc: String,
        #[clap(short, long)]
        tags: Option<String>,
    },
}

pub fn execute(command: Commands, context: &AppContext) -> Result<()> {
    match command {
        Commands::Register {
            script_path,
            desc,
            tags,
        } => {
            let response = context.services.register.register_file(
                &context.database,
                &script_path,
                desc,
                tags_to_vec(tags),
            )?;
            print_json(&RegisterResponse::from(response))
        }
        Commands::Search {
            query,
            top_k,
            threshold,
            json,
        } => {
            let results =
                context
                    .services
                    .search
                    .search(&context.database, &query, top_k, threshold)?;
            if json {
                print_json(&results)
            } else {
                print_search_results(&results)
            }
        }
        Commands::Get { script_id } => {
            let result = require_found(
                context.services.search.get(&context.database, &script_id)?,
                "script",
                &script_id,
            )?;
            print_json(&result)
        }
        Commands::List { tags, limit } => {
            let result = context.services.search.list(
                &context.database,
                Some(tags_to_vec(tags)).filter(|items| !items.is_empty()),
                limit,
            )?;
            print_script_infos(&result)
        }
        Commands::Update {
            script_id,
            desc,
            tags,
        } => {
            let result = require_found(
                context.services.register.update_metadata(
                    &context.database,
                    &script_id,
                    desc,
                    tags.map(|value| tags_to_vec(Some(value))),
                )?,
                "script",
                &script_id,
            )?;
            print_json(&result)
        }
        Commands::Delete { script_id, yes } => {
            if !yes {
                Err(anyhow!("refusing delete without --yes"))
            } else {
                let result = require_found(
                    context
                        .services
                        .cleanup
                        .delete_script(&context.database, &script_id)?,
                    "script",
                    &script_id,
                )?;
                print_json(&result)
            }
        }
        Commands::Run { script_id, args } => {
            let result = context
                .services
                .runner
                .run(&context.database, &script_id, &args)?;
            print_json(&result)
        }
        Commands::Stats => {
            let stats = context.services.cleanup.stats(&context.database)?;
            print_json(&stats)
        }
        Commands::Clean {
            older_than,
            unused,
            dry_run,
        } => {
            let result =
                context
                    .services
                    .cleanup
                    .clean(&context.database, older_than, unused, dry_run)?;
            print_json(&result)
        }
        Commands::Ai { command } => execute_ai(command, context),
    }
}

fn execute_ai(command: AiCommands, context: &AppContext) -> Result<()> {
    match command {
        AiCommands::Search {
            query,
            top_k,
            threshold,
        } => {
            let results =
                context
                    .services
                    .search
                    .search(&context.database, &query, top_k, threshold)?;
            let payload = serde_json::json!({
                "results": results,
                "total": results.len(),
            });
            print_json(&payload)
        }
        AiCommands::Check { query, threshold } => {
            let response = context
                .services
                .search
                .check(&context.database, &query, threshold)?;
            print_json(&CheckResponse::from(response))
        }
        AiCommands::Get { script_id } => {
            let result = require_found(
                context.services.search.get(&context.database, &script_id)?,
                "script",
                &script_id,
            )?;
            print_json(&result)
        }
        AiCommands::Register {
            script_file,
            stdin,
            script_text,
            desc,
            tags,
        } => {
            let response = context.services.register.register_source(
                &context.database,
                script_file,
                stdin,
                script_text,
                desc,
                tags_to_vec(tags),
            )?;
            print_json(&RegisterResponse::from(response))
        }
    }
}

fn tags_to_vec(tags: Option<String>) -> Vec<String> {
    tags.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn require_found<T>(value: Option<T>, resource_name: &str, resource_id: &str) -> Result<T> {
    value.ok_or_else(|| anyhow!("{resource_name} not found: {resource_id}"))
}
