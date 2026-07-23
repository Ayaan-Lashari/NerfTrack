use std::env;
use std::path::{Path, PathBuf};

use crate::models::{DiscoveryState, DiscoveryStatus};

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
}

pub fn redact_path(path: &Path) -> String {
    let Some(home) = home_dir() else {
        return "local path redacted".into();
    };
    if let Ok(relative) = path.strip_prefix(&home) {
        let suffix = relative.to_string_lossy();
        if suffix.is_empty() {
            "~".into()
        } else {
            format!("~/{suffix}")
        }
    } else if path.starts_with("/usr/local")
        || path.starts_with("/opt/homebrew")
        || path.starts_with("C:\\")
    {
        path.to_string_lossy().into_owned()
    } else {
        path.file_name()
            .map(|name| format!("…/{}", name.to_string_lossy()))
            .unwrap_or_else(|| "local path redacted".into())
    }
}

fn existing_directory_candidate(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|candidate| candidate.is_dir())
}

fn existing_file_candidate(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|candidate| candidate.is_file())
}

pub fn codex_home_candidates(override_path: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = override_path {
        candidates.push(path.to_path_buf());
    }
    if let Some(path) = env::var_os("CODEX_HOME") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(home) = home_dir() {
        candidates.push(home.join(".codex"));
        if cfg!(target_os = "macos") {
            candidates.push(home.join("Library/Application Support/Codex"));
            candidates.push(home.join("Library/Application Support/com.openai.codex"));
            candidates.push(home.join("Library/Application Support/OpenAI/Codex"));
        }
        if cfg!(windows) {
            candidates.push(home.join("AppData/Roaming/Codex"));
            candidates.push(home.join("AppData/Local/Codex"));
        }
        if cfg!(unix) {
            candidates.push(home.join(".local/share/codex"));
            candidates.push(home.join(".local/share/Codex"));
            candidates.push(home.join(".config/codex"));
        }
    }
    if let Some(path) = env::var_os("APPDATA") {
        candidates.push(PathBuf::from(path).join("Codex"));
    }
    if let Some(path) = env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(path).join("Codex"));
    }
    if let Some(path) = env::var_os("XDG_DATA_HOME") {
        candidates.push(PathBuf::from(path).join("codex"));
    }
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        candidates.push(PathBuf::from(path).join("codex"));
    }
    candidates
}

pub fn discover_codex_home(override_path: Option<&Path>) -> (Option<PathBuf>, DiscoveryStatus) {
    let candidates = codex_home_candidates(override_path);
    let selected = existing_directory_candidate(candidates);
    match selected {
        Some(path) => (Some(path.clone()), home_status(&path, override_path)),
        None => (None, DiscoveryStatus::missing("Not discovered yet")),
    }
}

pub fn discover_codex_home_for_mode(
    override_path: Option<&Path>,
    prefer_gui: bool,
) -> (Option<PathBuf>, DiscoveryStatus) {
    if prefer_gui && override_path.is_none() {
        let selected = existing_directory_candidate(
            codex_home_candidates(None)
                .into_iter()
                .filter(|candidate| is_gui_home(candidate)),
        );
        if let Some(path) = selected {
            return (Some(path.clone()), home_status(&path, None));
        }
    }
    discover_codex_home(override_path)
}

fn home_status(path: &Path, override_path: Option<&Path>) -> DiscoveryStatus {
    let selected = override_path
        .map(|override_path| override_path == path)
        .unwrap_or(false);
    DiscoveryStatus {
        state: if selected {
            DiscoveryState::Selected
        } else {
            DiscoveryState::AutoDetected
        },
        redacted_location: Some(redact_path(path)),
        message: if selected {
            "Selected".into()
        } else if is_gui_home(path) {
            "Desktop app data detected".into()
        } else {
            "Auto-detected".into()
        },
    }
}

pub fn codex_binary_candidates(override_path: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = override_path {
        candidates.push(path.to_path_buf());
    }
    if let Some(path) = env::var_os("CODEX_BINARY") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(path) = which_on_path("codex") {
        candidates.push(path);
    }
    if cfg!(windows) {
        if let Some(path) = which_on_path("codex.cmd") {
            candidates.push(path);
        }
        if let Some(path) = which_on_path("codex.exe") {
            candidates.push(path);
        }
    }
    if let Some(home) = home_dir() {
        candidates.push(home.join(".local/bin/codex"));
        candidates.push(home.join("Library/Application Support/Codex/bin/codex"));
        if cfg!(target_os = "macos") {
            candidates.push(home.join("Applications/ChatGPT.app/Contents/Resources/codex"));
            candidates.push(home.join("Applications/Codex.app/Contents/Resources/codex"));
        }
    }
    if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from(
            "/Applications/ChatGPT.app/Contents/Resources/codex",
        ));
        candidates.push(PathBuf::from(
            "/Applications/Codex.app/Contents/Resources/codex",
        ));
    }
    candidates.push(PathBuf::from("/usr/local/bin/codex"));
    candidates.push(PathBuf::from("/opt/homebrew/bin/codex"));
    candidates
}

