//! This Claude Code session's inbox (cross-session messaging): a socket, or a named pipe on
//! Windows, that Claude Code gives the processes it starts. A message posted there starts a turn
//! when the session is idle, or reaches Claude between tool calls when it is busy. The person's
//! messages in the studio's chat come in this way while the session is connected to a chat.

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

const POST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq)]
pub struct Inbox {
    pub socket: String,
    /// Proves the post comes from the session's own process; Windows requires it.
    pub token: Option<String>,
}

fn claude_home() -> PathBuf {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default().join(".claude")
}

/// Whether a settings file turns inbound messages away (`crossSessionInbound` other than
/// "accept"): a post held or refused would be lost without a word, so the inbox isn't used then,
/// and the messages come with Claude's next klistra tool call instead.
fn inbound_turned_away(claude_home: &Path, project: Option<&Path>) -> bool {
    let mut files = vec![claude_home.join("settings.json")];
    if let Some(project) = project {
        files.push(project.join(".claude").join("settings.json"));
        files.push(project.join(".claude").join("settings.local.json"));
    }
    files.iter().any(|file| {
        let Ok(text) = std::fs::read_to_string(file) else { return false };
        let Ok(settings) = serde_json::from_str::<Value>(&text) else { return false };
        !matches!(&settings["crossSessionInbound"], Value::Null) && settings["crossSessionInbound"] != "accept"
    })
}

fn inbox_from(socket: Option<String>, token: Option<String>, claude_home: &Path, project: Option<&Path>) -> Option<Inbox> {
    let socket = socket.filter(|s| !s.is_empty())?;
    if inbound_turned_away(claude_home, project) {
        return None;
    }
    Some(Inbox { socket, token: token.filter(|t| !t.is_empty()) })
}

/// The inbox of the session this process belongs to, if Claude Code gave it one and it takes messages.
pub fn of_session() -> Option<Inbox> {
    let var = |name| std::env::var(name).ok();
    let project = std::env::current_dir().ok();
    inbox_from(var("CLAUDE_CODE_MESSAGING_SOCKET"), var("CLAUDE_CODE_MESSAGING_TOKEN"), &claude_home(), project.as_deref())
}

/// The lines Claude Code reads: an auth line, then a user message.
fn lines(inbox: &Inbox, text: &str) -> String {
    let mut out = String::new();
    if let Some(token) = &inbox.token {
        out.push_str(&json!({ "type": "auth", "token": token }).to_string());
        out.push('\n');
    }
    out.push_str(&json!({ "type": "user", "message": { "role": "user", "content": text } }).to_string());
    out.push('\n');
    out
}

#[cfg(windows)]
fn deliver(socket: &str, payload: &[u8]) -> std::io::Result<()> {
    let mut pipe = std::fs::OpenOptions::new().read(true).write(true).open(socket)?;
    // in one go: Claude Code closes a connection that dawdles
    pipe.write_all(payload)?;
    // until Claude Code has read it all: closing sooner could drop what is still in the pipe
    pipe.sync_all()
}

#[cfg(unix)]
fn deliver(socket: &str, payload: &[u8]) -> std::io::Result<()> {
    use std::io::Read;
    use std::os::unix::net::UnixStream;
    let mut stream = UnixStream::connect(socket)?;
    stream.set_write_timeout(Some(POST_TIMEOUT))?;
    stream.set_read_timeout(Some(POST_TIMEOUT))?;
    stream.write_all(payload)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    // Claude Code closes its end once it has the message
    let mut rest = Vec::new();
    stream.read_to_end(&mut rest)?;
    Ok(())
}

/// Posts one message into the inbox, giving up after a few seconds.
pub fn post(inbox: &Inbox, text: &str) -> Result<(), String> {
    let socket = inbox.socket.clone();
    let payload = lines(inbox, text).into_bytes();
    let (done, result) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(deliver(&socket, &payload));
    });
    match result.recv_timeout(POST_TIMEOUT) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("the session's inbox did not answer".into()),
    }
}

/// `klistra inbox <text>`: puts a message into the Claude Code session it is run from, as the
/// person's messages from the studio are put in, to check that this works on this computer.
pub fn post_here(text: &str) -> i32 {
    let Some(inbox) = of_session() else {
        eprintln!("Not run from a Claude Code session that takes messages: there is no CLAUDE_CODE_MESSAGING_SOCKET, or crossSessionInbound turns them away.");
        return 1;
    };
    match post(&inbox, text) {
        Ok(()) => {
            println!("Put into this session's inbox.");
            0
        }
        Err(e) => {
            eprintln!("Could not put it into this session's inbox: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folders() -> (PathBuf, PathBuf) {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("klistra-inbox-{}-{nanos}", std::process::id()));
        let (config, project) = (root.join("claude"), root.join("project"));
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(project.join(".claude")).unwrap();
        (config, project)
    }

    fn inbox(config: &Path, project: &Path) -> Option<Inbox> {
        inbox_from(Some("/tmp/cc/x.sock".into()), Some("tok".into()), config, Some(project))
    }

    #[test]
    fn is_the_socket_and_token_claude_code_gives() {
        let (config, project) = folders();
        assert_eq!(inbox(&config, &project), Some(Inbox { socket: "/tmp/cc/x.sock".into(), token: Some("tok".into()) }));
        assert_eq!(inbox_from(None, None, &config, Some(&project)), None);
    }

    #[test]
    fn is_left_alone_when_settings_hold_or_refuse_inbound_messages() {
        let (config, project) = folders();
        std::fs::write(config.join("settings.json"), r#"{"crossSessionInbound":"hold"}"#).unwrap();
        assert_eq!(inbox(&config, &project), None);
        let (config, project) = folders();
        std::fs::write(project.join(".claude").join("settings.local.json"), r#"{"crossSessionInbound":"refuse"}"#).unwrap();
        assert_eq!(inbox(&config, &project), None);
        let (config, project) = folders();
        std::fs::write(config.join("settings.json"), r#"{"crossSessionInbound":"accept"}"#).unwrap();
        assert!(inbox(&config, &project).is_some());
    }

    #[test]
    fn posts_an_auth_line_then_the_message() {
        let text = lines(&Inbox { socket: String::new(), token: Some("t".into()) }, "hi");
        let parsed: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(parsed[0], json!({ "type": "auth", "token": "t" }));
        assert_eq!(parsed[1], json!({ "type": "user", "message": { "role": "user", "content": "hi" } }));
    }

    #[cfg(unix)]
    #[test]
    fn reaches_a_socket_that_reads_it() {
        use std::io::Read;
        use std::os::unix::net::UnixListener;
        // a socket's path has to be short: the temporary folder's own may be too long already
        let socket = PathBuf::from("/tmp").join(format!("klistra-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).unwrap();
        let reader = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut got = String::new();
            stream.read_to_string(&mut got).unwrap();
            got
        });
        post(&Inbox { socket: socket.display().to_string(), token: None }, "hello").unwrap();
        let got = reader.join().unwrap();
        let _ = std::fs::remove_file(&socket);
        assert!(got.contains(r#""content":"hello""#), "{got}");
    }
}
