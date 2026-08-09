use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct RateLimitObservation {
    pub used_percent: f64,
    pub reset_at_ms: Option<i64>,
    pub duration_minutes: f64,
    pub limit_id: Option<String>,
    pub plan: Option<String>,
}

pub const INITIALIZE_METHOD: &str = "initialize";
pub const INITIALIZED_METHOD: &str = "initialized";
pub const ACCOUNT_READ_METHOD: &str = "account/read";
pub const RATE_LIMITS_READ_METHOD: &str = "account/rateLimits/read";
pub const RATE_LIMITS_UPDATED_METHOD: &str = "account/rateLimits/updated";

pub struct AppServerSupervisor {
    pub binary: PathBuf,
    pub child: Option<Child>,
    stdout: Option<BufReader<ChildStdout>>,
    pub consecutive_failures: u32,
}

impl AppServerSupervisor {
    pub fn new(binary: PathBuf) -> Self {
        Self {
            binary,
            child: None,
            stdout: None,
            consecutive_failures: 0,
        }
    }

    pub fn launch(&mut self) -> Result<(), String> {
        let mut child = Command::new(&self.binary)
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "unable to start Codex App Server".to_string())?;
        let stdout = child
            .stdout
            .take()
            .map(BufReader::new)
            .ok_or_else(|| "Codex App Server did not expose stdout".to_string())?;
        self.child = Some(child);
        self.stdout = Some(stdout);
        self.consecutive_failures = 0;
        self.send_json(&initialize_request(1))?;
        self.send_json(&initialized_notification())?;
        Ok(())
    }

    pub fn send_json(&mut self, message: &Value) -> Result<(), String> {
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| "Codex App Server is not running".to_string())?;
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "Codex App Server stdin is unavailable".to_string())?;
        serde_json::to_writer(&mut *stdin, message)
            .map_err(|_| "unable to write App Server request".to_string())?;
        stdin
            .write_all(b"\n")
            .map_err(|_| "unable to flush App Server request".to_string())?;
        stdin
            .flush()
            .map_err(|_| "unable to flush App Server request".to_string())
    }

    pub fn read_message(&mut self) -> Result<Option<Value>, String> {
        let stdout = self
            .stdout
            .as_mut()
            .ok_or_else(|| "Codex App Server stdout is unavailable".to_string())?;
        let mut line = String::new();
        let bytes = stdout
            .read_line(&mut line)
            .map_err(|_| "unable to read App Server response".to_string())?;
        if bytes == 0 {
            return Ok(None);
        }
        serde_json::from_str(line.trim())
            .map(Some)
            .map_err(|_| "invalid App Server JSON-RPC message".to_string())
    }

    pub fn mark_failure(&mut self) -> u64 {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        restart_delay_seconds(self.consecutive_failures)
    }

    pub fn graceful_shutdown(&mut self) {
        if let Some(child) = self.child.as_mut() {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(shutdown_request().to_string().as_bytes());
                let _ = stdin.write_all(b"\n");
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child = None;
        self.stdout = None;
    }
}

pub fn json_rpc_request(method: &str, id: u64) -> Value {
    serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":{}})
}

pub fn initialize_request(id: u64) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": INITIALIZE_METHOD,
        "params": {
            "clientInfo": {"name": "nerftrack", "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {}
        }
    })
}

pub fn account_read_request(id: u64) -> Value {
    json_rpc_request(ACCOUNT_READ_METHOD, id)
}

pub fn rate_limits_read_request(id: u64) -> Value {
    json_rpc_request(RATE_LIMITS_READ_METHOD, id)
}

pub fn initialized_notification() -> Value {
    serde_json::json!({"jsonrpc":"2.0","method":INITIALIZED_METHOD,"params":{}})
}

pub fn shutdown_request() -> Value {
    json_rpc_request("shutdown", 0)
}

pub fn restart_delay_seconds(consecutive_failures: u32) -> u64 {
    2_u64.saturating_pow(consecutive_failures.min(6)).min(60)
}

