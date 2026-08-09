use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use reqwest::{Client, StatusCode, Url};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncWriteExt;

const MAX_UPDATE_BYTES: u64 = 1_073_741_824;
const UPDATE_DIRECTORY: &str = "nerftrack-updates";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_url: Option<String>,
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedUpdate {
    pub version: String,
    pub asset_name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallUpdateResult {
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

fn parse_repository_url(repository_url: &str) -> Result<(String, String), String> {
    let parsed = Url::parse(repository_url.trim())
        .map_err(|_| "GitHub repository URL is not a valid URL".to_string())?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("github.com")
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(
            "GitHub repository URL must be an https://github.com/owner/repository URL".into(),
        );
    }
    let segments: Vec<&str> = parsed
        .path_segments()
        .ok_or_else(|| "GitHub repository URL has no repository path".to_string())?
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.len() != 2 {
        return Err("GitHub repository URL must include exactly one owner and repository".into());
    }
    let owner = valid_repository_segment(segments[0], "owner")?;
    let repository = segments[1].strip_suffix(".git").unwrap_or(segments[1]);
    let repository = valid_repository_segment(repository, "repository")?;
    Ok((owner, repository))
}

fn valid_repository_segment(value: &str, label: &str) -> Result<String, String> {
    if value.is_empty()
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(format!("GitHub {label} contains unsupported characters"));
    }
    Ok(value.to_string())
}

fn github_client() -> Result<Client, String> {
    Client::builder()
        .user_agent(format!("NerfTrack/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|_| "could not initialize the GitHub update client".into())
}

async fn fetch_latest_release(repository_url: &str) -> Result<GithubRelease, String> {
    let (owner, repository) = parse_repository_url(repository_url)?;
    let endpoint = format!("https://api.github.com/repos/{owner}/{repository}/releases/latest");
    let response = github_client()?
        .get(endpoint)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| "could not reach GitHub. Check your internet connection".to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status {
            StatusCode::NOT_FOUND => "GitHub repository or published release was not found".into(),
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => {
                "GitHub rate-limited the update check. Try again later".into()
            }
            _ => format!("GitHub returned HTTP {status} while checking releases"),
        });
    }
    response
        .json::<GithubRelease>()
        .await
        .map_err(|_| "GitHub returned an invalid release response".into())
}

fn current_version() -> Result<Version, String> {
    Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| "the installed NerfTrack version is invalid".into())
}

fn release_version(tag: &str) -> Result<Version, String> {
    let normalized = tag.trim().trim_start_matches(['v', 'V']);
    Version::parse(normalized)
        .map_err(|_| format!("GitHub release tag {tag:?} is not a valid semantic version"))
}

#[cfg(target_os = "windows")]
fn supported_asset_extension(extension: &str) -> bool {
    matches!(extension, "msi" | "exe")
}

#[cfg(target_os = "macos")]
fn supported_asset_extension(extension: &str) -> bool {
    matches!(extension, "dmg" | "pkg" | "zip")
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn supported_asset_extension(_extension: &str) -> bool {
    false
}

#[cfg(target_os = "windows")]
fn platform_tokens() -> &'static [&'static str] {
    &["windows", "win32", "win64"]
}

#[cfg(target_os = "macos")]
fn platform_tokens() -> &'static [&'static str] {
    &["macos", "mac-os", "darwin", "osx"]
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_tokens() -> &'static [&'static str] {
    &[]
}

#[cfg(target_arch = "x86_64")]
fn architecture_tokens() -> &'static [&'static str] {
    &["x86-64", "x86_64", "x64", "amd64", "intel"]
}

#[cfg(target_arch = "aarch64")]
fn architecture_tokens() -> &'static [&'static str] {
    &["aarch64", "arm64", "apple-silicon"]
}

#[cfg(target_arch = "x86")]
fn architecture_tokens() -> &'static [&'static str] {
    &["x86", "i686", "win32"]
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "x86")))]
fn architecture_tokens() -> &'static [&'static str] {
    &[]
}

fn normalized_asset_name(name: &str) -> String {
    name.to_ascii_lowercase().replace(['_', ' ', '.'], "-")
}

