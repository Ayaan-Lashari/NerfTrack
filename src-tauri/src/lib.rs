#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod app_server;
pub mod collector;
pub mod discovery;
pub mod estimator;
pub mod models;
pub mod parser;
pub mod pricing;
pub mod storage;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::State;

use crate::models::{
    AccountState, AppSettings, AppStatus, AppStatusState, ConnectionQuality, DataQuality,
    DiscoveryStatus, IntegrationMode, Range, RedactedSelection,
};

pub struct AppState {
    pub database: Mutex<storage::Database>,
    codex_home_override: Mutex<Option<PathBuf>>,
    codex_binary_override: Mutex<Option<PathBuf>>,
}

impl AppState {
    fn new() -> Result<Self, String> {
        let state = Self {
            database: Mutex::new(storage::Database::open()?),
            codex_home_override: Mutex::new(None),
            codex_binary_override: Mutex::new(None),
        };
        let _ = state.reconcile();
        Ok(state)
    }

    fn reconcile(&self) -> Result<(), String> {
        let home_override = self
            .codex_home_override
            .lock()
            .map_err(|_| "discovery state is unavailable".to_string())?
            .clone();
        let binary_override = self
            .codex_binary_override
            .lock()
            .map_err(|_| "discovery state is unavailable".to_string())?
            .clone();
        let (binary, _) = discovery::discover_codex_binary(binary_override.as_deref());
        let Some(home) =
            discovery::discover_codex_home_for_mode(home_override.as_deref(), binary.is_none()).0
        else {
            return Ok(());
        };
        let mut database = self
            .database
            .lock()
            .map_err(|_| "database writer is unavailable".to_string())?;
        let previous = database.load_checkpoint_states()?;
        let collection = collector::scan_codex_home_with_state(&home, &previous)?;
        database.persist_collection(&collection, None, None)?;
        Ok(())
    }

    fn discover_status(&self) -> AppStatus {
        let home_override = self
            .codex_home_override
            .lock()
            .ok()
            .and_then(|path| path.clone());
        let binary_override = self
            .codex_binary_override
            .lock()
            .ok()
            .and_then(|path| path.clone());
        let (binary, detected_codex_executable) =
            discovery::discover_codex_binary(binary_override.as_deref());
        let (home, mut codex_home) =
            discovery::discover_codex_home_for_mode(home_override.as_deref(), binary.is_none());
        let detected_gui_home = home
            .as_ref()
            .map(|path| discovery::is_gui_home(path))
            .unwrap_or(false);
        let embedded_gui_binary = binary
            .as_ref()
            .map(|path| discovery::is_gui_binary(path))
            .unwrap_or(false);
        let explicit_cli_binary =
            binary_override.is_some() || std::env::var_os("CODEX_BINARY").is_some();
        let gui_mode = (detected_gui_home || embedded_gui_binary) && !explicit_cli_binary;
        if gui_mode && codex_home.message == "Auto-detected" {
            codex_home.message = "Desktop app data detected".into();
        }
        let integration_mode = if gui_mode {
            IntegrationMode::Gui
        } else if binary.is_some() {
            IntegrationMode::Cli
        } else {
            IntegrationMode::Unknown
        };
        let codex_executable = if gui_mode {
            DiscoveryStatus::not_required("Not required for desktop app")
        } else {
            detected_codex_executable
        };
        let app_server = discovery::app_server_status_for_mode(binary.is_some(), gui_mode);
        let configured = home.is_some() && (binary.is_some() || gui_mode);
        AppStatus {
            state: if configured {
                AppStatusState::Detecting
            } else {
                AppStatusState::NeedsSetup
            },
            label: if configured {
                "Detecting"
            } else {
                "Needs setup"
            }
            .into(),
            detail: if configured {
                if gui_mode {
                    "Desktop Mode · reading local data"
                } else {
                    "CLI Mode · checking App Server"
                }
            } else {
                "Local Mode"
            }
            .into(),
            integration_mode,
            account_state: AccountState::Unknown,
            connection_quality: ConnectionQuality::Unknown,
            plan: None,
            reset_at: None,
            last_updated_at: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_millis() as i64)
                    .unwrap_or_default(),
            ),
            codex_home,
            codex_executable,
            app_server,
            data_quality: DataQuality::Unknown,
        }
    }
}

fn parse_range(value: &str) -> Result<Range, String> {
    match value.to_ascii_uppercase().as_str() {
        "1D" => Ok(Range::D1),
        "1W" => Ok(Range::W1),
        "1M" => Ok(Range::M1),
        "3M" => Ok(Range::M3),
        "6M" => Ok(Range::M6),
        _ => Err("unsupported history range".into()),
    }
}

