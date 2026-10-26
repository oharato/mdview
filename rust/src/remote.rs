//! Remote and local image source resolution with an on-disk cache.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::paths::{ensure_dir, remote_cache_dir, short_hash};
use crate::term::warn;

fn url_extension(url: &str) -> String {
    let path = url.split('?').next().unwrap_or(url);
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_else(|| ".img".to_string())
}

fn fetch(url: &str) -> Result<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header(
            "User-Agent",
            format!("mdview/{}", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .with_context(|| format!("HTTP request failed: {url}"))?;
    response
        .body_mut()
        .read_to_vec()
        .with_context(|| format!("failed to read response body: {url}"))
}

/// Resolve an image reference to a local path, downloading remote images into
/// the cache. Returns `None` when the image cannot be found.
pub fn resolve_image_source(src: &str, base_dir: &Path) -> Option<PathBuf> {
    if src.starts_with("http://") || src.starts_with("https://") {
        let destination =
            remote_cache_dir().join(format!("{}{}", short_hash(&[src]), url_extension(src)));
        if destination.exists() {
            return Some(destination);
        }
        if let Err(error) = ensure_dir(&remote_cache_dir()) {
            warn(&format!("キャッシュを作成できません: {error}"));
            return None;
        }
        match fetch(src) {
            Ok(bytes) => {
                if let Err(error) = std::fs::write(&destination, bytes) {
                    warn(&format!("画像を保存できません: {error}"));
                    return None;
                }
                Some(destination)
            }
            Err(error) => {
                warn(&format!("画像の取得に失敗: {src} ({error})"));
                None
            }
        }
    } else {
        let path = if Path::new(src).is_absolute() {
            PathBuf::from(src)
        } else {
            base_dir.join(src)
        };
        if path.exists() {
            Some(path)
        } else {
            warn(&format!("画像が見つかりません: {src}"));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_url_extension() {
        assert_eq!(url_extension("https://example.com/a.png?x=1"), ".png");
        assert_eq!(url_extension("https://example.com/a"), ".img");
    }

    #[test]
    fn resolves_local_images_relative_to_base() {
        let base = std::env::temp_dir();
        let name = format!("mdview-test-{}.png", std::process::id());
        let path = base.join(&name);
        std::fs::write(&path, b"x").expect("write");
        let resolved = resolve_image_source(&name, &base).expect("resolved");
        assert_eq!(resolved, path);
        let _ = std::fs::remove_file(path);
    }
}
