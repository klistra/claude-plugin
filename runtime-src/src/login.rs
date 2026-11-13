//! `klistra login`, `klistra status`, `klistra logout`: what the plugin's /klistra:login and
//! /klistra:logout run, and the setup prompt. Output is for a person or Claude to read.

use crate::{config, http};
use serde_json::{json, Value};
use std::time::Duration;

const WAITING_STEP: Duration = Duration::from_secs(3);

/// What klistra says of this computer: its status, and the answer.
fn me(base: &str, token: &str) -> Result<http::Reply, String> {
    http::send("GET", &format!("{base}/api/agent/me"), Some(token), None, Duration::from_secs(20))
}

fn connected(answer: &Value, setup: bool) -> i32 {
    let team = answer["team"]["name"].as_str().unwrap_or("the team");
    let email = answer["user"]["email"].as_str().unwrap_or("its person");
    println!("Connected: Claude Code here can plan in {team} for {email}.");
    if setup {
        println!("If this Claude Code session doesn't have klistra's tools yet, run /reload-plugins or restart it.");
    }
    0
}

/// Asks every 3 seconds whether the computer was allowed, `times` times at most: Some(exit code)
/// once it is answered, None if it is still waiting after that.
fn wait_for(base: &str, token: &str, times: u32, setup: bool) -> Option<i32> {
    for _ in 0..times {
        match me(base, token) {
            Ok(reply) if reply.status == 200 => return Some(connected(&reply.body, setup)),
            Ok(reply) if reply.body["waiting"] == true => {}
            Ok(reply) => {
                println!("Not connected: {}", reply.error().unwrap_or_else(|| format!("klistra answered {}", reply.status)));
                println!("{}", if setup { "Run the setup again for a new link." } else { "Run klistra login again for a new link." });
                return Some(1);
            }
            // a moment without the network is no answer: keep asking
            Err(e) => crate::log(&e),
        }
        std::thread::sleep(WAITING_STEP);
    }
    None
}

fn computer() -> String {
    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("scutil").args(["--get", "ComputerName"]).stderr(std::process::Stdio::null()).output() {
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if out.status.success() && !name.is_empty() {
            return name;
        }
    }
    #[cfg(windows)]
    let name = std::env::var("COMPUTERNAME").ok();
    #[cfg(unix)]
    let name = {
        extern "C" {
            fn gethostname(name: *mut std::ffi::c_char, len: usize) -> i32;
        }
        let mut buffer = [0u8; 256];
        // SAFETY: the buffer outlives the call, and its length goes with it.
        let ok = unsafe { gethostname(buffer.as_mut_ptr().cast(), buffer.len()) } == 0;
        let end = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
        ok.then(|| String::from_utf8_lossy(&buffer[..end]).to_string())
    };
    name.map(|n| n.split('.').next().unwrap_or_default().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "A computer".to_string())
}

/// `klistra login`: a link and a code for the person to allow this computer with. `--wait` waits
/// for that up to 100 seconds; `--setup` does both, waiting as long as the link works.
pub fn login(mode: Option<&str>) -> i32 {
    let base = config::base();
    let token = match config::token_or_new(&base) {
        Ok(token) => token,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let setup = mode == Some("--setup");
    if mode == Some("--wait") {
        return wait_for(&base, &token, 34, false).unwrap_or_else(|| {
            println!("Still waiting for the link to be opened and the computer allowed. Run klistra login --wait again.");
            2
        });
    }
    match me(&base, &token) {
        Ok(reply) if reply.status == 200 => return connected(&reply.body, setup),
        Ok(_) => {}
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    }
    let computer = computer();
    let asked = http::send(
        "POST",
        &format!("{base}/api/agent/login"),
        None,
        Some(&json!({ "tokenHash": config::hash(&token), "computer": computer, "client": "claude-code" })),
        Duration::from_secs(20),
    );
    let reply = match asked {
        Ok(reply) => reply,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let (Some(code), Some(path)) = (reply.body["code"].as_str(), reply.body["path"].as_str()) else {
        println!("klistra did not give a code: {}", reply.error().unwrap_or_else(|| format!("it answered {}", reply.status)));
        return 1;
    };
    println!("To connect {computer} to klistra, open this link, sign in and allow it:");
    println!();
    println!("  {base}{path}");
    println!();
    println!("The page shows the code {code}. The link works for ten minutes.");
    if !setup {
        println!("Then run: klistra login --wait");
        return 0;
    }
    println!("Waiting for the computer to be allowed...");
    wait_for(&base, &token, 200, true).unwrap_or_else(|| {
        println!("The link ran out before the computer was allowed. Run the setup again for a new one.");
        2
    })
}

/// `klistra status`: whether this computer is connected, and for whom.
pub fn status() -> i32 {
    let base = config::base();
    let Some(token) = config::token(&base) else {
        println!("This computer is not connected to klistra at {base}. klistra login connects it.");
        return 1;
    };
    match me(&base, &token) {
        Ok(reply) if reply.status == 200 => connected(&reply.body, false),
        Ok(reply) if reply.body["waiting"] == true => {
            println!("This computer is waiting to be allowed: open the link klistra login gave, or run it again for a new one.");
            1
        }
        Ok(reply) => {
            println!("Not connected: {}", reply.error().unwrap_or_else(|| format!("klistra answered {}", reply.status)));
            1
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// `klistra logout`: this computer's token reaches nothing from then on. klistra login connects it
/// again, to the same account or another.
pub fn logout() -> i32 {
    let base = config::base();
    let Some(token) = config::token(&base) else {
        println!("This computer was not connected to klistra.");
        return 0;
    };
    match http::send("DELETE", &format!("{base}/api/agent/me"), Some(&token), None, Duration::from_secs(20)) {
        Ok(reply) if reply.status == 200 => {
            println!("Disconnected this computer from klistra.");
            0
        }
        Ok(_) => {
            println!("This computer was not connected to klistra.");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}
