//! `klistra mcp`: the plugin's MCP server over stdio. Every request goes on to klistra's own MCP
//! server, over HTTPS with this computer's token, and its answer comes back as it was: the tools
//! are all klistra's, so a new one needs nothing new here. The token is read for each call, so a
//! computer connected while Claude Code runs is used at once. While the session is connected to a
//! chat in the studio, it waits there for the person's messages and puts each into the session.

use crate::{awake, config, http, inbox, log};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

const NOT_CONNECTED: &str = "This computer is not connected to klistra. Run /klistra:login to connect it.";
/// How long klistra holds one request open while it waits for a build; wait_for_build asks again
/// until its own time is up.
const WAIT_STEP_SECONDS: u64 = 540;
const WAIT_DEFAULT_SECONDS: u64 = 1800;
const WAIT_MOST_SECONDS: u64 = 3600;
/// How long answers still on their way are waited for once Claude Code has closed the connection.
const LAST_ANSWERS: Duration = Duration::from_secs(30);
/// How long klistra holds one request for the person's messages open, at most.
const LISTEN_SECONDS: u64 = 25;

#[derive(Clone)]
struct Output(Arc<Mutex<std::io::Stdout>>);

impl Output {
    fn send(&self, message: &Value) {
        let mut out = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writeln!(out, "{message}");
        let _ = out.flush();
    }
}

fn failure(id: &Value, method: &str, text: String) -> Value {
    // a tool's failure is the model's to read and act on; anything else is the protocol's
    if method == "tools/call" {
        json!({ "jsonrpc": "2.0", "id": id, "result": { "content": [{ "type": "text", "text": text }], "isError": true } })
    } else {
        json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": text } })
    }
}

/// One request to klistra, answered as JSON-RPC under the same id.
fn forward(message: &Value, timeout: Duration) -> Value {
    let id = message["id"].clone();
    let method = message["method"].as_str().unwrap_or("");
    let base = config::base();
    let token = config::token(&base);
    match http::send("POST", &format!("{base}/api/agent/mcp"), token.as_deref(), Some(message), timeout) {
        Ok(reply) if reply.status == 200 && reply.body.is_object() => {
            let mut body = reply.body;
            body["id"] = id;
            body
        }
        Ok(reply) if reply.status == 401 => failure(&id, method, reply.error().unwrap_or_else(|| NOT_CONNECTED.to_string())),
        Ok(reply) => failure(&id, method, reply.error().unwrap_or_else(|| format!("klistra answered {}. Try again in a moment.", reply.status))),
        Err(e) => failure(&id, method, e),
    }
}

