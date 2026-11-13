//! Where klistra is, and this computer's token for it: made the first time it is needed, kept in
//! ~/.klistra where only this person can read it (a file for each klistra address), and sent
//! nowhere but klistra. `klistra login` has it allowed for an account; until then it reaches
//! nothing. klistra keeps only its SHA-256.

use std::io::Write;
use std::path::{Path, PathBuf};

pub const DEFAULT_URL: &str = "https://app.klistra.ai";

/// klistra's address, without a trailing slash.
pub fn base() -> String {
    std::env::var("KLISTRA_URL")
        .ok()
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| DEFAULT_URL.to_string())
}

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default()
}

/// "https://app.klistra.ai" → "app.klistra.ai", "http://localhost:4211" → "localhost_4211": the
/// names the plugin's first scripts gave the file, so the computers they connected stay connected.
fn host_of(base: &str) -> String {
    let rest = base.split_once("://").map_or(base, |(_, rest)| rest);
    rest.split('/').next().unwrap_or("").chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect()
}

/// Where klistra keeps what it keeps on this computer: ~/.klistra.
pub fn dir() -> PathBuf {
    home().join(".klistra")
}

pub fn token_file(base: &str) -> PathBuf {
    dir().join(format!("token-{}", host_of(base)))
}

/// ~/.klistra, made only this person's when it is made.
pub fn ensure_dir() -> Result<PathBuf, String> {
    let dir = dir();
    private_dir(&dir)?;
    Ok(dir)
}

/// This computer's token, if it has made one.
pub fn token(base: &str) -> Option<String> {
    std::fs::read_to_string(token_file(base)).ok().map(|t| t.trim().to_string()).filter(|t| t.starts_with("kla_"))
}

/// This computer's token, made now if it has none yet.
pub fn token_or_new(base: &str) -> Result<String, String> {
    if let Some(token) = token(base) {
        return Ok(token);
    }
    let file = token_file(base);
    private_dir(file.parent().unwrap_or(Path::new(".")))?;
    let made = format!("kla_{}", base64url(&random::<32>()?));
    match new_private_file(&file) {
        Ok(mut f) => {
            f.write_all(made.as_bytes()).map_err(|e| format!("Could not write {}: {e}", file.display()))?;
            Ok(made)
        }
        // two asking at once keep the one written first
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => token(base).ok_or_else(|| format!("{} is being written; try again", file.display())),
        Err(e) => Err(format!("Could not write {}: {e}", file.display())),
    }
}

#[cfg(unix)]
fn private_dir(dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir).map_err(|e| format!("Could not make {}: {e}", dir.display()))
}

#[cfg(not(unix))]
fn private_dir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Could not make {}: {e}", dir.display()))
}

fn new_private_file(file: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(file)
}

fn random<const N: usize>() -> Result<[u8; N], String> {
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; N];
    ring::rand::SystemRandom::new().fill(&mut bytes).map_err(|_| "This computer gave no random numbers".to_string())?;
    Ok(bytes)
}

/// The token's SHA-256, in hex: all klistra keeps of it.
pub fn hash(token: &str) -> String {
    ring::digest::digest(&ring::digest::SHA256, token.as_bytes()).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_file_as_the_scripts_did() {
        assert_eq!(host_of("https://app.klistra.ai"), "app.klistra.ai");
        assert_eq!(host_of("http://localhost:4211"), "localhost_4211");
        assert_eq!(host_of("http://localhost:4211/api/agent/mcp"), "localhost_4211");
    }

    #[test]
    fn tokens_look_like_klistras() {
        let token = format!("kla_{}", base64url(&[0xfb; 32]));
        assert_eq!(token.len(), 4 + 43);
        assert!(token[4..].chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_eq!(base64url(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(hash("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
