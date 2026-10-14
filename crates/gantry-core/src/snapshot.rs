use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{BlockerEntry, GoNoGoGate, IndexPayload, QAEntry, Signoff, get_vault_path};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrendSummary {
    pub dates: Vec<String>,
    pub scores: Vec<u8>,
    pub readiness_states: Vec<String>,
    pub blockers: Vec<usize>,
    pub qa_completion: Vec<f32>,
    pub telemetry_freshness: Vec<String>,
    pub waivers: Vec<usize>,
    pub evidence: Vec<usize>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GantryState {
    pub last_indexed_commit: String,
    pub last_snapshot: String,
    pub dispatched_events: Vec<String>,
    #[serde(default)]
    pub chain_head: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvSnapshotSummary {
    pub readiness: String,
    pub readiness_score: u8,
    pub telemetry_status: String,
    pub readiness_reasons: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GantrySnapshot {
    pub release: String,
    pub active_environment: String,
    pub readiness: String,
    pub readiness_score: u8,
    pub environments: HashMap<String, EnvSnapshotSummary>,
    pub open_blockers: usize,
    pub qa_total: usize,
    pub qa_completed: usize,
    pub blockers: Vec<BlockerEntry>,
    pub qa_suites: Vec<QAEntry>,
    pub gates: Vec<GoNoGoGate>,
    pub signoffs: Vec<Signoff>,
    pub timestamp: String,
    pub commit: String,
    #[serde(default)]
    pub waiver_count: usize,
    #[serde(default)]
    pub evidence_count: usize,
    #[serde(default)]
    pub scorecard_summary: Option<serde_json::Value>,
    #[serde(default)]
    pub release_manifest_path: Option<String>,
    /// SHA-256 of the previous snapshot's canonical JSON ("" for the first entry).
    #[serde(default)]
    pub prev_hash: String,
    /// SHA-256 of this snapshot's canonical JSON (computed with entry_hash = "").
    #[serde(default)]
    pub entry_hash: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SnapshotDiff {
    pub base_commit: String,
    pub head_commit: String,
    pub readiness_changed: Option<(String, String)>,
    pub score_changed: Option<(u8, u8)>,
    pub resolved_blockers: Vec<String>,
    pub added_blockers: Vec<String>,
    pub completed_qa_suites: Vec<String>,
    pub completed_gates: Vec<String>,
    pub completed_signoffs: Vec<String>,
}

/// Result of walking the snapshot hash chain.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChainReport {
    pub intact: bool,
    pub snapshots_checked: usize,
    /// First filename where the chain breaks, if any.
    pub broken_at: Option<String>,
    pub broken_reason: Option<String>,
}

// ── Hashing helpers ──────────────────────────────────────────────────────────

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

/// Compute the canonical hash of a snapshot: serialise with entry_hash="" so
/// the field is stable before we know our own hash.
fn canonical_hash(snap: &GantrySnapshot) -> String {
    let mut tmp = snap.clone();
    tmp.entry_hash = String::new();
    let canonical = serde_json::to_string(&tmp).unwrap_or_default();
    sha256_hex(&canonical)
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn get_current_git_commit_ctx(project_root: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(project_root)
        .output()
        .ok()?;
    if output.status.success() {
        let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !hash.is_empty() {
            return Some(hash);
        }
    }
    None
}

pub fn get_current_git_commit() -> Option<String> {
    let project_root = std::env::current_dir().unwrap_or_default();
    get_current_git_commit_ctx(&project_root)
}

pub fn get_git_commit_hash_ctx(project_root: &Path, path: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args([
            "log",
            "-1",
            "--format=%H",
            "--",
            path.to_string_lossy().as_ref(),
        ])
        .current_dir(project_root)
        .output()
        .ok()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stdout.is_empty() {
            return Some(stdout);
        }
    }
    None
}

pub fn write_snapshot_ctx(
    vault_path: &Path,
    payload: &IndexPayload,
) -> Result<Option<(String, PathBuf)>, String> {
    if !vault_path.exists() {
        return Err("Gantry vault (.gantry/) directory not found.".to_string());
    }

    let project_root = vault_path.parent().unwrap_or(vault_path);
    let commit = get_current_git_commit_ctx(project_root).unwrap_or_else(|| "unknown".to_string());
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H-%M-%SZ").to_string();

    let mut environments = HashMap::new();
    for (name, env_payload) in &payload.environment_results {
        environments.insert(
            name.clone(),
            EnvSnapshotSummary {
                readiness: env_payload.readiness.clone(),
                readiness_score: env_payload.readiness_score,
                telemetry_status: env_payload.telemetry_status.clone(),
                readiness_reasons: env_payload.readiness_reasons.clone(),
            },
        );
    }

    // Load state to get the current chain head (prev_hash for this entry).
    let state_path = vault_path.join("state.json");
    let prev_hash = if state_path.exists() {
        fs::read_to_string(&state_path)
            .ok()
            .and_then(|c| serde_json::from_str::<GantryState>(&c).ok())
            .map(|s| s.chain_head)
            .unwrap_or_default()
    } else {
        String::new()
    };

    let mut current_snapshot = GantrySnapshot {
        release: payload.release.clone(),
        active_environment: payload.active_environment.clone(),
        readiness: payload.readiness.clone(),
        readiness_score: payload.readiness_score,
        environments,
        open_blockers: payload.open_blockers,
        qa_total: payload.qa_total,
        qa_completed: payload.qa_completed,
        blockers: payload.blockers.clone(),
        qa_suites: payload.qa_suites.clone(),
        gates: payload.go_no_go_gates.clone(),
        signoffs: payload.signoffs.clone(),
        timestamp: timestamp.clone(),
        commit: commit.clone(),
        waiver_count: payload.active_waivers.len(),
        evidence_count: payload.evidence_records.len(),
        scorecard_summary: if payload.scorecard_results.is_empty() {
            None
        } else {
            Some(
                serde_json::to_value(&payload.scorecard_results).unwrap_or(serde_json::Value::Null),
            )
        },
        release_manifest_path: payload.release_manifest_path.clone(),
        prev_hash: prev_hash.clone(),
        entry_hash: String::new(),
    };

    // Compute and seal the entry hash.
    current_snapshot.entry_hash = canonical_hash(&current_snapshot);

    // Deduplicate: if nothing changed since the last indexed commit, skip.
    if state_path.exists() {
        if let Ok(state_content) = fs::read_to_string(&state_path) {
            if let Ok(state) = serde_json::from_str::<GantryState>(&state_content) {
                if state.last_indexed_commit == commit {
                    let last_snapshot_path =
                        vault_path.join("snapshots").join(&state.last_snapshot);
                    if last_snapshot_path.exists() {
                        if let Ok(snap_content) = fs::read_to_string(&last_snapshot_path) {
                            if let Ok(last_snap) =
                                serde_json::from_str::<GantrySnapshot>(&snap_content)
                            {
                                let mut comparable = last_snap;
                                comparable.timestamp = timestamp.clone();
                                comparable.entry_hash = current_snapshot.entry_hash.clone();
                                comparable.prev_hash = current_snapshot.prev_hash.clone();
                                if comparable == current_snapshot {
                                    return Ok(None);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let snapshots_dir = vault_path.join("snapshots");
    fs::create_dir_all(&snapshots_dir)
        .map_err(|e| format!("Failed to create snapshots directory: {}", e))?;

    let filename = format!("{}-{}.json", timestamp, commit);
    let snapshot_file_path = snapshots_dir.join(&filename);
    let serialized = serde_json::to_string_pretty(&current_snapshot)
        .map_err(|e| format!("Failed to serialize snapshot: {}", e))?;

    fs::write(&snapshot_file_path, serialized)
        .map_err(|e| format!("Failed to write snapshot file: {}", e))?;

    Ok(Some((filename, snapshot_file_path)))
}

pub fn write_snapshot(payload: &IndexPayload) -> Result<Option<(String, PathBuf)>, String> {
    let vault_path = get_vault_path();
    write_snapshot_ctx(&vault_path, payload)
}

pub fn update_state_after_snapshot_ctx(
    vault_path: &Path,
    filename: &str,
    commit: &str,
) -> Result<(), String> {
    let state_path = vault_path.join("state.json");

    // Read the entry_hash from the snapshot we just wrote so we can advance the chain head.
    let chain_head = {
        let snap_path = vault_path.join("snapshots").join(filename);
        fs::read_to_string(&snap_path)
            .ok()
            .and_then(|c| serde_json::from_str::<GantrySnapshot>(&c).ok())
            .map(|s| s.entry_hash)
            .unwrap_or_default()
    };

    let mut state = GantryState {
        last_indexed_commit: commit.to_string(),
        last_snapshot: filename.to_string(),
        dispatched_events: Vec::new(),
        chain_head,
    };

    if state_path.exists() {
        if let Ok(content) = fs::read_to_string(&state_path) {
            if let Ok(existing) = serde_json::from_str::<GantryState>(&content) {
                state.dispatched_events = existing.dispatched_events;
            }
        }
    }

    let serialized = serde_json::to_string_pretty(&state)
        .map_err(|e| format!("Failed to serialize state: {}", e))?;
    fs::write(&state_path, serialized).map_err(|e| format!("Failed to write state.json: {}", e))?;

    Ok(())
}

pub fn update_state_after_snapshot(filename: &str, commit: &str) -> Result<(), String> {
    let vault_path = get_vault_path();
    update_state_after_snapshot_ctx(&vault_path, filename, commit)
}

pub fn verify_chain_ctx(vault_path: &Path) -> Result<ChainReport, String> {
    let snapshots_dir = vault_path.join("snapshots");

    if !snapshots_dir.exists() {
        return Ok(ChainReport {
            intact: true,
            snapshots_checked: 0,
            broken_at: None,
            broken_reason: None,
        });
    }

    // Collect and sort chronologically (filenames begin with UTC timestamp).
    let mut entries: Vec<(String, PathBuf)> = fs::read_dir(&snapshots_dir)
        .map_err(|e| format!("Failed to read snapshots dir: {}", e))?
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("json") {
                let name = p.file_name()?.to_string_lossy().to_string();
                Some((name, p))
            } else {
                None
            }
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut prev_entry_hash = String::new();
    let mut checked = 0;

    for (filename, path) in &entries {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", filename, e))?;
        let snap: GantrySnapshot = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {}", filename, e))?;

        // Old snapshots without hash fields are silently skipped.
        if snap.entry_hash.is_empty() {
            prev_entry_hash = String::new();
            checked += 1;
            continue;
        }

        // Verify prev_hash link.
        if snap.prev_hash != prev_entry_hash {
            return Ok(ChainReport {
                intact: false,
                snapshots_checked: checked,
                broken_at: Some(filename.clone()),
                broken_reason: Some(format!(
                    "prev_hash mismatch: expected '{}', found '{}'",
                    prev_entry_hash, snap.prev_hash
                )),
            });
        }

        // Verify entry_hash.
        let expected = canonical_hash(&snap);
        if snap.entry_hash != expected {
            return Ok(ChainReport {
                intact: false,
                snapshots_checked: checked,
                broken_at: Some(filename.clone()),
                broken_reason: Some(format!(
                    "entry_hash mismatch: file may have been tampered (expected '{}')",
                    expected
                )),
            });
        }

        prev_entry_hash = snap.entry_hash.clone();
        checked += 1;
    }

    Ok(ChainReport {
        intact: true,
        snapshots_checked: checked,
        broken_at: None,
        broken_reason: None,
    })
}

pub fn verify_chain() -> Result<ChainReport, String> {
    let vault_path = get_vault_path();
    verify_chain_ctx(&vault_path)
}

pub fn diff_snapshots(base: &GantrySnapshot, head: &GantrySnapshot) -> SnapshotDiff {
    let readiness_changed = if base.readiness != head.readiness {
        Some((base.readiness.clone(), head.readiness.clone()))
    } else {
        None
    };

    let score_changed = if base.readiness_score != head.readiness_score {
        Some((base.readiness_score, head.readiness_score))
    } else {
        None
    };

    let mut resolved_blockers = Vec::new();
    let mut added_blockers = Vec::new();

    let base_blocker_ids: HashMap<String, &BlockerEntry> =
        base.blockers.iter().map(|b| (b.id.clone(), b)).collect();
    let head_blocker_ids: HashMap<String, &BlockerEntry> =
        head.blockers.iter().map(|b| (b.id.clone(), b)).collect();

    for (id, base_b) in &base_blocker_ids {
        if base_b.status.to_lowercase() != "resolved" {
            if let Some(head_b) = head_blocker_ids.get(id) {
                if head_b.status.to_lowercase() == "resolved" {
                    resolved_blockers.push(id.clone());
                }
            } else {
                resolved_blockers.push(id.clone());
            }
        }
    }

    for (id, head_b) in &head_blocker_ids {
        if head_b.status.to_lowercase() != "resolved" && !base_blocker_ids.contains_key(id) {
            added_blockers.push(id.clone());
        }
    }

    let mut completed_qa_suites = Vec::new();
    let base_qa_suites: HashMap<String, &QAEntry> =
        base.qa_suites.iter().map(|q| (q.id.clone(), q)).collect();

    for head_q in &head.qa_suites {
        let head_pct = (head_q.completed_tests * 100)
            .checked_div(head_q.total_tests)
            .unwrap_or(0);

        if head_pct == 100 {
            if let Some(base_q) = base_qa_suites.get(&head_q.id) {
                let base_pct = (base_q.completed_tests * 100)
                    .checked_div(base_q.total_tests)
                    .unwrap_or(0);
                if base_pct < 100 {
                    completed_qa_suites.push(head_q.id.clone());
                }
            } else {
                completed_qa_suites.push(head_q.id.clone());
            }
        }
    }

    let mut completed_gates = Vec::new();
    let base_gates: HashMap<String, &GoNoGoGate> =
        base.gates.iter().map(|g| (g.name.clone(), g)).collect();

    for head_g in &head.gates {
        if head_g.status == "checked" {
            if let Some(base_g) = base_gates.get(&head_g.name) {
                if base_g.status != "checked" {
                    completed_gates.push(head_g.name.clone());
                }
            } else {
                completed_gates.push(head_g.name.clone());
            }
        }
    }

    let mut completed_signoffs = Vec::new();
    let base_signoffs: HashMap<String, &Signoff> =
        base.signoffs.iter().map(|s| (s.role.clone(), s)).collect();

    for head_s in &head.signoffs {
        if head_s.status == "Approved" {
            if let Some(base_s) = base_signoffs.get(&head_s.role) {
                if base_s.status != "Approved" {
                    completed_signoffs.push(head_s.role.clone());
                }
            } else {
                completed_signoffs.push(head_s.role.clone());
            }
        }
    }

    SnapshotDiff {
        base_commit: base.commit.clone(),
        head_commit: head.commit.clone(),
        readiness_changed,
        score_changed,
        resolved_blockers,
        added_blockers,
        completed_qa_suites,
        completed_gates,
        completed_signoffs,
    }
}

pub fn load_snapshot_by_commit_ctx(
    vault_path: &Path,
    commit_or_ref: &str,
) -> Result<GantrySnapshot, String> {
    let snapshots_dir = vault_path.join("snapshots");
    if !snapshots_dir.exists() {
        return Err(
            "Snapshots directory not found. Run 'gantry index' to generate snapshots.".to_string(),
        );
    }

    let trimmed_ref = commit_or_ref.trim();
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();

    if let Ok(entries) = fs::read_dir(snapshots_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            candidates.push((filename, path));
        }
    }

    // Newest first.
    candidates.sort_by(|a, b| b.0.cmp(&a.0));

    let mut best_match: Option<PathBuf> = None;
    for (filename, path) in candidates {
        if !filename.ends_with(".json") {
            continue;
        }
        let stem = &filename[..filename.len() - 5];
        let Some((_, commit_segment)) = stem.rsplit_once('-') else {
            continue;
        };

        if commit_segment == trimmed_ref || commit_segment.starts_with(trimmed_ref) {
            best_match = Some(path);
            break;
        }
    }

    if let Some(path) = best_match {
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read snapshot file: {}", e))?;
        let snapshot: GantrySnapshot = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse snapshot JSON: {}", e))?;
        Ok(snapshot)
    } else {
        Err(format!(
            "Error: Could not locate a snapshot matching ref '{}'. Run 'gantry index' on that ref first to generate snapshots.",
            trimmed_ref
        ))
    }
}

pub fn load_snapshot_by_commit(commit_or_ref: &str) -> Result<GantrySnapshot, String> {
    let vault_path = get_vault_path();
    load_snapshot_by_commit_ctx(&vault_path, commit_or_ref)
}

pub fn compile_trends_ctx(vault_path: &Path) -> Result<TrendSummary, String> {
    let snapshots_dir = vault_path.join("snapshots");
    if !snapshots_dir.exists() {
        return Ok(TrendSummary {
            dates: Vec::new(),
            scores: Vec::new(),
            readiness_states: Vec::new(),
            blockers: Vec::new(),
            qa_completion: Vec::new(),
            telemetry_freshness: Vec::new(),
            waivers: Vec::new(),
            evidence: Vec::new(),
        });
    }

    let mut snapshots = Vec::new();
    if let Ok(entries) = fs::read_dir(snapshots_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(snap) = serde_json::from_str::<GantrySnapshot>(&content) {
                        snapshots.push(snap);
                    }
                }
            }
        }
    }

    snapshots.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    let mut dates = Vec::new();
    let mut scores = Vec::new();
    let mut readiness_states = Vec::new();
    let mut blockers = Vec::new();
    let mut qa_completion = Vec::new();
    let mut telemetry_freshness = Vec::new();
    let mut waivers = Vec::new();
    let mut evidence = Vec::new();

    for snap in snapshots {
        dates.push(snap.timestamp.clone());
        scores.push(snap.readiness_score);
        readiness_states.push(snap.readiness.clone());
        blockers.push(snap.open_blockers);

        let pct = if snap.qa_total > 0 {
            (snap.qa_completed as f32 / snap.qa_total as f32) * 100.0
        } else {
            100.0
        };
        qa_completion.push(pct);

        let env_payload = snap.environments.get(&snap.active_environment);
        let freshness = env_payload
            .map(|e| e.telemetry_status.clone())
            .unwrap_or_else(|| "not_required".to_string());
        telemetry_freshness.push(freshness);

        waivers.push(snap.waiver_count);
        evidence.push(snap.evidence_count);
    }

    Ok(TrendSummary {
        dates,
        scores,
        readiness_states,
        blockers,
        qa_completion,
        telemetry_freshness,
        waivers,
        evidence,
    })
}

pub fn compile_trends() -> Result<TrendSummary, String> {
    let vault_path = get_vault_path();
    compile_trends_ctx(&vault_path)
}