fn asset_matches_platform(asset: &GithubAsset) -> bool {
    let extension = Path::new(&asset.name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if !supported_asset_extension(&extension) {
        return false;
    }
    let normalized = normalized_asset_name(&asset.name);
    let architecture_matches = architecture_tokens()
        .iter()
        .any(|token| normalized.contains(token));
    if !architecture_matches {
        return false;
    }
    let platform_matches = platform_tokens()
        .iter()
        .any(|token| normalized.contains(token));
    let other_platform_present = [
        "windows", "win32", "win64", "macos", "mac-os", "darwin", "osx",
    ]
    .iter()
    .any(|token| normalized.contains(token) && !platform_tokens().contains(token));
    platform_matches || !other_platform_present
}

fn asset_priority(asset: &GithubAsset) -> u8 {
    let extension = Path::new(&asset.name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    #[cfg(target_os = "windows")]
    {
        if extension == "msi" {
            0
        } else {
            1
        }
    }
    #[cfg(target_os = "macos")]
    {
        if extension == "pkg" {
            0
        } else if extension == "dmg" {
            1
        } else {
            2
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = extension;
        0
    }
}

fn select_asset(release: &GithubRelease) -> Option<&GithubAsset> {
    release
        .assets
        .iter()
        .filter(|asset| asset_matches_platform(asset))
        .min_by_key(|asset| asset_priority(asset))
}

fn update_result(
    current: &Version,
    latest: &Version,
    release: &GithubRelease,
) -> UpdateCheckResult {
    let update_available = latest > current;
    let asset = if update_available {
        select_asset(release)
    } else {
        None
    };
    let message = if !update_available {
        if latest == current {
            format!("NerfTrack v{current} is up to date.")
        } else {
            format!("NerfTrack v{current} is newer than the latest published release.")
        }
    } else if let Some(asset) = asset {
        format!(
            "NerfTrack v{latest} is ready to download. Compatible asset: {}.",
            asset.name
        )
    } else {
        format!(
            "NerfTrack v{latest} is newer, but no compatible Windows or macOS asset was published."
        )
    };
    UpdateCheckResult {
        current_version: current.to_string(),
        latest_version: Some(latest.to_string()),
        update_available,
        release_url: Some(release.html_url.clone()),
        asset_name: asset.map(|value| value.name.clone()),
        asset_url: asset.map(|value| value.browser_download_url.clone()),
        message,
    }
}

#[tauri::command]
pub async fn check_for_update(repository_url: String) -> Result<UpdateCheckResult, String> {
    let current = current_version()?;
    let release = fetch_latest_release(&repository_url).await?;
    let latest = release_version(&release.tag_name)?;
    Ok(update_result(&current, &latest, &release))
}

fn safe_asset_filename(name: &str) -> String {
    let filename = Path::new(name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("release-package");
    filename
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn validate_asset_url(url: &str) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|_| "GitHub returned an invalid asset URL".to_string())?;
    let host = parsed.host_str().unwrap_or_default();
    if parsed.scheme() != "https"
        || !matches!(
            host,
            "github.com" | "objects.githubusercontent.com" | "release-assets.githubusercontent.com"
        )
    {
        return Err("GitHub returned an unsupported download host".into());
    }
    Ok(())
}

async fn download_asset(
    asset: &GithubAsset,
    version: &Version,
) -> Result<DownloadedUpdate, String> {
    validate_asset_url(&asset.browser_download_url)?;
    if asset.size > MAX_UPDATE_BYTES {
        return Err("the published update package is too large to download safely".into());
    }
    let extension = Path::new(&asset.name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if !supported_asset_extension(&extension) {
        return Err("the published update package is not supported on this platform".into());
    }

    let directory = std::env::temp_dir().join(UPDATE_DIRECTORY);
    fs::create_dir_all(&directory)
        .await
        .map_err(|_| "could not create the temporary update directory".to_string())?;
    let filename = format!(
        "NerfTrack-update-{version}-{}",
        safe_asset_filename(&asset.name)
    );
    let final_path = directory.join(filename);
    let partial_path = final_path.with_extension(format!("{extension}.part"));
    let _ = fs::remove_file(&partial_path).await;

    let result = async {
        let response = github_client()?
            .get(&asset.browser_download_url)
            .header("Accept", "application/octet-stream")
            .send()
            .await
            .map_err(|_| "the update download could not be reached".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "GitHub returned HTTP {} while downloading",
                response.status()
            ));
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_UPDATE_BYTES)
        {
            return Err("the downloaded update package is too large".into());
        }
        let mut file = fs::File::create(&partial_path)
            .await
            .map_err(|_| "could not create the temporary update file".to_string())?;
        let mut response = response;
        let mut hasher = Sha256::new();
        let mut downloaded = 0_u64;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "the update download was interrupted".to_string())?
        {
            downloaded = downloaded
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| "the update download size overflowed".to_string())?;
            if downloaded > MAX_UPDATE_BYTES {
                return Err("the update download exceeded the safe size limit".into());
            }
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(|_| "the update could not be written to disk".to_string())?;
        }
        file.flush()
            .await
            .map_err(|_| "the update could not be flushed to disk".to_string())?;
        drop(file);
        if downloaded != asset.size {
            return Err("the update download was incomplete".into());
        }
        if let Some(expected) = asset
            .digest
            .as_deref()
            .and_then(|digest| digest.strip_prefix("sha256:"))
        {
            let actual = format!("{:x}", hasher.finalize());
            if !expected.eq_ignore_ascii_case(&actual) {
                return Err("the downloaded update failed its SHA-256 verification".into());
            }
        }
        let _ = fs::remove_file(&final_path).await;
        fs::rename(&partial_path, &final_path)
            .await
            .map_err(|_| "could not finalize the downloaded update".to_string())?;
        Ok(DownloadedUpdate {
            version: version.to_string(),
            asset_name: asset.name.clone(),
            path: final_path.to_string_lossy().into_owned(),
        })
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&partial_path).await;
    }
    result
}

