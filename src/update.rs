//! Release checking and Linux self-update.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::paths::{ensure_dir, sha256_hex, update_dir};

const REPOSITORY: &str = "oharato/mdview";
const API_BASE: &str = "https://api.github.com/repos";

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Clone)]
pub struct LatestRelease {
    pub version: String,
    pub url: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    tag_name: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct TagResponse {
    name: Option<String>,
}

#[derive(Debug, Clone)]
pub enum InstallKind {
    Binary(PathBuf),
    Unknown,
}

#[derive(Debug, Clone)]
pub struct UpdateCheck {
    pub current: String,
    pub latest: Option<LatestRelease>,
    pub has_update: bool,
    pub install: InstallKind,
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn normalize(version: &str) -> [u32; 3] {
    let mut output = [0u32; 3];
    let trimmed = version.trim().trim_start_matches('v');
    for (index, part) in trimmed.split(['.', '-', '+']).take(3).enumerate() {
        output[index] = part.parse::<u32>().unwrap_or(0);
    }
    output
}

/// Compare semantic versions (a > b is positive).
pub fn compare_versions(a: &str, b: &str) -> i64 {
    let a = normalize(a);
    let b = normalize(b);
    for index in 0..3 {
        let diff = a[index] as i64 - b[index] as i64;
        if diff != 0 {
            return diff;
        }
    }
    0
}

fn github_json<T: for<'de> Deserialize<'de>>(path: &str) -> Option<T> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let url = format!("{API_BASE}/{REPOSITORY}{path}");
    let mut response = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", format!("mdview/{}", current_version()))
        .call()
        .ok()?;
    let body = response.body_mut().read_to_string().ok()?;
    serde_json::from_str(&body).ok()
}

/// Fetch the latest GitHub release (falling back to the latest tag).
pub fn fetch_latest_release() -> Option<LatestRelease> {
    if let Some(release) = github_json::<ReleaseResponse>("/releases/latest") {
        if let Some(tag) = release.tag_name {
            return Some(LatestRelease {
                version: tag.trim_start_matches('v').to_string(),
                url: release
                    .html_url
                    .unwrap_or_else(|| format!("https://github.com/{REPOSITORY}/releases")),
                assets: release.assets,
            });
        }
    }

    let tags = github_json::<Vec<TagResponse>>("/tags?per_page=1")?;
    let name = tags.first()?.name.clone()?;
    Some(LatestRelease {
        version: name.trim_start_matches('v').to_string(),
        url: format!("https://github.com/{REPOSITORY}/releases/tag/{name}"),
        assets: Vec::new(),
    })
}

pub fn detect_install_kind() -> InstallKind {
    std::env::current_exe()
        .map(InstallKind::Binary)
        .unwrap_or(InstallKind::Unknown)
}

pub fn describe_install(kind: &InstallKind) -> String {
    match kind {
        InstallKind::Binary(path) => format!("binary ({})", path.display()),
        InstallKind::Unknown => "不明".to_string(),
    }
}

/// Check for a newer release without modifying anything.
pub fn check_for_update() -> UpdateCheck {
    let current = current_version().to_string();
    let latest = fetch_latest_release();
    let has_update = latest
        .as_ref()
        .map(|release| compare_versions(&release.version, &current) > 0)
        .unwrap_or(false);
    UpdateCheck {
        current,
        latest,
        has_update,
        install: detect_install_kind(),
    }
}

fn target_triple() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        _ => None,
    }
}

fn find_asset<'a>(release: &'a LatestRelease, name: &str) -> Option<&'a ReleaseAsset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .or_else(|| {
            release
                .assets
                .iter()
                .find(|asset| asset.name.contains(name))
        })
}

fn download(url: &str) -> Option<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header("User-Agent", format!("mdview/{}", current_version()))
        .call()
        .ok()?;
    response.body_mut().read_to_vec().ok()
}

fn expected_checksum(release: &LatestRelease, asset_name: &str) -> Option<String> {
    let checksums = find_asset(release, "SHA256SUMS")?;
    let text = String::from_utf8(download(&checksums.browser_download_url)?).ok()?;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        if name == asset_name {
            return Some(hash.to_string());
        }
    }
    None
}

/// Download and replace the running Linux binary.
pub fn run_self_update() -> Result<String> {
    let current = current_version();
    let Some(latest) = fetch_latest_release() else {
        anyhow::bail!("最新バージョンを取得できませんでした（ネットワークを確認してください）");
    };
    if compare_versions(&latest.version, current) <= 0 {
        return Ok(format!("最新です (v{current})"));
    }
    let Some(target) = target_triple() else {
        anyhow::bail!(
            "このプラットフォーム用の自動更新は未対応です。{} から取得してください",
            latest.url
        );
    };
    let asset_name = format!("mdview-{target}");
    let Some(asset) = find_asset(&latest, &asset_name) else {
        anyhow::bail!(
            "v{} の {} が見つかりません。{} を確認してください",
            latest.version,
            asset_name,
            latest.url
        );
    };

    eprintln!("v{current} → v{} に更新します", latest.version);
    let bytes = download(&asset.browser_download_url)
        .with_context(|| format!("{} をダウンロードできませんでした", asset.name))?;
    if let Some(expected) = expected_checksum(&latest, &asset.name) {
        let actual = sha256_hex(&bytes);
        if actual != expected {
            anyhow::bail!("チェックサムが一致しません（期待 {expected}, 実際 {actual}）");
        }
    }

    let directory = update_dir();
    ensure_dir(&directory)?;
    let new_path = directory.join("mdview.new");
    std::fs::write(&new_path, &bytes)
        .with_context(|| format!("{} に書き込めません", new_path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new_path, std::fs::Permissions::from_mode(0o755))?;
    }

    let current_exe = std::env::current_exe().context("現在の実行ファイルを特定できません")?;
    std::fs::rename(&new_path, &current_exe).or_else(|_| {
        std::fs::copy(&new_path, &current_exe)?;
        std::fs::remove_file(&new_path)
    })?;
    Ok(format!("v{} に更新しました", latest.version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(compare_versions("0.7.0", "0.6.1") > 0);
        assert_eq!(compare_versions("v0.6.1", "0.6.1"), 0);
        assert!(compare_versions("0.6.0", "0.6.1") < 0);
        assert!(compare_versions("1.0", "0.99.99") > 0);
    }

    #[test]
    fn current_version_is_non_empty() {
        assert!(!current_version().is_empty());
    }
}