#[tauri::command]
fn get_current_quote(state: State<'_, AppState>) -> Result<Option<models::CurrentQuote>, String> {
    state
        .database
        .lock()
        .map_err(|_| "database writer is unavailable".to_string())?
        .latest_quote()
}

#[tauri::command]
fn get_current_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    Ok(state.discover_status())
}

#[tauri::command]
fn get_history(
    state: State<'_, AppState>,
    range: String,
) -> Result<models::HistoryResponse, String> {
    state
        .database
        .lock()
        .map_err(|_| "database reader is unavailable".to_string())?
        .history(parse_range(&range)?)
}

#[tauri::command]
fn get_annotations(state: State<'_, AppState>) -> Result<Vec<models::Annotation>, String> {
    state
        .database
        .lock()
        .map_err(|_| "database reader is unavailable".to_string())?
        .annotations()
}

#[tauri::command]
fn reset_annotations(state: State<'_, AppState>) -> Result<(), String> {
    state
        .database
        .lock()
        .map_err(|_| "database writer is unavailable".to_string())?
        .reset_annotations()
}

#[tauri::command]
fn get_diagnostics_summary(
    state: State<'_, AppState>,
) -> Result<models::DiagnosticsSummary, String> {
    state
        .database
        .lock()
        .map_err(|_| "database reader is unavailable".to_string())?
        .diagnostics()
}

#[tauri::command]
fn retry_detection(state: State<'_, AppState>) -> Result<AppStatus, String> {
    state.reconcile()?;
    Ok(state.discover_status())
}

#[tauri::command]
fn select_codex_home(state: State<'_, AppState>) -> Result<RedactedSelection, String> {
    let path = rfd::FileDialog::new()
        .set_title("Choose Codex data folder")
        .pick_folder();
    let Some(path) = path else {
        return Ok(RedactedSelection {
            selected: false,
            status: DiscoveryStatus::missing("No selection made"),
        });
    };
    let redacted = discovery::redact_path(&path);
    *state
        .codex_home_override
        .lock()
        .map_err(|_| "discovery state is unavailable".to_string())? = Some(path);
    state.reconcile()?;
    Ok(RedactedSelection {
        selected: true,
        status: DiscoveryStatus {
            state: models::DiscoveryState::Selected,
            redacted_location: Some(redacted),
            message: "Selected".into(),
        },
    })
}

#[tauri::command]
fn select_codex_executable(state: State<'_, AppState>) -> Result<RedactedSelection, String> {
    let path = rfd::FileDialog::new()
        .set_title("Choose Codex executable")
        .pick_file();
    let Some(path) = path else {
        return Ok(RedactedSelection {
            selected: false,
            status: DiscoveryStatus::missing("No selection made"),
        });
    };
    if !path.is_file() {
        return Ok(RedactedSelection {
            selected: false,
            status: DiscoveryStatus {
                state: models::DiscoveryState::Unsupported,
                redacted_location: None,
                message: "Selected item is not a CLI executable".into(),
            },
        });
    }
    let redacted = discovery::redact_path(&path);
    *state
        .codex_binary_override
        .lock()
        .map_err(|_| "discovery state is unavailable".to_string())? = Some(path);
    Ok(RedactedSelection {
        selected: true,
        status: DiscoveryStatus {
            state: models::DiscoveryState::Selected,
            redacted_location: Some(redacted),
            message: "Selected".into(),
        },
    })
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .database
        .lock()
        .map_err(|_| "database reader is unavailable".to_string())?
        .load_settings()
}

#[tauri::command]
fn update_settings(
    state: State<'_, AppState>,
    settings: models::AdvancedSettings,
) -> Result<AppSettings, String> {
    settings.validate()?;
    let mut database = state
        .database
        .lock()
        .map_err(|_| "database writer is unavailable".to_string())?;
    let mut current = database.load_settings()?;
    current.advanced = settings;
    database.save_settings(&current)?;
    Ok(current)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::new().expect("Nerfify could not initialize its local database");
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_current_quote,
            get_current_status,
            get_history,
            get_annotations,
            reset_annotations,
            get_diagnostics_summary,
            retry_detection,
            select_codex_home,
            select_codex_executable,
            get_settings,
            update_settings
        ])
        .run(tauri::generate_context!())
        .expect("Nerfify runtime error");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_public_ranges() {
        assert!(matches!(parse_range("1W"), Ok(Range::W1)));
        assert!(parse_range("all").is_err());
    }
}