fn text_of(answer: &Value) -> String {
    answer["result"]["content"].as_array().map(|c| c.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default()
}

/// wait_for_build for as long as it was asked, up to an hour: klistra waits at most nine minutes a
/// request, so it is asked again while the build goes on. Claude Code moves a call this long to
/// the background, so the session goes on meanwhile.
fn wait_for_build(message: &Value) -> Value {
    let asked = message["params"]["arguments"]["seconds"].as_u64().unwrap_or(WAIT_DEFAULT_SECONDS).clamp(5, WAIT_MOST_SECONDS);
    let until = Instant::now() + Duration::from_secs(asked);
    loop {
        let left = until.saturating_duration_since(Instant::now()).as_secs().clamp(5, WAIT_STEP_SECONDS);
        let mut step = message.clone();
        step["params"]["arguments"]["seconds"] = json!(left);
        let answer = forward(&step, Duration::from_secs(left + 60));
        let still = answer["result"]["isError"] != true && text_of(&answer).starts_with("Still building");
        if !still || Instant::now() + Duration::from_secs(5) >= until {
            return answer;
        }
    }
}

fn answer(message: &Value) -> Value {
    let method = message["method"].as_str().unwrap_or("");
    if method == "tools/call" && message["params"]["name"] == "wait_for_build" {
        return wait_for_build(message);
    }
    // a tool may take a while (taking in a long video, a website); the rest is quick
    forward(message, Duration::from_secs(if method == "tools/call" { 600 } else { 60 }))
}

/// The name this session goes by at klistra: Claude Code's id for it, or one of its own.
fn session_name() -> String {
    std::env::var("CLAUDE_CODE_SESSION_ID")
        .ok()
        .filter(|id| awake::is_session_name(id))
        .unwrap_or_else(|| config::hash(&format!("{:?}{}", Instant::now(), std::process::id()))[..32].to_string())
}

static SESSION: OnceLock<String> = OnceLock::new();
/// Whether Claude was told how to keep this session awake since the server started.
static TOLD_AWAKE: AtomicBool = AtomicBool::new(false);

/// A tool's answer, with how to keep the session awake when klistra says it is connected to a
/// chat and its computer keeps it awake meanwhile, in an app that lets an idle session sleep, and
/// nothing keeps it awake yet: told on connecting to a chat, and once after the server starts (a
/// session the app stopped and started again is still connected, but no longer kept awake).
fn with_awake(message: &Value, answer: Value) -> Value {
    awake_told(message, answer, awake::sleeps_when_idle(), SESSION.get().map(String::as_str))
}

fn awake_told(message: &Value, mut answer: Value, sleeps: bool, session: Option<&str>) -> Value {
    let result = &answer["result"];
    if result["_meta"]["klistra/keepAwake"] != true || result["isError"] == true || !sleeps {
        return answer;
    }
    if message["params"]["name"] != "connect_chat" && TOLD_AWAKE.load(Ordering::SeqCst) {
        return answer;
    }
    let Some(session) = session else { return answer };
    if awake::kept(session) {
        return answer;
    }
    let Some(command) = awake::command(session) else { return answer };
    if let Some(content) = answer["result"]["content"].as_array_mut() {
        TOLD_AWAKE.store(true, Ordering::SeqCst);
        content.push(json!({ "type": "text", "text": format!("Keep this session awake while it is connected, so that what the person writes in the studio reaches you even when you are idle: start this once with Bash, run_in_background:\n\n{command}\n\nIt ends by itself when the session leaves its chats; there is nothing to do then.") }));
    }
    answer
}

/// Whether the person's messages are being waited for at klistra.
static LISTENING: AtomicBool = AtomicBool::new(false);
/// Calls of a tool answered so far: one may have connected the session to a chat since the
/// waiting last asked, so the waiting stops only when none came meanwhile.
static CALLED: AtomicU64 = AtomicU64::new(0);

/// Waits at klistra for what the person says to this session in the studio's chat, and puts it
/// into the session: from the first call of a tool (connecting to a chat is one) until the
/// session is connected to no chat. Started again by the next call when it stopped.
fn listen(into: inbox::Inbox) {
    if LISTENING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let base = config::base();
        let mut pause = Duration::from_secs(2);
        loop {
            let Some(token) = config::token(&base) else { break };
            let calls = CALLED.load(Ordering::SeqCst);
            let asked = http::send("GET", &format!("{base}/api/agent/inbox?wait={LISTEN_SECONDS}"), Some(&token), None, Duration::from_secs(LISTEN_SECONDS + 20));
            let reply = match asked {
                Ok(reply) if reply.status == 200 => reply,
                // not connected, no longer allowed, or a klistra without chats to wait in: nothing to wait for until a tool is called again
                Ok(reply) if matches!(reply.status, 401 | 403 | 404) => break,
                failed => {
                    if let Err(e) = failed {
                        log(&e);
                    }
                    std::thread::sleep(pause);
                    pause = (pause * 2).min(Duration::from_secs(60));
                    continue;
                }
            };
            pause = Duration::from_secs(2);
            let messages = reply.body["messages"].as_array().cloned().unwrap_or_default();
            let mut given = Vec::new();
            for message in &messages {
                let Some(text) = message["text"].as_str() else { continue };
                match inbox::post(&into, text) {
                    Ok(()) => given.push(json!({ "project": message["project"], "chat": message["chat"], "id": message["id"] })),
                    // left for klistra to give with Claude's next call of a tool
                    Err(e) => log(&format!("could not put a message from klistra into the session: {e}")),
                }
            }
            if !given.is_empty() {
                let _ = http::send("POST", &format!("{base}/api/agent/inbox"), Some(&token), Some(&json!({ "delivered": given })), Duration::from_secs(20));
            }
            if given.len() < messages.len() {
                std::thread::sleep(Duration::from_secs(10));
            }
            // connected to no chat, and no tool called since it asked: nothing to wait for until one is
            if messages.is_empty() && reply.body["connected"].as_u64() == Some(0) && CALLED.load(Ordering::SeqCst) == calls {
                LISTENING.store(false, Ordering::SeqCst);
                // a call answered just then found it still waiting and started nothing: it waits on for it, unless another has started
                if CALLED.load(Ordering::SeqCst) == calls || LISTENING.swap(true, Ordering::SeqCst) {
                    return;
                }
            }
        }
        LISTENING.store(false, Ordering::SeqCst);
    });
}

