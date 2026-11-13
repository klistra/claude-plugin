//! klistra's runtime for Claude Code. Claude Code starts it as the plugin's MCP server
//! (`klistra mcp`), which relays each call to klistra with this computer's token, so that Claude
//! Code itself never handles the token, and puts what the person says to it in the studio's chat
//! into its session. `klistra login` connects the computer to a klistra account, once, by a link
//! and a code allowed on any device.

mod awake;
mod config;
mod http;
mod inbox;
mod login;
mod mcp;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "klistra: lets Claude Code on this computer plan in klistra.

  klistra login           Connect this computer: prints a link to open and allow it, and the
                          code that page shows
  klistra login --wait    Wait until it is allowed (up to 100 seconds)
  klistra login --setup   Both in one go, waiting as long as the link works (ten minutes)
  klistra status          Whether this computer is connected, and for whom
  klistra logout          Disconnect this computer
  klistra mcp             The plugin's MCP server (Claude Code runs it)
  klistra inbox <text>    Put a message into the Claude Code session this is run from, as
                          messages from the studio's chat are: to check that works here
  klistra wait            Keep the Claude Code session this is run from awake while it is
                          connected to a chat in the studio (Claude starts it in the desktop app)
  klistra --version

KLISTRA_URL points it at a klistra other than https://app.klistra.ai.";

/// For whoever reads Claude Code's logs: stdout belongs to the MCP protocol.
pub fn log(message: &str) {
    eprintln!("[klistra] {message}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("mcp") => mcp::run(),
        Some("login") => std::process::exit(login::login(args.get(1).map(String::as_str))),
        Some("logout") => std::process::exit(login::logout()),
        Some("status") => std::process::exit(login::status()),
        Some("wait") => std::process::exit(awake::wait(&args[1..])),
        Some("inbox") => std::process::exit(inbox::post_here(&args[1..].join(" "))),
        Some("--version" | "-v") => println!("{VERSION}"),
        Some("help" | "--help" | "-h") => println!("{HELP}"),
        _ => {
            eprintln!("{HELP}");
            std::process::exit(1);
        }
    }
}
