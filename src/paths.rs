//! Cache paths and stable hashes.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Root cache directory (`MDVIEW_CACHE` overrides it).
pub fn cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("MDVIEW_CACHE") {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("mdview")
}

pub fn mermaid_cache_dir() -> PathBuf {
    cache_dir().join("mermaid")
}

pub fn remote_cache_dir() -> PathBuf {
    cache_dir().join("remote")
}

pub fn caps_path() -> PathBuf {
    cache_dir().join("caps.json")
}

pub fn update_dir() -> PathBuf {
    cache_dir().join("update")
}

/// Create a directory and all parents.
pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Short stable hash for cache keys.
pub fn short_hash(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    let mut output = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

/// Hex encoded SHA-256 (used for release asset verification).
pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_hash_is_stable_and_bounded() {
        let a = short_hash(&["abc", "def"]);
        let b = short_hash(&["abc", "def"]);
        let c = short_hash(&["abc", "deg"]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn sha256_matches_known_value() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