#[cfg(target_os = "macos")]
fn macos_app_binary_candidates() -> Vec<PathBuf> {
    let Ok(output) = std::process::Command::new("/usr/bin/mdfind")
        .arg("kMDItemFSName == 'ChatGPT.app' || kMDItemFSName == 'Codex.app'")
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .flat_map(|bundle| {
            let bundle = PathBuf::from(bundle.trim());
            [
                bundle.join("Contents/Resources/codex"),
                bundle.join("Contents/MacOS/codex"),
                bundle.join("Contents/MacOS/Codex"),
            ]
        })
        .collect()
}

fn which_on_path(command: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(command))
        .find(|candidate| candidate.is_file())
}

pub fn discover_codex_binary(override_path: Option<&Path>) -> (Option<PathBuf>, DiscoveryStatus) {
    let selected = existing_file_candidate(codex_binary_candidates(override_path)).or_else(|| {
        #[cfg(target_os = "macos")]
        {
            existing_file_candidate(macos_app_binary_candidates())
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    });
    match selected {
        Some(path) => (
            Some(path.clone()),
            DiscoveryStatus {
                state: if override_path.is_some() {
                    DiscoveryState::Selected
                } else {
                    DiscoveryState::AutoDetected
                },
                redacted_location: Some(redact_path(&path)),
                message: if override_path.is_some() {
                    "Selected".into()
                } else {
                    "Auto-detected".into()
                },
            },
        ),
        None => (None, DiscoveryStatus::missing("Not discovered yet")),
    }
}

pub fn app_server_status(binary_found: bool) -> DiscoveryStatus {
    if binary_found {
        DiscoveryStatus {
            state: DiscoveryState::AutoDetected,
            redacted_location: Some("Local stdio App Server".into()),
            message: "Ready to supervise".into(),
        }
    } else {
        DiscoveryStatus::missing("Waiting for Codex executable")
    }
}

pub fn app_server_status_for_mode(binary_found: bool, gui_mode: bool) -> DiscoveryStatus {
    if gui_mode {
        DiscoveryStatus::not_required("Not required for desktop app")
    } else {
        app_server_status(binary_found)
    }
}

pub fn is_gui_home(path: &Path) -> bool {
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let normalized = normalized.trim_end_matches('/');
    normalized.ends_with("/library/application support/codex")
        || normalized.ends_with("/library/application support/com.openai.codex")
        || normalized.ends_with("/library/application support/openai/codex")
        || normalized.ends_with("/appdata/roaming/codex")
        || normalized.ends_with("/appdata/local/codex")
        || normalized.ends_with("/.local/share/codex")
        || normalized.ends_with("/.config/codex")
}

pub fn is_gui_binary(path: &Path) -> bool {
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    normalized.ends_with("/chatgpt.app/contents/resources/codex")
        || normalized.ends_with("/codex.app/contents/resources/codex")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_home_username() {
        let Some(home) = home_dir() else {
            return;
        };
        let redacted = redact_path(&home.join("Library/Application Support/Codex"));
        assert!(redacted.starts_with("~/"));
        assert!(!redacted.contains(home.file_name().unwrap().to_string_lossy().as_ref()));
    }

    #[test]
    fn honors_codex_home_override_first() {
        let candidates = codex_home_candidates(Some(Path::new("/tmp/nerfify-test-home")));
        assert_eq!(
            candidates.first().unwrap(),
            Path::new("/tmp/nerfify-test-home")
        );
    }

    #[test]
    fn recognizes_desktop_app_data_roots() {
        assert!(is_gui_home(Path::new(
            "/tmp/nerfify/Library/Application Support/Codex"
        )));
        assert!(is_gui_home(Path::new(
            "C:/profiles/sample/AppData/Roaming/Codex"
        )));
        assert!(!is_gui_home(Path::new("/tmp/nerfify/Documents/Codex")));
        assert!(!is_gui_home(Path::new("/tmp/nerfify/.codex")));
        assert!(is_gui_binary(Path::new(
            "/Applications/ChatGPT.app/Contents/Resources/codex"
        )));
        assert!(!is_gui_binary(Path::new("/usr/local/bin/codex")));
    }

    #[test]
    fn desktop_app_does_not_require_app_server() {
        let status = app_server_status_for_mode(false, true);
        assert!(matches!(status.state, DiscoveryState::NotRequired));
        assert_eq!(status.message, "Not required for desktop app");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn includes_standard_macos_desktop_binary_candidates() {
        let candidates = codex_binary_candidates(None);
        assert!(candidates
            .iter()
            .any(|candidate| { candidate.ends_with("ChatGPT.app/Contents/Resources/codex") }));
    }
}
