#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod app_server;
pub mod collector;
pub mod discovery;
pub mod estimator;
pub mod models;
pub mod parser;
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
    collection_paused: Mutex<bool>,
}

impl AppState {
    fn new() -> Result<Self, String> {
        let state = Self {
            database: Mutex::new(storage::Database::open()?),
            codex_home_override: Mutex::new(None),
            codex_binary_override: Mutex::new(None),
            collection_paused: Mutex::new(false),
        };
        let _ = state.reconcile();
        Ok(state)
    }

    fn reconcile(&self) -> Result<(), String> {
        if *self
            .collection_paused
            .lock()
            .map_err(|_| "collection state is unavailable".to_string())?
        {
            return Ok(());
        }
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
        database.persist_collection::<()>(&collection, None, None)?;
        Ok(())
    }

    fn pause_collection(&self) -> Result<(), String> {
        *self
            .collection_paused
            .lock()
            .map_err(|_| "collection state is unavailable".to_string())? = true;
        Ok(())
    }

    fn resume_collection(&self) -> Result<(), String> {
        *self
            .collection_paused
            .lock()
            .map_err(|_| "collection state is unavailable".to_string())? = false;
        Ok(())
    }

    fn baseline_collection_at_current_end(&self) -> Result<(), String> {
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
        let collection = collector::scan_codex_home_with_state(&home, &Default::default())?;
        let baseline_events = collection
            .events
            .iter()
            .filter(|event| {
                event.quota_used_percent.is_some()
                    && event
                        .quota_window_minutes
                        .is_some_and(|minutes| (minutes - 10_080.0).abs() <= 240.0)
            })
            .max_by_key(|event| event.timestamp_ms)
            .cloned()
            .into_iter()
            .collect();
        let checkpoints_only = collector::CollectionSummary {
            events: baseline_events,
            checkpoints: collection.checkpoints,
            ..Default::default()
        };
        self.database
            .lock()
            .map_err(|_| "database writer is unavailable".to_string())?
            .persist_collection::<()>(&checkpoints_only, None, None)?;
        Ok(())
    }

    fn discover_status(&self, reconciliation_failed: bool) -> AppStatus {
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
        let database = self.database.lock().ok();
        let diagnostics = database
            .as_ref()
            .and_then(|database| database.diagnostics().ok());
        let quota = database
            .as_ref()
            .and_then(|database| database.latest_quota_observation().ok())
            .flatten();
        let total_events = diagnostics
            .as_ref()
            .map(|summary| summary.total_events)
            .unwrap_or_default();
        let diagnostics_failed = diagnostics.is_none();
        let collection_failed = reconciliation_failed || diagnostics_failed;
        let (state, label, connection_quality, data_quality) =
            collection_status(configured, collection_failed, total_events);
        let mode = if gui_mode { "Desktop Mode" } else { "CLI Mode" };
        let detail = if !configured {
            "Local Mode".into()
        } else if collection_failed {
            format!("{mode} · unable to read local data")
        } else if total_events > 0 {
            format!(
                "{mode} · {total_events} usage event{} observed",
                if total_events == 1 { "" } else { "s" }
            )
        } else {
            format!("{mode} · waiting for usage")
        };
        AppStatus {
            state,
            label: label.into(),
            detail,
            integration_mode,
            account_state: if quota.is_some() {
                AccountState::Authenticated
            } else {
                AccountState::Unknown
            },
            connection_quality,
            plan: quota.as_ref().and_then(|quota| quota.plan.clone()),
            reset_at: quota.as_ref().and_then(|quota| quota.reset_at_ms),
            last_updated_at: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_millis() as i64)
                    .unwrap_or_default(),
            ),
            codex_home,
            codex_executable,
            app_server,
            data_quality,
        }
    }
}

fn collection_status(
    configured: bool,
    failed: bool,
    total_events: i64,
) -> (AppStatusState, &'static str, ConnectionQuality, DataQuality) {
    if !configured {
        return (
            AppStatusState::NeedsSetup,
            "Needs setup",
            ConnectionQuality::Offline,
            DataQuality::Unknown,
        );
    }
    if failed {
        return (
            AppStatusState::Error,
            "Unavailable",
            ConnectionQuality::Offline,
            DataQuality::Interrupted,
        );
    }
    if total_events > 0 {
        return (
            AppStatusState::Connected,
            "Connected",
            ConnectionQuality::Good,
            DataQuality::Complete,
        );
    }
    (
        AppStatusState::Settling,
        "Waiting for usage",
        ConnectionQuality::Good,
        DataQuality::Partial,
    )
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
async fn get_current_quote(
    state: State<'_, AppState>,
) -> Result<Option<models::CurrentQuote>, String> {
    state
        .database
        .lock()
        .map_err(|_| "database writer is unavailable".to_string())?
        .latest_quote()
}

#[tauri::command]
async fn get_current_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    // ponytail: polling scans all source names; replace with a file watcher if large histories
    // make the configured refresh interval measurably slow.
    let reconciliation_failed = state.reconcile().is_err();
    Ok(state.discover_status(reconciliation_failed))
}

#[tauri::command]
async fn get_history(
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
async fn get_annotations(state: State<'_, AppState>) -> Result<Vec<models::Annotation>, String> {
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
fn reset_all_data(state: State<'_, AppState>) -> Result<(), String> {
    state.pause_collection()?;
    let result = state
        .database
        .lock()
        .map_err(|_| "database writer is unavailable".to_string())?
        .reset_all_data();
    if let Err(error) = result {
        let _ = state.resume_collection();
        return Err(error);
    }
    let baseline_result = state.baseline_collection_at_current_end();
    let resume_result = state.resume_collection();
    baseline_result.and(resume_result)
}

#[tauri::command]
fn restore_graph_data(state: State<'_, AppState>) -> Result<(), String> {
    state.resume_collection()?;
    state.reconcile()
}

#[tauri::command]
async fn get_diagnostics_summary(
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
    let reconciliation_failed = state.reconcile().is_err();
    Ok(state.discover_status(reconciliation_failed))
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
async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .database
        .lock()
        .map_err(|_| "database reader is unavailable".to_string())?
        .load_settings()
}

#[tauri::command]
fn update_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    settings.validate()?;
    let mut database = state
        .database
        .lock()
        .map_err(|_| "database writer is unavailable".to_string())?;
    database.save_settings(&settings)?;
    database.rebuild_quotes()?;
    Ok(settings)
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
            reset_all_data,
            restore_graph_data,
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

    #[test]
    fn collection_status_reflects_real_scan_results() {
        assert!(matches!(
            collection_status(false, false, 0).0,
            AppStatusState::NeedsSetup
        ));
        assert!(matches!(
            collection_status(true, false, 0).0,
            AppStatusState::Settling
        ));
        assert!(matches!(
            collection_status(true, false, 1).0,
            AppStatusState::Connected
        ));
        assert!(matches!(
            collection_status(true, true, 1).0,
            AppStatusState::Error
        ));
    }
}
