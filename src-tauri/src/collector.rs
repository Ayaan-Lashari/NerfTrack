use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::parser::{
    event_fingerprint, parse_newline_terminated_bytes_with_state, ParseStats, ParserState,
    UsageEvent,
};

#[derive(Debug, Clone, Default)]
pub struct SourceCheckpoint {
    pub source_key: String,
    pub byte_offset: u64,
    pub source_active: bool,
    pub parser_state_json: String,
}

#[derive(Debug, Clone, Default)]
pub struct PersistedCheckpoint {
    pub byte_offset: u64,
    pub parser_state_json: String,
}

#[derive(Debug, Clone, Default)]
pub struct CollectionSummary {
    pub events: Vec<UsageEvent>,
    pub checkpoints: Vec<SourceCheckpoint>,
    pub stats: ParseStats,
    pub interrupted_sources: Vec<String>,
}

fn is_active_source(path: &Path) -> bool {
    let lower = path.to_string_lossy().to_ascii_lowercase();
    !lower.contains("archive") && !lower.contains("archived")
}

fn collect_jsonl_paths(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        fs::read_dir(directory).map_err(|_| "unable to read Codex data directory".to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let _ = collect_jsonl_paths(&path, output);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
        {
            output.push(path);
        }
    }
    Ok(())
}

fn source_key(path: &Path) -> String {
    let mut digest = Sha256::new();
    digest.update(path.to_string_lossy().as_bytes());
    format!("source_{:x}", digest.finalize())
}

pub fn scan_codex_home(
    home: &Path,
    previous: &HashMap<String, u64>,
) -> Result<CollectionSummary, String> {
    let previous_with_state = previous
        .iter()
        .map(|(key, offset)| {
            (
                key.clone(),
                PersistedCheckpoint {
                    byte_offset: *offset,
                    parser_state_json: String::new(),
                },
            )
        })
        .collect();
    scan_codex_home_with_state(home, &previous_with_state)
}

pub fn scan_codex_home_with_state(
    home: &Path,
    previous: &HashMap<String, PersistedCheckpoint>,
) -> Result<CollectionSummary, String> {
    let mut paths = Vec::new();
    collect_jsonl_paths(home, &mut paths)?;
    paths.sort_by_key(|path| (!is_active_source(path), source_key(path)));
    let mut summary = CollectionSummary::default();
    let mut seen = HashSet::new();
    for path in paths {
        let key = source_key(&path);
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => {
                summary.interrupted_sources.push(key);
                continue;
            }
        };
        let size = metadata.len();
        let previous_checkpoint = previous.get(&key);
        let requested_offset = previous_checkpoint
            .map(|checkpoint| checkpoint.byte_offset)
            .unwrap_or(0);
        let mut parser_state = previous_checkpoint
            .and_then(|checkpoint| {
                serde_json::from_str::<ParserState>(&checkpoint.parser_state_json).ok()
            })
            .unwrap_or_default();
        let offset = requested_offset.min(size);
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(_) => {
                summary.interrupted_sources.push(key);
                continue;
            }
        };
        if file.seek(SeekFrom::Start(offset)).is_err() {
            summary.interrupted_sources.push(key);
            continue;
        }
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            summary.interrupted_sources.push(key);
            continue;
        }
        let terminal_newline = bytes.last().is_some_and(|byte| *byte == b'\n');
        let next_offset = if terminal_newline {
            size
        } else {
            bytes
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map(|position| offset + position as u64 + 1)
                .unwrap_or(offset)
        };
        let (events, stats) = parse_newline_terminated_bytes_with_state(&bytes, &mut parser_state);
        summary.stats.imported_records += stats.imported_records;
        summary.stats.partial_line_retries += stats.partial_line_retries;
        summary.stats.rejected_records += stats.rejected_records;
        for event in events {
            if seen.insert(event_fingerprint(&event)) {
                summary.events.push(event);
            }
        }
        summary.checkpoints.push(SourceCheckpoint {
            source_key: key,
            byte_offset: next_offset,
            source_active: is_active_source(&path),
            parser_state_json: serde_json::to_string(&parser_state).unwrap_or_else(|_| "{}".into()),
        });
    }
    Ok(summary)
}

pub fn stats_add(left: &mut ParseStats, right: &ParseStats) {
    left.imported_records += right.imported_records;
    left.partial_line_retries += right.partial_line_retries;
    left.rejected_records += right.rejected_records;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn scans_with_byte_offsets_and_active_files_first() {
        let root = std::env::temp_dir().join(format!("nerfify-collector-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("archive")).expect("directory");
        let active = root.join("session.jsonl");
        let archived = root.join("archive/session.jsonl");
        let line = r#"{"request_id":"r1","turn_id":"t1","timestamp":1735689600,"model":"gpt-5-codex","usage":{"input_tokens":10,"output_tokens":4}}"#;
        File::create(&active)
            .expect("active")
            .write_all(format!("{line}\n").as_bytes())
            .expect("write");
        File::create(&archived)
            .expect("archived")
            .write_all(format!("{line}\n").as_bytes())
            .expect("write");
        let first = scan_codex_home(&root, &HashMap::new()).expect("scan");
        assert_eq!(first.events.len(), 1);
        assert_eq!(first.stats.imported_records, 2);
        let previous: HashMap<_, _> = first
            .checkpoints
            .iter()
            .map(|checkpoint| (checkpoint.source_key.clone(), checkpoint.byte_offset))
            .collect();
        let second = scan_codex_home(&root, &previous).expect("second scan");
        assert!(second.events.is_empty());
        assert_eq!(second.stats.imported_records, 0);
        assert!(!first.checkpoints[0]
            .source_key
            .contains(root.to_string_lossy().as_ref()));
        let _ = fs::remove_dir_all(root);
    }
}
