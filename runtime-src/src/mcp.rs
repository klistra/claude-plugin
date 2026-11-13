//! `klistra mcp`: the plugin's MCP server over stdio. Every request goes on to klistra's own MCP
//! server, over HTTPS with this computer's token, and its answer comes back as it was: the tools
//! are all klistra's, so a new one needs nothing new here. The token is read for each call, so a
//! computer connected while Claude Code runs is used at once.

use crate::{config, http, log};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const NOT_CONNECTED: &str = "This computer is not connected to klistra. Run /klistra:login to connect it.";
/// How long klistra holds one request open while it waits for a build; wait_for_build asks again
/// until its own time is up.
const WAIT_STEP_SECONDS: u64 = 540;
const WAIT_DEFAULT_SECONDS: u64 = 1800;
const WAIT_MOST_SECONDS: u64 = 3600;
/// How long answers still on their way are waited for once Claude Code has closed the connection.
const LAST_ANSWERS: Duration = Duration::from_secs(30);

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
        let (output, underway) = (output.clone(), underway.clone());
        underway.start();
        // each on its own, as Claude Code sends the next while a long one runs
        std::thread::spawn(move || {
            output.send(&answer(&message));
            underway.end();
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
}