#[tauri::command]
pub async fn download_update(repository_url: String) -> Result<DownloadedUpdate, String> {
    let current = current_version()?;
    let release = fetch_latest_release(&repository_url).await?;
    let latest = release_version(&release.tag_name)?;
    if latest <= current {
        return Err("NerfTrack is already up to date".into());
    }
    let asset = select_asset(&release)
        .ok_or_else(|| "the latest release has no compatible Windows or macOS asset".to_string())?;
    download_asset(asset, &latest).await
}

fn validate_downloaded_path(path: &str) -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(UPDATE_DIRECTORY);
    let canonical_root = std::fs::canonicalize(&root)
        .map_err(|_| "the temporary update directory is unavailable".to_string())?;
    let canonical_path = std::fs::canonicalize(path)
        .map_err(|_| "the downloaded update file is unavailable".to_string())?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err("the update file is outside NerfTrack's safe temporary directory".into());
    }
    let filename = canonical_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "the update file has no valid filename".to_string())?;
    if !filename.starts_with("NerfTrack-update-") {
        return Err("the update file was not downloaded by NerfTrack".into());
    }
    let extension = canonical_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if !supported_asset_extension(&extension) {
        return Err("the update file type is unsupported on this platform".into());
    }
    if !canonical_path.is_file() {
        return Err("the update path is not a regular file".into());
    }
    Ok(canonical_path)
}

#[cfg(target_os = "windows")]
fn launch_installer(path: &Path, extension: &str) -> Result<InstallUpdateResult, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    let mut installer = if extension == "msi" {
        let mut command = Command::new("msiexec");
        command.arg("/i").arg(path);
        command
    } else {
        Command::new(path)
    };
    installer
        .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
        .spawn()
        .map_err(|_| "Windows could not launch the update installer".to_string())?;
    Ok(InstallUpdateResult {
        message: "Installer launched. Follow its prompts to finish updating NerfTrack.".into(),
    })
}

#[cfg(target_os = "macos")]
fn launch_installer(path: &Path, extension: &str) -> Result<InstallUpdateResult, String> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|_| "macOS could not open the downloaded update package".to_string())?;
    let message = if extension == "pkg" {
        "macOS Installer launched. Follow its prompts to finish updating NerfTrack."
    } else {
        "Update package opened. Install the newer NerfTrack app from the mounted package."
    };
    Ok(InstallUpdateResult {
        message: message.into(),
    })
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn launch_installer(_path: &Path, _extension: &str) -> Result<InstallUpdateResult, String> {
    Err("Automatic installation is supported only on Windows and macOS".into())
}

#[tauri::command]
pub fn install_update(path: String) -> Result<InstallUpdateResult, String> {
    let path = validate_downloaded_path(&path)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    launch_installer(&path, &extension)
}

#[cfg(target_os = "windows")]
fn launch_external_url(url: &str) -> Result<(), String> {
    Command::new("cmd")
        .args(["/D", "/C", "start", "", url])
        .spawn()
        .map_err(|_| "Windows could not open the GitHub repository".to_string())?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn launch_external_url(url: &str) -> Result<(), String> {
    Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|_| "macOS could not open the GitHub repository".to_string())?;
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn launch_external_url(url: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map_err(|_| "the system could not open the GitHub repository".to_string())?;
    Ok(())
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    parse_repository_url(&url)?;
    launch_external_url(url.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_repository_urls_without_query_or_fragment() {
        assert_eq!(
            parse_repository_url("https://github.com/nerftrack/nerftrack"),
            Ok(("nerftrack".into(), "nerftrack".into()))
        );
        assert!(parse_repository_url("https://github.com/nerftrack/nerftrack/releases").is_err());
        assert!(parse_repository_url("http://github.com/nerftrack/nerftrack").is_err());
    }

    #[test]
    fn strips_a_v_prefix_and_rejects_invalid_release_versions() {
        assert_eq!(release_version("v1.2.3").unwrap(), Version::new(1, 2, 3));
        assert!(release_version("latest").is_err());
    }
}