fn parse_window(value: &Value, inherited_limit_id: Option<String>) -> Option<RateLimitObservation> {
    let used_percent = value
        .get("usedPercent")
        .or_else(|| value.get("used_percent"))
        .and_then(Value::as_f64)?;
    let duration_minutes = value
        .get("durationMinutes")
        .or_else(|| value.get("duration_minutes"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let reset_at_ms = value
        .get("resetAt")
        .or_else(|| value.get("reset_at"))
        .and_then(Value::as_i64)
        .map(|value| {
            if value < 10_000_000_000 {
                value * 1000
            } else {
                value
            }
        });
    Some(RateLimitObservation {
        used_percent,
        reset_at_ms,
        duration_minutes,
        limit_id: inherited_limit_id.or_else(|| {
            value
                .get("limitId")
                .or_else(|| value.get("limit_id"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        }),
        plan: value
            .get("plan")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    })
}

pub fn select_codex_rate_limit(value: &Value) -> Result<RateLimitObservation, String> {
    if let Some(codex) = value
        .get("rateLimitsByLimitId")
        .and_then(Value::as_object)
        .and_then(|map| map.get("codex"))
    {
        if let Some(observation) = parse_window(codex, Some("codex".into())) {
            return Ok(observation);
        }
    }
    let mut candidates = Vec::new();
    if let Some(windows) = value.get("rateLimits").and_then(Value::as_array) {
        for window in windows {
            if let Some(observation) = parse_window(window, None) {
                candidates.push(observation);
            }
        }
    }
    if candidates.is_empty() {
        return Err("unsupported rate-limit schema".into());
    }
    candidates.sort_by(|left, right| {
        (left.duration_minutes - 10_080.0)
            .abs()
            .total_cmp(&(right.duration_minutes - 10_080.0).abs())
    });
    let winner = candidates.remove(0);
    if (winner.duration_minutes - 10_080.0).abs() > 240.0
        || candidates.first().is_some_and(|next| {
            (next.duration_minutes - 10_080.0).abs() == (winner.duration_minutes - 10_080.0).abs()
        })
    {
        return Err("ambiguous weekly rate-limit schema".into());
    }
    Ok(winner)
}

pub fn merge_sparse_rate_limit(
    previous: Option<RateLimitObservation>,
    update: &Value,
) -> Option<RateLimitObservation> {
    let current = previous.unwrap_or(RateLimitObservation {
        used_percent: 0.0,
        reset_at_ms: None,
        duration_minutes: 10_080.0,
        limit_id: Some("codex".into()),
        plan: None,
    });
    Some(RateLimitObservation {
        used_percent: update
            .get("usedPercent")
            .or_else(|| update.get("used_percent"))
            .and_then(Value::as_f64)
            .unwrap_or(current.used_percent),
        reset_at_ms: update
            .get("resetAt")
            .or_else(|| update.get("reset_at"))
            .and_then(Value::as_i64)
            .or(current.reset_at_ms),
        duration_minutes: update
            .get("durationMinutes")
            .or_else(|| update.get("duration_minutes"))
            .and_then(Value::as_f64)
            .unwrap_or(current.duration_minutes),
        limit_id: update
            .get("limitId")
            .or_else(|| update.get("limit_id"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or(current.limit_id),
        plan: update
            .get("plan")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or(current.plan),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_codex_limit_id_when_present() {
        let value = serde_json::json!({"rateLimitsByLimitId":{"codex":{"usedPercent":34.5,"durationMinutes":10080,"resetAt":1735689600,"plan":"Plus"}}});
        let selected = select_codex_rate_limit(&value).expect("codex limit");
        assert_eq!(selected.limit_id.as_deref(), Some("codex"));
        assert_eq!(selected.used_percent, 34.5);
        assert_eq!(selected.reset_at_ms, Some(1_735_689_600_000));
    }

    #[test]
    fn rejects_ambiguous_fallback_windows() {
        let value = serde_json::json!({"rateLimits":[{"usedPercent":10,"durationMinutes":10080},{"usedPercent":20,"durationMinutes":10080}]});
        assert!(select_codex_rate_limit(&value).is_err());
    }

    #[test]
    fn restart_backoff_caps_at_one_minute() {
        assert_eq!(restart_delay_seconds(0), 1);
        assert_eq!(restart_delay_seconds(6), 60);
        assert_eq!(restart_delay_seconds(20), 60);
    }

    #[test]
    fn exposes_initialize_and_read_requests() {
        assert_eq!(initialize_request(1)["method"], INITIALIZE_METHOD);
        assert_eq!(account_read_request(2)["method"], ACCOUNT_READ_METHOD);
        assert_eq!(
            rate_limits_read_request(3)["method"],
            RATE_LIMITS_READ_METHOD
        );
    }
}