/// Requests being answered, so that the last answers go out before the server ends.
#[derive(Clone, Default)]
struct Underway(Arc<(Mutex<usize>, Condvar)>);

impl Underway {
    fn start(&self) {
        *self.0 .0.lock().unwrap_or_else(|e| e.into_inner()) += 1;
    }
    fn end(&self) {
        *self.0 .0.lock().unwrap_or_else(|e| e.into_inner()) -= 1;
        self.0 .1.notify_all();
    }
    /// Waits until none is left, or `most` has passed.
    fn wait(&self, most: Duration) {
        let count = self.0 .0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self.0 .1.wait_timeout_while(count, most, |n| *n > 0);
    }
}

pub fn run() {
    let output = Output(Arc::new(Mutex::new(std::io::stdout())));
    let underway = Underway::default();
    // klistra knows this session by a name, to give it the messages of the chat it connects to;
    // it waits a little before giving them with a tool's result when they can come in here
    let into = inbox::of_session();
    let session = SESSION.get_or_init(session_name).clone();
    let mut headers = vec![("x-klistra-session".to_string(), session)];
    if into.is_some() {
        headers.push(("x-klistra-inbox".to_string(), "1".to_string()));
    }
    http::send_always(headers);
    // a session taken up again (a desktop session woken, claude --resume) may be connected to a
    // chat already, with messages waiting: asked at once, not only once a tool is called
    if let Some(into) = into.clone() {
        listen(into);
    }
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        // a byte order mark, as some Windows programs write first, is no part of the message
        let line = line.trim_start_matches('\u{feff}');
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            log(&format!("not JSON, left unanswered: {}", line.chars().take(200).collect::<String>()));
            continue;
        };
        // notifications, and answers to requests klistra never makes, need nothing from klistra
        if message.get("id").is_none() || message.get("method").is_none() {
            continue;
        }
        let (output, underway, into) = (output.clone(), underway.clone(), into.clone());
        underway.start();
        // each on its own, as Claude Code sends the next while a long one runs
        std::thread::spawn(move || {
            let called = message["method"] == "tools/call";
            let answered = answer(&message);
            output.send(&if called { with_awake(&message, answered) } else { answered });
            underway.end();
            if called {
                CALLED.fetch_add(1, Ordering::SeqCst);
                if let Some(into) = into {
                    listen(into);
                }
            }
        });
    }
    log("Claude Code closed the connection");
    underway.wait(LAST_ANSWERS);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_failure_is_the_models_and_the_rest_the_protocols() {
        let tool = failure(&json!(7), "tools/call", "no".into());
        assert_eq!(tool["id"], 7);
        assert_eq!(tool["result"]["isError"], true);
        assert_eq!(text_of(&tool), "no");
        let other = failure(&json!("a"), "tools/list", "no".into());
        assert_eq!(other["error"]["message"], "no");
        assert!(other.get("result").is_none());
    }

    #[test]
    fn says_how_to_keep_awake_only_when_klistra_asks_it() {
        let connect = json!({ "id": 1, "method": "tools/call", "params": { "name": "connect_chat" } });
        let other = json!({ "id": 2, "method": "tools/call", "params": { "name": "reply" } });
        let plain = json!({ "id": 1, "result": { "content": [{ "type": "text", "text": "Connected." }] } });
        let asked = json!({ "id": 1, "result": { "content": [{ "type": "text", "text": "Connected." }], "_meta": { "klistra/keepAwake": true } } });
        let failed = json!({ "id": 1, "result": { "content": [{ "type": "text", "text": "No." }], "isError": true, "_meta": { "klistra/keepAwake": true } } });
        let session = Some("klistra-test-session");
        assert_eq!(awake_told(&connect, plain.clone(), true, session), plain, "not unless klistra asks");
        assert_eq!(awake_told(&connect, failed.clone(), true, session), failed, "not on a failure");
        assert_eq!(awake_told(&connect, asked.clone(), false, session), asked, "not where the session stays awake anyway");
        let told = awake_told(&connect, asked.clone(), true, session);
        let text = text_of(&told);
        assert!(text.starts_with("Connected.\nKeep this session awake"), "{text}");
        assert!(text.contains("\" wait --session klistra-test-session\n"), "{text}");
        assert_eq!(awake_told(&other, asked.clone(), true, session), asked, "told once, then on connecting again only");
        assert_eq!(text_of(&awake_told(&connect, asked.clone(), true, session)), text);
    }
}
