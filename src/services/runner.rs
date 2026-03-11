use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use std::fs::{self, File};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::config::Config;
use crate::db::Database;
use crate::models::result::RunResult;

pub struct RunnerService {
    config: Config,
}

impl RunnerService {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn run(&self, database: &Database, script_id: &str, args: &[String]) -> Result<RunResult> {
        let resolved_id = database
            .resolve_script_id(script_id)?
            .ok_or_else(|| anyhow::anyhow!("script not found: {script_id}"))?;
        let (script_path, interpreter) = load_run_target(database, &resolved_id)?
            .ok_or_else(|| anyhow::anyhow!("script not found: {script_id}"))?;
        let mut command = vec![
            interpreter.unwrap_or_else(|| self.config.default_interpreter.clone()),
            script_path,
        ];
        command.extend(args.iter().cloned());
        let started_at = Utc::now();
        let start = Instant::now();
        let output = run_command_with_timeout(&command, self.config.default_timeout_secs)?;
        let duration_ms = start.elapsed().as_millis() as i64;

        let connection = database.connection();
        connection.execute(
            "UPDATE scripts SET last_used = ?1, use_count = use_count + 1 WHERE id = ?2",
            params![started_at.to_rfc3339(), &resolved_id],
        )?;
        connection.execute(
            "INSERT INTO usage_history (script_id, used_at, context, exit_code, duration_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                &resolved_id,
                started_at.to_rfc3339(),
                "run",
                output.exit_code,
                duration_ms
            ],
        )?;

        Ok(RunResult {
            script_id: resolved_id,
            command,
            exit_code: output.exit_code,
            stdout: output.stdout,
            stderr: output.stderr,
            success: output.success,
            duration_ms,
        })
    }
}

struct CommandOutput {
    exit_code: i32,
    stdout: String,
    stderr: String,
    success: bool,
}

fn load_run_target(
    database: &Database,
    script_id: &str,
) -> Result<Option<(String, Option<String>)>> {
    let connection = database.connection();
    let script = connection
        .query_row(
            "SELECT path, interpreter FROM scripts WHERE id = ?1",
            params![script_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()?;

    Ok(script)
}

fn run_command_with_timeout(command: &[String], timeout_secs: u64) -> Result<CommandOutput> {
    let stdout_path = temp_output_path("stdout");
    let stderr_path = temp_output_path("stderr");
    let stdout_file = File::options()
        .create_new(true)
        .write(true)
        .open(&stdout_path)?;
    let stderr_file = File::options()
        .create_new(true)
        .write(true)
        .open(&stderr_path)?;

    let mut child_command = Command::new(&command[0]);
    child_command
        .args(&command[1..])
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));
    #[cfg(unix)]
    unsafe {
        child_command.pre_exec(|| {
            if libc::setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }

    let mut child = child_command.spawn()?;

    let timeout = Duration::from_secs(timeout_secs);
    let started_at = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }

        if start_timed_out(timeout, started_at, &mut child, &mut timed_out)? {
            break child.wait()?;
        }

        thread::sleep(Duration::from_millis(25));
    };

    let stdout = read_output_file(&stdout_path)?;
    let mut stderr = read_output_file(&stderr_path)?;
    let _ = fs::remove_file(&stdout_path);
    let _ = fs::remove_file(&stderr_path);

    if timed_out {
        if !stderr.is_empty() && !stderr.ends_with('\n') {
            stderr.push('\n');
        }
        stderr.push_str(&format!("process timed out after {timeout_secs}s"));
    }

    Ok(CommandOutput {
        exit_code: if timed_out {
            -1
        } else {
            status.code().unwrap_or(-1)
        },
        stdout,
        stderr,
        success: status.success() && !timed_out,
    })
}

fn start_timed_out(
    timeout: Duration,
    started_at: Instant,
    child: &mut std::process::Child,
    timed_out: &mut bool,
) -> Result<bool> {
    if timeout == Duration::ZERO {
        *timed_out = true;
        terminate_child(child)?;
        return Ok(true);
    }

    if started_at.elapsed() >= timeout {
        *timed_out = true;
        terminate_child(child)?;
        return Ok(true);
    }

    Ok(false)
}

fn terminate_child(child: &mut std::process::Child) -> Result<()> {
    #[cfg(unix)]
    {
        let pgid = child.id() as i32;
        let kill_result = unsafe { libc::killpg(pgid, libc::SIGKILL) };
        if kill_result == 0 {
            return Ok(());
        }

        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(());
        }

        return Err(error.into());
    }

    #[cfg(not(unix))]
    {
        child.kill()?;
        Ok(())
    }
}

fn read_output_file(path: &std::path::Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

fn temp_output_path(kind: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("pyrunner-{kind}-{nanos}.log"))
}
