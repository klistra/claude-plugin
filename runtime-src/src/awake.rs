//! `klistra wait`: keeps a Claude Code session awake while it is connected to a chat in the
//! studio. The Claude desktop app stops a session's process a minute or two after its last turn,
//! and a stopped session hears nothing the person writes; a command the session started in the
//! background keeps it running (a hook does not). This is that command: Claude starts it when the
//! MCP server says how, after it connects to a chat. It asks klistra now and then whether the
//! session is still connected and its computer keeps it awake, and ends when not, with a line
//! for Claude to read, which says there is nothing to do.

use crate::{config, http};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// How often klistra is asked whether the session is still connected (KLISTRA_AWAKE_EVERY_MS
/// for the studio's checks, which cannot wait that long).
fn every() -> Duration {
    std::env::var("KLISTRA_AWAKE_EVERY_MS").ok().and_then(|ms| ms.parse().ok()).map_or(Duration::from_secs(20), Duration::from_millis)
}
/// A wait that has not said it is there for this long is gone.
const STALE: Duration = Duration::from_secs(60);
/// klistra out of reach this long, the session is let sleep.
const UNREACHED_MOST: Duration = Duration::from_secs(30 * 60);

/// Whether the session's app stops it when it is idle, so that it needs keeping awake: the
/// desktop app does; a terminal keeps its session running for as long as it is open.
pub fn sleeps_when_idle() -> bool {
    std::env::var("CLAUDE_CODE_ENTRYPOINT").is_ok_and(|e| e == "claude-desktop")
}

/// The file a wait for this session touches while it runs.
fn mark(session: &str) -> PathBuf {
    config::dir().join(format!("awake-{}", &config::hash(session)[..16]))
}

/// Whether a wait keeps this session awake now.
pub fn kept(session: &str) -> bool {
    std::fs::metadata(mark(session)).and_then(|m| m.modified()).ok().and_then(|at| SystemTime::now().duration_since(at).ok()).is_some_and(|age| age < STALE)
}

/// A session's name as klistra takes it.
pub fn is_session_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 100 && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:".contains(c))
}

/// The command that keeps `session` awake, to run from its Bash, if this program's path can be
/// written as one: in double quotes, with forward slashes (Claude Code's Bash on Windows is Git Bash).
pub fn command(session: &str) -> Option<String> {
    let exe = std::env::current_exe().ok()?.to_string_lossy().replace('\\', "/");
    if exe.is_empty() || exe.contains(['"', '$', '`', '\n']) || !is_session_name(session) {
        return None;
    }
    let base = config::base();
    let url = if base == config::DEFAULT_URL {
        String::new()
    } else if base.chars().all(|c| c.is_ascii_alphanumeric() || ":/.-_".contains(c)) {
        format!(" --url {base}")
    } else {
        return None;
    };
    Some(format!("\"{exe}\" wait --session {session}{url}"))
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// `klistra wait [--session <id>] [--url <klistra>]`: runs while the session is connected to a chat.
pub fn wait(args: &[String]) -> i32 {
    if let Some(url) = option(args, "--url") {
        std::env::set_var("KLISTRA_URL", url);
    }
    let session = option(args, "--session").or_else(|| std::env::var("CLAUDE_CODE_SESSION_ID").ok()).unwrap_or_default();
    if !is_session_name(&session) {
        eprintln!("Which Claude Code session? Run it from one, or give --session <id>.");
        return 1;
    }
    if kept(&session) {
        println!("This session is kept awake already. Nothing to do.");
        return 0;
    }
    let base = config::base();
    if config::token(&base).is_none() {
        println!("This computer is not connected to klistra, so this session need not be kept awake. Nothing to do.");
        return 0;
    }
    if let Err(e) = config::ensure_dir() {
        eprintln!("{e}");
        return 1;
    }
    http::send_always(vec![("x-klistra-session".to_string(), session.clone())]);
    let file = mark(&session);
    let mut unreached: Option<Instant> = None;
    let every = every();
    let why = loop {
        let _ = std::fs::write(&file, std::process::id().to_string());
        let Some(token) = config::token(&base) else { break "This computer is not connected to klistra any more" };
        match http::send("GET", &format!("{base}/api/agent/inbox?awake=1"), Some(&token), None, Duration::from_secs(30)) {
            Ok(reply) if reply.status == 200 => {
                unreached = None;
                if reply.body["keepAwake"] == false {
                    break "Keeping Claude Code awake was turned off for this computer in klistra";
                }
                if reply.body["connected"].as_u64() == Some(0) {
                    break "This session is connected to no chat in klistra any more";
                }
            }
            Ok(reply) if matches!(reply.status, 401 | 403) => break "This computer can no longer work in klistra's projects",
            Ok(reply) if reply.status == 404 => break "This klistra does not keep sessions awake",
            _ => {
                if unreached.get_or_insert_with(Instant::now).elapsed() > UNREACHED_MOST {
                    break "klistra has not answered for half an hour";
                }
            }
        }
        std::thread::sleep(every);
    };
    let _ = std::fs::remove_file(&file);
    println!("{why}, so this session no longer needs keeping awake. Nothing to do.");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_a_session_as_klistra_takes_it() {
        assert!(is_session_name("0b9c2f6e-1a2b-4c3d-9e8f-001122334455"));
        assert!(!is_session_name(""));
        assert!(!is_session_name("a b"));
        assert!(!is_session_name("x;rm"));
    }

    #[test]
    fn the_command_runs_this_program_for_the_session() {
        let command = command("s-1").unwrap();
        assert!(command.starts_with('"'), "{command}");
        assert!(command.contains("\" wait --session s-1"), "{command}");
        assert!(!command.contains('\\'), "{command}");
        assert_eq!(super::command("bad name"), None);
    }

    #[test]
    fn a_session_unmarked_is_not_kept() {
        assert!(!kept("klistra-test-never-marked"));
    }
}
