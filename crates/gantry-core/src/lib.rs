use chrono::{Local, NaiveDate, TimeZone, Utc};
use jsonschema::validator_for;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

pub mod graph;
pub mod mobile;
pub mod notify;
pub mod release;
pub mod snapshot;
pub mod support;
pub mod telemetry;

pub use graph::{
    GraphEdge, GraphNode, ImpactGraph, ImpactSummary, get_impact_graph, get_impact_graph_ctx,
    get_impact_summary_ctx, get_impact_summary_ex,
};
pub use notify::{
    NotificationConfig, SlackConfig, compile_slack_digest, compile_slack_digest_for_events,
};
pub use snapshot::{
    ChainReport, EnvSnapshotSummary, GantrySnapshot, GantryState, SnapshotDiff, TrendSummary,
    compile_trends, compile_trends_ctx, diff_snapshots, get_current_git_commit,
    get_current_git_commit_ctx, get_git_commit_hash_ctx, load_snapshot_by_commit,
    load_snapshot_by_commit_ctx, update_state_after_snapshot, update_state_after_snapshot_ctx,
    verify_chain, verify_chain_ctx, write_snapshot, write_snapshot_ctx,
};
pub use support::{RedactedSupportBundle, SupportBundleResult, write_redacted_support_bundle};
pub use telemetry::{ObservedTelemetry, TelemetryObsMap, TelemetryRule, evaluate_telemetry_rules};

#[derive(Debug, Clone)]
pub struct VaultContext {
    pub vault_path: PathBuf,
    pub project_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct EvaluationOptions {
    pub vault: VaultContext,
    pub environment: Option<String>,
    pub policy_overlay: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GitUser {
    pub name: String,
    pub email: String,
}

pub fn get_git_user_ctx(project_root: &Path) -> Option<GitUser> {
    let name_output = Command::new("git")
        .args(["config", "user.name"])
        .current_dir(project_root)
        .output();
    let email_output = Command::new("git")
        .args(["config", "user.email"])
        .current_dir(project_root)
        .output();

    let name = if let Ok(out) = name_output {
        if out.status.success() {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else {
            "".to_string()
        }
    } else {
        "".to_string()
    };

    let email = if let Ok(out) = email_output {
        if out.status.success() {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else {
            "".to_string()
        }
    } else {
        "".to_string()
    };

    if name.is_empty() && email.is_empty() {
        None
    } else {
        Some(GitUser { name, email })
    }
}

pub fn resolve_existing_vault(explicit_path: Option<&Path>) -> Result<VaultContext, String> {
    let vault_path = if let Some(path) = explicit_path {
        let abs_path = if path.is_relative() {
            std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?
                .join(path)
        } else {
            path.to_path_buf()
        };

        if !abs_path.exists() {
            return Err(format!(
                "Explicit vault path does not exist: {}",
                abs_path.display()
            ));
        }
        if !abs_path.is_dir() {
            return Err(format!(
                "Explicit vault path is not a directory: {}",
                abs_path.display()
            ));
        }
        abs_path
    } else if let Ok(env_val) = std::env::var("GANTRY_VAULT_PATH") {
        if env_val.is_empty() {
            return Err("GANTRY_VAULT_PATH environment variable is empty".to_string());
        }
        let env_path = PathBuf::from(env_val);
        let abs_path = if env_path.is_relative() {
            std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?
                .join(env_path)
        } else {
            env_path
        };
        if !abs_path.exists() {
            return Err(format!(
                "GANTRY_VAULT_PATH does not exist: {}",
                abs_path.display()
            ));
        }
        if !abs_path.is_dir() {
            return Err(format!(
                "GANTRY_VAULT_PATH is not a directory: {}",
                abs_path.display()
            ));
        }
        abs_path
    } else {
        // Upward search
        let cwd = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;
        let mut current = cwd.as_path();
        let mut resolved_path = None;
        loop {
            let candidate = current.join(".gantry");
            if candidate.is_dir() {
                resolved_path = Some(candidate);
                break;
            }
            match current.parent() {
                Some(parent) => current = parent,
                None => break,
            }
        }

        resolved_path.unwrap_or_else(|| cwd.join(".gantry"))
    };

    let project_root = vault_path
        .parent()
        .ok_or_else(|| {
            format!(
                "Vault path has no parent directory: {}",
                vault_path.display()
            )
        })?
        .to_path_buf();

    Ok(VaultContext {
        vault_path,
        project_root,
    })
}

pub fn resolve_init_target(explicit_path: Option<&Path>) -> Result<VaultContext, String> {
    let vault_path = if let Some(path) = explicit_path {
        if path.is_relative() {
            std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?
                .join(path)
        } else {
            path.to_path_buf()
        }
    } else {
        std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?
            .join(".gantry")
    };

    let project_root = vault_path
        .parent()
        .ok_or_else(|| {
            format!(
                "Vault path has no parent directory: {}",
                vault_path.display()
            )
        })?
        .to_path_buf();

    Ok(VaultContext {
        vault_path,
        project_root,
    })
}

// Resolves path relative to the active .gantry folder in the CWD
pub fn get_vault_path() -> PathBuf {
    std::env::current_dir().unwrap_or_default().join(".gantry")
}

pub fn get_default_context() -> VaultContext {
    let vault_path = get_vault_path();
    let project_root = std::env::current_dir().unwrap_or_default();
    VaultContext {
        vault_path,
        project_root,
    }
}

pub fn safe_resolve_vault_path_ctx(vault_path: &Path, rel_path: &str) -> Result<PathBuf, String> {
    let canonical_vault = if vault_path.exists() {
        vault_path
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize vault path: {}", e))?
    } else {
        vault_path.to_path_buf()
    };

    let path = Path::new(rel_path);
    if path.is_absolute() {
        return Err("Absolute paths are not allowed inside the vault".to_string());
    }

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                return Err("Path traversal (..) is not allowed inside the vault".to_string());
            }
            std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                return Err(
                    "Root or prefix components are not allowed inside the vault".to_string()
                );
            }
            _ => {}
        }
    }

    let full_path = canonical_vault.join(rel_path);
    if full_path.exists() {
        let canonical_full = full_path
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize target path: {}", e))?;
        if !canonical_full.starts_with(&canonical_vault) {
            return Err("Path resolves outside of the vault directory".to_string());
        }
    }
    Ok(full_path)
}

pub fn safe_resolve_project_path_ctx(
    project_root: &Path,
    rel_path: &str,
) -> Result<PathBuf, String> {
    let canonical_root = if project_root.exists() {
        project_root
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize workspace root: {}", e))?
    } else {
        project_root.to_path_buf()
    };

    let path = Path::new(rel_path);
    if path.is_absolute() {
        return Err("Absolute paths are not allowed".to_string());
    }

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                return Err("Path traversal (..) is not allowed".to_string());
            }
            std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                return Err("Root or prefix components are not allowed".to_string());
            }
            _ => {}
        }
    }

    let full_path = canonical_root.join(rel_path);
    if full_path.exists() {
        let canonical_full = full_path
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize target path: {}", e))?;
        if !canonical_full.starts_with(&canonical_root) {
            return Err("Path resolves outside of the project root directory".to_string());
        }
    }
    Ok(full_path)
}

pub fn safe_resolve_vault_path(rel_path: &str) -> Result<PathBuf, String> {
    let vault_path = get_vault_path();
    safe_resolve_vault_path_ctx(&vault_path, rel_path)
}

pub fn safe_resolve_project_path(rel_path: &str) -> Result<PathBuf, String> {
    let project_root = std::env::current_dir().unwrap_or_default();
    safe_resolve_project_path_ctx(&project_root, rel_path)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GantryEvidence {
    pub id: String,
    pub kind: String, // "doc", "source_file", "git_commit", "test_report", "telemetry", "signoff", "config"
    pub path: String,
    pub line: Option<usize>,
    pub message: String,
    pub severity: String, // "error" or "warning"
    pub related_finding: Option<String>,
    pub suggested_action: Option<String>,

    // V2 Additions
    pub confidence: Option<f32>,
    pub owner: Option<String>,
    pub source_commit: Option<String>,
    pub test_name: Option<String>,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
    pub remediation_task_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GantryIssue {
    pub severity: String, // "error" or "warning"
    pub file: String,
    pub code: String, // "INVALID_SCHEMA", "BROKEN_LINK", "SOURCE_MISSING", "STALE_DOC", "MISSING_DOC"
    pub message: String,
    pub field: Option<String>,
    pub evidence: Option<GantryEvidence>,
    #[serde(default)]
    pub waived: bool,
}

pub fn create_evidence(
    id: &str,
    kind: &str,
    path: &str,
    message: &str,
    severity: &str,
    related_finding: Option<&str>,
    suggested_action: Option<&str>,
) -> GantryEvidence {
    GantryEvidence {
        id: id.to_string(),
        kind: kind.to_string(),
        path: path.to_string(),
        line: None,
        message: message.to_string(),
        severity: severity.to_string(),
        related_finding: related_finding.map(|s| s.to_string()),
        suggested_action: suggested_action.map(|s| s.to_string()),
        confidence: None,
        owner: None,
        source_commit: None,
        test_name: None,
        line_start: None,
        line_end: None,
        remediation_task_id: None,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_evidence_v2(
    id: &str,
    kind: &str,
    path: &str,
    message: &str,
    severity: &str,
    related_finding: Option<&str>,
    suggested_action: Option<&str>,
    confidence: Option<f32>,
    owner: Option<&str>,
    source_commit: Option<&str>,
    test_name: Option<&str>,
    line_start: Option<usize>,
    line_end: Option<usize>,
    remediation_task_id: Option<&str>,
) -> GantryEvidence {
    GantryEvidence {
        id: id.to_string(),
        kind: kind.to_string(),
        path: path.to_string(),
        line: line_start,
        message: message.to_string(),
        severity: severity.to_string(),
        related_finding: related_finding.map(|s| s.to_string()),
        suggested_action: suggested_action.map(|s| s.to_string()),
        confidence,
        owner: owner.map(|s| s.to_string()),
        source_commit: source_commit.map(|s| s.to_string()),
        test_name: test_name.map(|s| s.to_string()),
        line_start,
        line_end,
        remediation_task_id: remediation_task_id.map(|s| s.to_string()),
    }
}

fn get_git_commit_hash(path: &Path) -> Option<String> {
    let project_root = std::env::current_dir().unwrap_or_default();
    snapshot::get_git_commit_hash_ctx(&project_root, path)
}

fn resolve_path_pattern_ctx(project_root: &Path, pattern: &str) -> Vec<PathBuf> {
    let workspace_root = project_root.to_path_buf();
    let mut results = Vec::new();
    let parts: Vec<&str> = pattern.split('/').collect();
    recursive_match(&workspace_root, &parts, 0, &mut results);

    let direct_path = workspace_root.join(pattern);
    if direct_path.exists() && direct_path.is_dir() {
        fn collect_all_files(dir: &Path, results: &mut Vec<PathBuf>) {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        results.push(path);
                    } else if path.is_dir() {
                        collect_all_files(&path, results);
                    }
                }
            }
        }
        collect_all_files(&direct_path, &mut results);
    }

    results
}

#[allow(dead_code)] // backward-compat wrapper; Tauri callers may use CWD-based resolution
fn resolve_path_pattern(pattern: &str) -> Vec<PathBuf> {
    let workspace_root = std::env::current_dir().unwrap_or_default();
    resolve_path_pattern_ctx(&workspace_root, pattern)
}

fn recursive_match(current_dir: &Path, parts: &[&str], index: usize, results: &mut Vec<PathBuf>) {
    if index >= parts.len() {
        if current_dir.is_file() {
            results.push(current_dir.to_path_buf());
        }
        return;
    }

    let part = parts[index];
    if part == "**" {
        if index + 1 < parts.len() {
            fn find_all_subdirs_and_match(
                dir: &Path,
                remaining_parts: &[&str],
                results: &mut Vec<PathBuf>,
            ) {
                recursive_match(dir, remaining_parts, 0, results);
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            find_all_subdirs_and_match(&path, remaining_parts, results);
                        }
                    }
                }
            }
            find_all_subdirs_and_match(current_dir, &parts[index + 1..], results);
        } else {
            fn collect_all_files(dir: &Path, results: &mut Vec<PathBuf>) {
                if dir.is_file() {
                    results.push(dir.to_path_buf());
                    return;
                }
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            results.push(path);
                        } else if path.is_dir() {
                            collect_all_files(&path, results);
                        }
                    }
                }
            }
            collect_all_files(current_dir, results);
        }
    } else if part.contains('*') {
        if let Ok(entries) = fs::read_dir(current_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if wildcard_match(&name, part) {
                    recursive_match(&path, parts, index + 1, results);
                }
            }
        }
    } else {
        let next_path = current_dir.join(part);
        if next_path.exists() {
            recursive_match(&next_path, parts, index + 1, results);
        }
    }
}

fn wildcard_match(s: &str, pattern: &str) -> bool {
    fn match_helper(s: &[char], p: &[char]) -> bool {
        if p.is_empty() {
            return s.is_empty();
        }
        if p[0] == '*' {
            if p.len() == 1 {
                return true;
            }
            for i in 0..=s.len() {
                if match_helper(&s[i..], &p[1..]) {
                    return true;
                }
            }
            return false;
        }
        if s.is_empty() {
            return false;
        }
        if p[0] == s[0] {
            return match_helper(&s[1..], &p[1..]);
        }
        false
    }
    let s_vec: Vec<char> = s.chars().collect();
    let p_vec: Vec<char> = pattern.chars().collect();
    match_helper(&s_vec, &p_vec)
}

fn parse_comment_metadata(
    comment: &str,
) -> (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
) {
    let mut id = String::new();
    let mut source = String::new();
    let mut severity = String::new();
    let mut evidence = String::new();
    let mut created_at = String::new();
    let mut status = String::new();
    let mut reason = None;

    let keys = vec![
        "gantry:id:",
        "source:",
        "severity:",
        "evidence:",
        "created_at:",
        "status:",
        "reason:",
    ];

    let get_val = |key: &str| -> Option<String> {
        if let Some(pos) = comment.find(key) {
            let start = pos + key.len();
            let mut end = comment.len();
            for other_key in &keys {
                if *other_key != key {
                    if let Some(other_pos) = comment[start..].find(other_key) {
                        let absolute_other_pos = start + other_pos;
                        if absolute_other_pos < end {
                            end = absolute_other_pos;
                        }
                    }
                }
            }
            Some(comment[start..end].trim().to_string())
        } else {
            None
        }
    };

    if let Some(val) = get_val("gantry:id:") {
        id = val;
    }
    if let Some(val) = get_val("source:") {
        source = val;
    }
    if let Some(val) = get_val("severity:") {
        severity = val;
    }
    if let Some(val) = get_val("evidence:") {
        evidence = val;
    }
    if let Some(val) = get_val("created_at:") {
        created_at = val;
    }
    if let Some(val) = get_val("status:") {
        status = val;
    }
    if let Some(val) = get_val("reason:") {
        reason = Some(val);
    }

    (id, source, severity, evidence, created_at, status, reason)
}

fn is_task_resolved_or_ignored(gantry_id: &str, tasks_content: &str) -> bool {
    let target = format!("gantry:id:{}", gantry_id);
    for line in tasks_content.lines() {
        if line.contains(&target) {
            if line.trim().starts_with("- [x]") {
                return true;
            }
            if let Some(start_comment) = line.find("<!--") {
                if let Some(end_comment) = line[start_comment..].find("-->") {
                    let comment = &line[start_comment + 4..start_comment + end_comment];
                    let (_, _, _, _, _, status, _) = parse_comment_metadata(comment);
                    if status == "ignored" || status == "manually_completed" || status == "fixed" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn update_task_comment(line: &str, new_status: &str, new_reason: Option<&str>) -> String {
    if let Some(start_comment) = line.find("<!--") {
        if let Some(end_comment) = line[start_comment..].find("-->") {
            let actual_end = start_comment + end_comment;
            let comment_content = &line[start_comment + 4..actual_end];

            let (id, source, severity, evidence, created_at, _, _) =
                parse_comment_metadata(comment_content);

            let status = new_status.to_string();
            let reason = new_reason.map(|s| s.to_string());

            let mut new_comment = format!(
                "<!-- gantry:id:{} source:{} severity:{} evidence:{} created_at:{}",
                id, source, severity, evidence, created_at
            );
            if !status.is_empty() {
                new_comment.push_str(&format!(" status:{}", status));
            }
            if let Some(ref r) = reason {
                new_comment.push_str(&format!(" reason:{}", r));
            }
            new_comment.push_str(" -->");

            let mut new_line = line[..start_comment].to_string();
            new_line.push_str(&new_comment);
            new_line.push_str(&line[actual_end + 3..]);
            return new_line;
        }
    }
    line.to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GoNoGoGate {
    pub name: String,
    pub status: String, // "checked" / "partial" / "unchecked"
    pub owner: String,
    pub line_index: usize,
    #[serde(default)]
    pub waived: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Signoff {
    pub name: String,
    pub role: String,
    pub status: String, // "Approved" / "In Progress" / "Pending"
    pub date: String,
    #[serde(default)]
    pub waived: bool,
}

// Blocker frontmatter properties mapping to blocker.schema.json
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BlockerEntry {
    pub path: String,
    pub id: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub owner: String,
    pub source_files: Vec<String>,
    pub stale: bool,
    pub last_verified: String,
    #[serde(default)]
    pub waived: bool,
}

// QA frontmatter properties mapping to qa.schema.json
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct QAEntry {
    pub path: String,
    pub id: String,
    pub title: String,
    pub owner: String,
    pub total_tests: usize,
    pub completed_tests: usize,
    pub stale: bool,
    pub last_verified: String,
    pub source_files: Vec<String>,
    #[serde(default)]
    pub waived: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Feat,
    Fix,
    Docs,
    Refactor,
    Chore,
    Misc,
}

impl ChangeKind {
    pub fn from_message(message: &str) -> Self {
        let msg = message.trim();
        if msg.is_empty() {
            return ChangeKind::Misc;
        }

        // Split by first ':' to find the prefix
        if let Some((prefix, _)) = msg.split_once(':') {
            // The prefix might contain scope like feat(ui) or breaking mark like feat! or feat(ui)!
            // Strip any trailing '!' first
            let mut prefix = prefix.trim();
            if prefix.ends_with('!') {
                prefix = &prefix[..prefix.len() - 1];
            }
            // If there's a '(' and ')', strip the scope: e.g. "feat(ui)" -> "feat"
            if let Some(open_idx) = prefix.find('(') {
                if prefix[open_idx..].contains(')') {
                    prefix = &prefix[..open_idx];
                }
            }

            // Standardize to lowercase
            let prefix_lower = prefix.trim().to_lowercase();
            match prefix_lower.as_str() {
                "feat" | "feature" => ChangeKind::Feat,
                "fix" | "bug" | "hotfix" => ChangeKind::Fix,
                "docs" | "doc" | "vault" | "file modified" => ChangeKind::Docs,
                "refactor" => ChangeKind::Refactor,
                "chore" | "ci" | "build" | "test" | "style" | "perf" | "revert" => {
                    ChangeKind::Chore
                }
                _ => ChangeKind::Misc,
            }
        } else {
            // No colon. Let's look for fallback matches within the message
            let msg_lower = msg.to_lowercase();
            if msg_lower.starts_with("file modified") {
                ChangeKind::Docs
            } else if msg_lower.contains("fix") || msg_lower.contains("bug") {
                ChangeKind::Fix
            } else if msg_lower.contains("feat") {
                ChangeKind::Feat
            } else if msg_lower.contains("doc") {
                ChangeKind::Docs
            } else {
                ChangeKind::Misc
            }
        }
    }
}

// Dynamic launch index payload mapping to index.json requirements
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RecentChange {
    pub hash: String,
    pub date: String,
    pub message: String,
    pub author: String,
    pub kind: ChangeKind,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AuditPreview {
    pub documents_to_create: Vec<String>,
    pub tasks_to_create: Vec<String>,
    pub tasks_to_auto_resolve: Vec<String>,
    pub affected_files: Vec<String>,
}

// ImpactSummary is now in graph.rs

// Serde defaults for profile configuration fallbacks
pub fn default_qa_completion() -> Option<usize> {
    Some(100)
}
pub fn default_block_on_p0() -> bool {
    true
}
pub fn default_require_signoffs() -> bool {
    true
}
pub fn default_max_observed_age_hours() -> Option<u64> {
    Some(24)
}
pub fn default_block_on_missing_telemetry() -> bool {
    true
}
pub fn default_env_str() -> String {
    "production".to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentProfile {
    #[serde(default = "default_qa_completion")]
    pub required_qa_completion: Option<usize>,
    #[serde(default = "default_block_on_p0")]
    pub block_on_p0: bool,
    pub min_crash_free_rate: Option<f64>,
    #[serde(default = "default_require_signoffs")]
    pub require_signoffs: bool,
    #[serde(default = "default_max_observed_age_hours")]
    pub max_observed_age_hours: Option<u64>,
    pub block_on_missing_telemetry: Option<bool>,
    #[serde(default)]
    pub telemetry_rules: Vec<TelemetryRule>,
}

// ObservedTelemetry and TelemetryObsMap are now in telemetry.rs

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnvironmentPayload {
    pub profile: EnvironmentProfile,
    pub readiness: String,
    pub readiness_score: u8,
    pub readiness_reasons: Vec<String>,
    pub category_scores: std::collections::HashMap<String, u8>,
    pub score_reasons: std::collections::HashMap<String, Vec<String>>,
    pub telemetry_status: String, // "fresh" | "stale" | "missing" | "invalid" | "not_required"
    pub observed_telemetry: Option<ObservedTelemetry>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SentryConfig {
    pub organization: String,
    pub project: String,
    pub project_id: Option<String>,
    pub target_crash_free_rate: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RevenueCatConfig {
    pub project: String,
    pub offerings: Vec<String>,
}

/// Represents the core configuration of a Gantry launch vault (`config.json`).
/// Contains metadata, environments, telemetry metrics, and notification setups.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GantryConfig {
    pub vault_name: String,
    pub version: String,
    pub release_date: String,
    #[serde(default = "default_env_str")]
    pub default_environment: String,
    #[serde(default)]
    pub environments: std::collections::HashMap<String, EnvironmentProfile>,
    pub sentry: Option<SentryConfig>,
    pub revenue_cat: Option<RevenueCatConfig>,
    pub notifications: Option<NotificationConfig>,
}

/// The unified payload summarizing the current launch-readiness state of the project.
/// This includes blockers, QA completion status, sign-offs, telemetry status, scorecards,
/// and metadata for the active environment.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IndexPayload {
    pub vault_name: String,
    pub release: String,
    pub readiness: String,
    pub target_date: String,
    pub open_blockers: usize,
    pub qa_total: usize,
    pub qa_completed: usize,
    pub stale_docs: Vec<String>,
    pub missing_docs: Vec<String>,
    pub blockers: Vec<BlockerEntry>,
    pub qa_suites: Vec<QAEntry>,
    pub last_updated: String,

    // Added fields
    pub validation_errors: Vec<GantryIssue>,
    pub broken_links: Vec<GantryIssue>,
    pub source_missing: Vec<GantryIssue>,
    pub docs_health_status: String, // "healthy", "warnings", "critical"
    pub go_no_go_gates: Vec<GoNoGoGate>,
    pub signoffs: Vec<Signoff>,
    pub readiness_reasons: Vec<String>,

    pub readiness_score: u8,
    pub category_scores: std::collections::HashMap<String, u8>,
    pub evidence_graph: Vec<GantryEvidence>,
    pub score_reasons: std::collections::HashMap<String, Vec<String>>,

    pub audit_preview: Option<AuditPreview>,
    pub impact_summary: Option<ImpactSummary>,
    pub report_url: Option<String>,

    // Phase 1 Multi-Environment fields
    pub active_environment: String,
    pub environment_results: std::collections::HashMap<String, EnvironmentPayload>,

    // New Local Gantry Capabilities
    pub active_waivers: Vec<Waiver>,
    pub expired_waivers: Vec<Waiver>,
    pub waived_items: Vec<WaivedItem>,
    pub scorecard_results: Vec<ScorecardRuleResult>,
    pub evidence_records: Vec<EvidenceRecord>,
    pub release_manifest_path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct WaivedItem {
    pub target_type: String, // "blocker", "qa", "document", "gate", "validation"
    pub target_id: Option<String>,
    pub target_path: Option<String>,
    pub target_code: Option<String>,
    pub reason: String,
    pub approved_by: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Waiver {
    pub id: String,
    pub target_type: String, // "blocker", "qa", "document", "gate", "validation"
    pub target_id: Option<String>,
    pub target_path: Option<String>,
    pub target_code: Option<String>,
    pub reason: String,
    pub approved_by: String,
    pub created_at: String,
    pub expires_at: String,
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EvidenceRecord {
    pub id: String,
    pub kind: String, // "test_report", "security_scan", "telemetry_pull", "approval", "artifact", "app_store_build", "ci_run", "manual_note"
    pub title: String,
    pub source: String,
    pub path: Option<String>,
    pub url: Option<String>,
    pub artifact_sha: Option<String>,
    pub commit: Option<String>,
    pub environment: Option<String>,
    pub release: Option<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub attachments: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ScorecardRule {
    pub id: String,
    pub category: String,
    pub title: String,
    pub severity: String, // "error", "warning"
    pub weight: u8,
    pub predicate: String,
    pub params: Option<serde_json::Value>,
    pub failure_message: String,
    pub suggested_action: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ScorecardRuleResult {
    pub rule_id: String,
    pub title: String,
    pub category: String,
    pub severity: String,
    pub weight: u8,
    pub passed: bool,
    pub value: Option<serde_json::Value>,
    pub failure_message: Option<String>,
    pub suggested_action: Option<String>,
    pub waived: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ReleaseManifest {
    pub release: String,
    pub commit: String,
    pub commit_range: Option<String>,
    pub created_at: String,
    pub environments: Vec<String>,
    pub artifact_sha: Option<String>,
    pub build_url: Option<String>,
    pub included_blockers: Vec<String>,
    pub completed_qa: Vec<String>,
    pub signoffs: Vec<String>,
    pub waivers: Vec<String>,
    pub evidence_ids: Vec<String>,
    pub readiness: String,
    pub readiness_score: u8,
}

// Real YAML Parser for frontmatter
pub fn parse_frontmatter(content: &str) -> Result<(JsonValue, String), String> {
    let trimmed_start = content.trim_start();
    if trimmed_start.starts_with("---") {
        if let Some(first_line_end) = trimmed_start.find('\n') {
            let rest = &trimmed_start[first_line_end + 1..];
            let delimiter = if rest.contains("\r\n---") {
                "\r\n---"
            } else {
                "\n---"
            };
            if let Some(pos) = rest.find(delimiter) {
                let yaml_str = &rest[..pos];
                let body_start = pos + delimiter.len();
                let body = rest[body_start..].to_string();
                let parsed: JsonValue = yaml_serde::from_str(yaml_str)
                    .map_err(|e| format!("YAML parsing error: {}", e))?;
                return Ok((parsed, body));
            }
        }
    }
    Ok((
        JsonValue::Object(serde_json::Map::new()),
        content.to_string(),
    ))
}

// Helper to check file modified times via Git or filesystem compared to last_verified
pub fn check_file_freshness_ctx(
    vault: &VaultContext,
    last_verified_str: &str,
    source_file_rel: &str,
) -> Result<bool, String> {
    let src_path = safe_resolve_project_path_ctx(&vault.project_root, source_file_rel)?;
    if !src_path.exists() {
        return Err(format!("Source file not found: {}", source_file_rel));
    }

    // Parse last_verified date (expected YYYY-MM-DD)
    let naive_date =
        NaiveDate::parse_from_str(last_verified_str.trim(), "%Y-%m-%d").map_err(|e| {
            format!(
                "Failed to parse verification date '{}': {}",
                last_verified_str, e
            )
        })?;
    let verified_midnight = naive_date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| format!("Verification date '{last_verified_str}' has no valid midnight"))?;

    // Prefer the calendar date recorded in the Git commit. Converting an epoch
    // timestamp through the runner's timezone can shift a late local commit into
    // the next UTC day and incorrectly mark same-day verification as stale.
    if let Some(commit_date) = get_git_commit_date_ctx(&vault.project_root, &src_path) {
        return Ok(commit_date > naive_date);
    }

    // Fallback to standard filesystem modification times (local date comparison)
    if let Ok(meta) = fs::metadata(&src_path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH) {
                let file_ts = duration.as_secs();
                let file_local_date = Utc
                    .timestamp_opt(file_ts as i64, 0)
                    .single()
                    .map(|dt| dt.with_timezone(&Local).date_naive());
                if let Some(file_date) = file_local_date {
                    return Ok(file_date > naive_date);
                }
                let ver_ts = Utc.from_utc_datetime(&verified_midnight).timestamp() as u64;
                return Ok(file_ts > ver_ts);
            }
        }
    }

    Err(format!(
        "Unable to determine modification time for source file: {}",
        source_file_rel
    ))
}

pub fn check_file_freshness(
    last_verified_str: &str,
    source_file_rel: &str,
) -> Result<bool, String> {
    let ctx = get_default_context();
    check_file_freshness_ctx(&ctx, last_verified_str, source_file_rel)
}

fn get_git_commit_date_ctx(project_root: &Path, path: &Path) -> Option<NaiveDate> {
    let output = Command::new("git")
        .args([
            "log",
            "-1",
            "--format=%cI",
            "--",
            path.to_string_lossy().as_ref(),
        ])
        .current_dir(project_root)
        .output()
        .ok()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return parse_git_commit_date(stdout.trim());
    }
    None
}

fn parse_git_commit_date(value: &str) -> Option<NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.date_naive())
}

pub fn record_telemetry_event_ctx(vault_path: &Path, event_type: &str) {
    if !vault_path.exists() {
        return;
    }
    let config_path = vault_path.join("config.json");
    let mut opt_in = true;
    if config_path.exists() {
        if let Ok(raw_conf) = fs::read_to_string(&config_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw_conf) {
                if let Some(val) = json.get("telemetryOptIn").and_then(|v| v.as_bool()) {
                    opt_in = val;
                }
            }
        }
    }
    if !opt_in {
        return;
    }
    let telemetry_path = vault_path.join("telemetry.json");
    let mut events = match fs::read_to_string(&telemetry_path) {
        Ok(s) => serde_json::from_str::<Vec<serde_json::Value>>(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let timestamp = Utc::now().to_rfc3339();
    let new_event = serde_json::json!({
        "event": event_type,
        "timestamp": timestamp,
    });
    events.push(new_event);
    if let Ok(serialized) = serde_json::to_string_pretty(&events) {
        let _ = fs::write(telemetry_path, serialized);
    }
}

pub fn record_telemetry_event(event_type: &str) {
    let vault_path = get_vault_path();
    record_telemetry_event_ctx(&vault_path, event_type);
}

// Find markdown wiki links
fn find_wiki_links(content: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut start = 0;
    while let Some(pos) = content[start..].find("[[") {
        let actual_pos = start + pos;
        if let Some(end_pos) = content[actual_pos..].find("]]") {
            let actual_end = actual_pos + end_pos;
            let target = &content[actual_pos + 2..actual_end];
            links.push(target.trim().to_string());
            start = actual_end + 2;
        } else {
            break;
        }
    }
    links
}

fn find_matching_waiver(
    target_type: &str,
    target_id: Option<&str>,
    target_path: Option<&str>,
    target_code: Option<&str>,
    active_waivers: &[Waiver],
) -> Option<Waiver> {
    for w in active_waivers {
        if w.target_type != target_type {
            continue;
        }

        let id_match = match (&w.target_id, target_id) {
            (Some(a), Some(b)) => a == b,
            (None, _) => true,
            _ => false,
        };

        let path_match = match (&w.target_path, target_path) {
            (Some(a), Some(b)) => a == b,
            (None, _) => true,
            _ => false,
        };

        let code_match = match (&w.target_code, target_code) {
            (Some(a), Some(b)) => a == b,
            (None, _) => true,
            _ => false,
        };

        if id_match && path_match && code_match {
            return Some(w.clone());
        }
    }
    None
}

// Dynamic JSON Schema Validation helper
fn validate_file_schema(
    vault_path: &Path,
    file_rel_path: &str,
    frontmatter: &JsonValue,
    schema_name: &str,
    issues: &mut Vec<GantryIssue>,
    tasks_content: &str,
) {
    let schema_path = vault_path
        .join("schema")
        .join(format!("{}.schema.json", schema_name));
    let remediation_id = format!("validation-err-{}", file_rel_path);
    let is_ignored = is_task_resolved_or_ignored(&remediation_id, tasks_content);
    let default_severity = if is_ignored { "warning" } else { "error" };

    if !schema_path.exists() {
        issues.push(GantryIssue {
            severity: "warning".to_string(),
            file: file_rel_path.to_string(),
            code: "SCHEMA_MISSING".to_string(),
            message: format!(
                "Schema validation skipped because schema/{}.schema.json does not exist.",
                schema_name
            ),
            field: None,
            evidence: Some(create_evidence_v2(
                &format!("schema-missing-{}", schema_name),
                "config",
                &format!("schema/{}.schema.json", schema_name),
                &format!("Missing JSON schema for category: {}", schema_name),
                "warning",
                Some("SCHEMA_MISSING"),
                Some("Create the JSON schema file to enable format validation"),
                Some(1.0),
                None,
                None,
                None,
                None,
                None,
                Some(&remediation_id),
            )),
            waived: false,
        });
        return;
    }

    let schema_str = match fs::read_to_string(&schema_path) {
        Ok(s) => s,
        Err(e) => {
            issues.push(GantryIssue {
                severity: default_severity.to_string(),
                file: file_rel_path.to_string(),
                code: "SCHEMA_READ_FAILED".to_string(),
                message: format!("Failed to read schema file: {}", e),
                field: None,
                evidence: Some(create_evidence_v2(
                    &format!("schema-read-failed-{}", schema_name),
                    "config",
                    &format!("schema/{}.schema.json", schema_name),
                    "Failed to read JSON schema file",
                    default_severity,
                    Some("SCHEMA_READ_FAILED"),
                    Some("Ensure the schema file exists and is readable"),
                    Some(1.0),
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(&remediation_id),
                )),
                waived: false,
            });
            return;
        }
    };

    let schema_json: JsonValue = match serde_json::from_str(&schema_str) {
        Ok(j) => j,
        Err(e) => {
            issues.push(GantryIssue {
                severity: default_severity.to_string(),
                file: file_rel_path.to_string(),
                code: "SCHEMA_PARSE_FAILED".to_string(),
                message: format!("Failed to parse schema JSON: {}", e),
                field: None,
                evidence: Some(create_evidence_v2(
                    &format!("schema-parse-failed-{}", schema_name),
                    "config",
                    &format!("schema/{}.schema.json", schema_name),
                    "JSON schema file contains syntax errors",
                    default_severity,
                    Some("SCHEMA_PARSE_FAILED"),
                    Some("Verify that the schema contains valid JSON syntax"),
                    Some(1.0),
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(&remediation_id),
                )),
                waived: false,
            });
            return;
        }
    };

    let compiled = match validator_for(&schema_json) {
        Ok(c) => c,
        Err(e) => {
            issues.push(GantryIssue {
                severity: default_severity.to_string(),
                file: file_rel_path.to_string(),
                code: "SCHEMA_COMPILE_FAILED".to_string(),
                message: format!("Failed to compile JSON schema: {}", e),
                field: None,
                evidence: Some(create_evidence_v2(
                    &format!("schema-compile-failed-{}", schema_name),
                    "config",
                    &format!("schema/{}.schema.json", schema_name),
                    "JSON schema is not valid Draft-7 standard schema",
                    default_severity,
                    Some("SCHEMA_COMPILE_FAILED"),
                    Some("Correct JSON schema constraints to comply with standards"),
                    Some(1.0),
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(&remediation_id),
                )),
                waived: false,
            });
            return;
        }
    };

    for err in compiled.iter_errors(frontmatter) {
        let instance_path = err.instance_path.to_string();
        let field_opt = if instance_path.is_empty() {
            None
        } else {
            Some(instance_path)
        };
        let msg_str = err.to_string();
        issues.push(GantryIssue {
            severity: default_severity.to_string(),
            file: file_rel_path.to_string(),
            code: "INVALID_SCHEMA".to_string(),
            message: msg_str.clone(),
            field: field_opt,
            evidence: Some(create_evidence_v2(
                &format!(
                    "invalid-schema-{}-{}",
                    file_rel_path,
                    msg_str.replace(" ", "-")
                ),
                "doc",
                file_rel_path,
                &msg_str,
                default_severity,
                Some("INVALID_SCHEMA"),
                Some("Edit frontmatter fields to match schema definitions"),
                Some(1.0),
                None,
                None,
                None,
                None,
                None,
                Some(&remediation_id),
            )),
            waived: false,
        });
    }
}

// Parse go/no-go gates and signoffs from markdown body
fn parse_gates_and_signoffs(content: &str) -> (Vec<GoNoGoGate>, Vec<Signoff>) {
    let mut gates = Vec::new();
    let mut signoffs = Vec::new();

    for (line_index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("- [ ")
            || trimmed.starts_with("- [x]")
            || trimmed.starts_with("- [/]")
        {
            let status_char = &trimmed[3..4];
            let status = match status_char {
                "x" => "checked",
                "/" => "partial",
                _ => "unchecked",
            };

            let mut role = "General".to_string();
            if let Some(start_bold) = trimmed.find("**") {
                if let Some(end_bold) = trimmed[start_bold + 2..].find("**") {
                    let extracted = &trimmed[start_bold + 2..start_bold + 2 + end_bold];
                    role = extracted.trim_end_matches(':').trim().to_string();
                }
            }

            let mut owner = "@unassigned".to_string();
            let mut date = "".to_string();
            if let Some(start_comment) = trimmed.find("<!--") {
                if let Some(end_comment) = trimmed[start_comment..].find("-->") {
                    let comment_content = &trimmed[start_comment + 4..start_comment + end_comment];
                    let comment_trimmed = comment_content.trim();
                    let parts: Vec<&str> = comment_trimmed.split_whitespace().collect();
                    if !parts.is_empty() {
                        // Strip "owner:" prefix if present in the stakeholder comment (e.g. <!-- owner:@amcdev1 -->)
                        let mut p = parts[0];
                        if let Some(stripped) = p.strip_prefix("owner:") {
                            p = stripped;
                        }
                        owner = p.to_string();
                        if parts.len() > 1 {
                            date = parts[1].to_string();
                        }
                    }
                }
            }

            // Name is everything after the checkbox
            let name = trimmed[5..].trim().to_string();

            gates.push(GoNoGoGate {
                name,
                status: status.to_string(),
                owner: owner.clone(),
                line_index,
                waived: false,
            });

            let signoff_status = match status_char {
                "x" => "Approved",
                "/" => "In Progress",
                _ => "Pending",
            };

            signoffs.push(Signoff {
                name: owner,
                role,
                status: signoff_status.to_string(),
                date,
                waived: false,
            });
        }
    }

    (gates, signoffs)
}

/// Compiles the launch readiness index for a given environment profile.
///
/// This parses all blockers, QA plans, config rules, and evaluates Sentry/Datadog telemetry
/// and scorecard predicates to compute a final readiness state ("Go", "No-Go", "At Risk") and score.
pub fn compile_launch_index_ctx(opts: &EvaluationOptions) -> Result<IndexPayload, String> {
    let vault_path = &opts.vault.vault_path;
    let project_root = &opts.vault.project_root;
    let active_env = opts.environment.as_deref();
    if !vault_path.exists() {
        return Err("Gantry vault (.gantry/) directory not found.".to_string());
    }

    let tasks_path = vault_path.join("tasks.md");
    let tasks_content = if tasks_path.exists() {
        fs::read_to_string(&tasks_path).unwrap_or_default()
    } else {
        String::new()
    };

    let mut validation_errors = Vec::new();

    let mut active_waivers = Vec::new();
    let mut expired_waivers = Vec::new();
    let mut waived_items = Vec::new();
    let mut waivers: Vec<Waiver> = Vec::new();

    let waivers_path = vault_path.join("waivers.json");
    if waivers_path.exists() {
        if let Ok(raw_waivers) = fs::read_to_string(&waivers_path) {
            match serde_json::from_str::<serde_json::Value>(&raw_waivers) {
                Ok(json_value) => {
                    validate_file_schema(
                        vault_path,
                        "waivers.json",
                        &json_value,
                        "waivers",
                        &mut validation_errors,
                        &tasks_content,
                    );

                    if let Ok(w_list) = serde_json::from_value::<Vec<Waiver>>(json_value) {
                        waivers = w_list;
                    } else {
                        let remediation_id = "validation-err-waivers.json";
                        let is_ignored =
                            is_task_resolved_or_ignored(remediation_id, &tasks_content);
                        let severity = if is_ignored { "warning" } else { "error" };
                        validation_errors.push(GantryIssue {
                            severity: severity.to_string(),
                            file: "waivers.json".to_string(),
                            code: "INVALID_WAIVERS_JSON".to_string(),
                            message: "Failed to parse waivers.json as a list of Waiver objects."
                                .to_string(),
                            field: None,
                            evidence: Some(create_evidence_v2(
                                "invalid-waivers-json",
                                "config",
                                "waivers.json",
                                "Failed to parse waivers.json",
                                severity,
                                Some("INVALID_WAIVERS_JSON"),
                                Some("Fix waivers.json format"),
                                Some(1.0),
                                None,
                                None,
                                None,
                                None,
                                None,
                                Some(remediation_id),
                            )),
                            waived: false,
                        });
                    }
                }
                Err(e) => {
                    let remediation_id = "validation-err-waivers.json";
                    let is_ignored = is_task_resolved_or_ignored(remediation_id, &tasks_content);
                    let severity = if is_ignored { "warning" } else { "error" };
                    validation_errors.push(GantryIssue {
                        severity: severity.to_string(),
                        file: "waivers.json".to_string(),
                        code: "INVALID_WAIVERS_JSON".to_string(),
                        message: format!("Failed to parse waivers.json: {}", e),
                        field: None,
                        evidence: Some(create_evidence_v2(
                            "invalid-waivers-json",
                            "config",
                            "waivers.json",
                            &format!("Failed to parse waivers.json: {}", e),
                            severity,
                            Some("INVALID_WAIVERS_JSON"),
                            Some("Fix waivers.json syntax to contain valid JSON format"),
                            Some(1.0),
                            None,
                            None,
                            None,
                            None,
                            None,
                            Some(remediation_id),
                        )),
                        waived: false,
                    });
                }
            }
        }
    }

    let today_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    for w in waivers {
        if w.status == "active" && w.expires_at >= today_str {
            active_waivers.push(w);
        } else {
            expired_waivers.push(w.clone());
            let remediation_id = format!("expired-waiver-{}", w.id);
            validation_errors.push(GantryIssue {
                severity: "warning".to_string(),
                file: "waivers.json".to_string(),
                code: "WAIVER_EXPIRED".to_string(),
                message: format!("Waiver '{}' has expired on {}", w.id, w.expires_at),
                field: Some("expires_at".to_string()),
                evidence: Some(create_evidence_v2(
                    &format!("expired-waiver-{}", w.id),
                    "config",
                    "waivers.json",
                    &format!("Waiver '{}' has expired on {}", w.id, w.expires_at),
                    "warning",
                    Some("WAIVER_EXPIRED"),
                    Some("Renew or delete the expired waiver"),
                    Some(1.0),
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(&remediation_id),
                )),
                waived: false,
            });
        }
    }
    let mut broken_links = Vec::new();
    let mut source_missing = Vec::new();
    let mut stale_docs = Vec::new();
    let mut missing_docs = Vec::new();

    let mut all_wiki_links = Vec::new();
    let mut valid_targets = HashSet::new();

    // 1. Load config.json and validate its schema
    let config_path = vault_path.join("config.json");
    let mut vault_name = "Launch Vault".to_string();
    let mut version = "1.0.0".to_string();
    let mut release_date = "".to_string();
    let mut report_url = None;

    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|x| x == "--report-url") {
        if pos + 1 < args.len() {
            report_url = Some(args[pos + 1].clone());
        }
    }

    let mut gantry_config = GantryConfig {
        vault_name: vault_name.clone(),
        version: version.clone(),
        release_date: release_date.clone(),
        default_environment: "production".to_string(),
        environments: std::collections::HashMap::new(),
        sentry: None,
        revenue_cat: None,
        notifications: None,
    };

    if config_path.exists() {
        if let Ok(raw_conf) = fs::read_to_string(&config_path) {
            match serde_json::from_str::<serde_json::Value>(&raw_conf) {
                Ok(json_value) => {
                    validate_file_schema(
                        vault_path,
                        "config.json",
                        &json_value,
                        "config",
                        &mut validation_errors,
                        &tasks_content,
                    );

                    if let Ok(cfg) = serde_json::from_value::<GantryConfig>(json_value.clone()) {
                        gantry_config = cfg;
                        vault_name = gantry_config.vault_name.clone();
                        version = gantry_config.version.clone();
                        release_date = gantry_config.release_date.clone();
                    } else {
                        if let Some(name) = json_value.get("vaultName").and_then(|v| v.as_str()) {
                            vault_name = name.to_string();
                        }
                        if let Some(ver) = json_value.get("version").and_then(|v| v.as_str()) {
                            version = ver.to_string();
                        }
                        if let Some(rd) = json_value.get("releaseDate").and_then(|v| v.as_str()) {
                            release_date = rd.to_string();
                        }
                    }

                    if report_url.is_none() {
                        if let Some(url) = json_value.get("reportUrl").and_then(|v| v.as_str()) {
                            report_url = Some(url.to_string());
                        } else if let Some(url) =
                            json_value.get("report_url").and_then(|v| v.as_str())
                        {
                            report_url = Some(url.to_string());
                        }
                    }
                }
                Err(e) => {
                    return Err(format!("Failed to parse config.json: {}", e));
                }
            }
        }
    } else {
        missing_docs.push("config.json".to_string());
        let remediation_id = "missing-doc-config.json";
        let is_ignored = is_task_resolved_or_ignored(remediation_id, &tasks_content);
        let severity = if is_ignored { "warning" } else { "error" };
        validation_errors.push(GantryIssue {
            severity: severity.to_string(),
            file: "config.json".to_string(),
            code: "MISSING_DOC".to_string(),
            message: "Missing required config file: config.json".to_string(),
            field: None,
            evidence: Some(create_evidence_v2(
                "missing-config-json",
                "doc",
                "config.json",
                "Missing required config file: config.json",
                severity,
                Some("MISSING_DOC"),
                Some("Create config.json file inside .gantry/"),
                Some(1.0),
                None,
                None,
                None,
                None,
                None,
                Some(remediation_id),
            )),
            waived: false,
        });
    }

    // 2. Required/Recommended documents check
    let required_docs = vec![
        ("README-launch.md", true),
        ("go_no_go.md", true),
        ("tasks.md", true),
        ("AGENTS.md", true),
        ("release-notes.md", false),
        ("engineering-notes.md", false),
        ("runbook.md", false),
        ("postlaunch.md", false),
    ];

    for (doc, is_required) in required_docs {
        let full_doc_path = vault_path.join(doc);
        if !full_doc_path.exists() {
            missing_docs.push(doc.to_string());
            let remediation_id = format!("missing-doc-{}", doc);
            let is_ignored = is_task_resolved_or_ignored(&remediation_id, &tasks_content);
            let severity = if is_ignored {
                "warning".to_string()
            } else {
                if is_required { "error" } else { "warning" }.to_string()
            };
            validation_errors.push(GantryIssue {
                severity: severity.clone(),
                file: doc.to_string(),
                code: "MISSING_DOC".to_string(),
                message: format!(
                    "Missing {} document: {}",
                    if is_required {
                        "required"
                    } else {
                        "recommended"
                    },
                    doc
                ),
                field: None,
                evidence: Some(create_evidence_v2(
                    &format!("missing-doc-{}", doc),
                    "doc",
                    doc,
                    &format!(
                        "Missing {} document: {}",
                        if is_required {
                            "required"
                        } else {
                            "recommended"
                        },
                        doc
                    ),
                    &severity,
                    Some("MISSING_DOC"),
                    Some(&format!("Create missing document: {}", doc)),
                    Some(1.0),
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(&remediation_id),
                )),
                waived: false,
            });
        } else {
            valid_targets.insert(doc.replace(".md", ""));
            valid_targets.insert(doc.to_string());
        }
    }

    // 3. Parse blockers
    let mut blockers = Vec::new();
    let mut open_blockers = 0;
    let mut has_open_p0_blocker = false;
    let blockers_dir = vault_path.join("blockers");

    if blockers_dir.exists() && blockers_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(blockers_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                    let Some(file_name) = path.file_name() else {
                        continue;
                    };
                    let Some(file_stem) = path.file_stem() else {
                        continue;
                    };
                    let file_stem = file_stem.to_string_lossy().to_string();
                    let file_rel = format!("blockers/{}", file_name.to_string_lossy());
                    valid_targets.insert(file_stem.clone());
                    valid_targets.insert(file_rel.clone());

                    if let Ok(content) = fs::read_to_string(&path) {
                        match parse_frontmatter(&content) {
                            Ok((fm, body)) => {
                                validate_file_schema(
                                    vault_path,
                                    &file_rel,
                                    &fm,
                                    "blocker",
                                    &mut validation_errors,
                                    &tasks_content,
                                );

                                let id = fm
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| file_stem.clone());
                                valid_targets.insert(id.clone());

                                let title = fm
                                    .get("title")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("Untitled Blocker")
                                    .to_string();
                                let severity = fm
                                    .get("severity")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("P1")
                                    .to_string();
                                let status = fm
                                    .get("status")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("open")
                                    .to_string();
                                let owner = fm
                                    .get("owner")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("@unassigned")
                                    .to_string();
                                let last_verified = fm
                                    .get("last_verified")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();

                                let mut source_files = Vec::new();
                                if let Some(src_val) = fm.get("source_files") {
                                    if let Some(arr) = src_val.as_array() {
                                        for v in arr {
                                            if let Some(s) = v.as_str() {
                                                source_files.push(s.to_string());
                                            }
                                        }
                                    }
                                }

                                let mut stale = false;
                                for src in &source_files {
                                    match check_file_freshness(&last_verified, src) {
                                        Ok(is_stale) => {
                                            if is_stale {
                                                let remediation_id =
                                                    format!("stale-doc-{}", file_rel);
                                                let is_ignored = is_task_resolved_or_ignored(
                                                    &remediation_id,
                                                    &tasks_content,
                                                );
                                                if !is_ignored {
                                                    stale = true;
                                                    stale_docs.push(file_rel.clone());
                                                    let commit_hash =
                                                        safe_resolve_project_path(src)
                                                            .ok()
                                                            .and_then(|p| get_git_commit_hash(&p));
                                                    validation_errors.push(GantryIssue {
                                                            severity: "warning".to_string(),
                                                            file: file_rel.clone(),
                                                            code: "STALE_DOC".to_string(),
                                                            message: format!("Document may be out of date because source file changed: {}", src),
                                                            field: Some("source_files".to_string()),
                                                            evidence: Some(create_evidence_v2(
                                                                &format!("stale-doc-{}", src),
                                                                "source_file",
                                                                src,
                                                                &format!("Document {} may be out of date because related source file changed: {}", file_rel, src),
                                                                "warning",
                                                                Some("STALE_DOC"),
                                                                Some("Re-verify document contents and update its last_verified metadata"),
                                                                Some(1.0),
                                                                Some(&owner),
                                                                commit_hash.as_deref(),
                                                                None,
                                                                None,
                                                                None,
                                                                Some(&remediation_id),
                                                            )),
                                                            waived: false,
                                                        });
                                                }
                                            }
                                        }
                                        Err(err_msg) => {
                                            let remediation_id = format!("blocker-{}", id);
                                            let is_ignored = is_task_resolved_or_ignored(
                                                &remediation_id,
                                                &tasks_content,
                                            );
                                            let severity =
                                                if is_ignored { "warning" } else { "error" };
                                            source_missing.push(GantryIssue {
                                                    severity: severity.to_string(),
                                                    file: file_rel.clone(),
                                                    code: "SOURCE_MISSING".to_string(),
                                                    message: err_msg.clone(),
                                                    field: Some("source_files".to_string()),
                                                    evidence: Some(create_evidence_v2(
                                                        &format!("source-missing-{}", src),
                                                        "source_file",
                                                        src,
                                                        &err_msg,
                                                        severity,
                                                        Some("SOURCE_MISSING"),
                                                        Some("Ensure the source file exists in the workspace or update blocker's source_files list"),
                                                        Some(1.0),
                                                        Some(&owner),
                                                        None,
                                                        None,
                                                        None,
                                                        None,
                                                        Some(&remediation_id),
                                                    )),
                                                    waived: false,
                                                });
                                            stale = true;
                                        }
                                    }
                                }

                                let mut is_blocker_waived = false;
                                let blocker_waived_opt = find_matching_waiver(
                                    "blocker",
                                    Some(&id),
                                    Some(&file_rel),
                                    None,
                                    &active_waivers,
                                );
                                if let Some(ref w) = blocker_waived_opt {
                                    is_blocker_waived = true;
                                    waived_items.push(WaivedItem {
                                        target_type: "blocker".to_string(),
                                        target_id: Some(id.clone()),
                                        target_path: Some(file_rel.clone()),
                                        target_code: None,
                                        reason: w.reason.clone(),
                                        approved_by: w.approved_by.clone(),
                                    });
                                }

                                if status.to_lowercase() != "resolved" {
                                    let remediation_id = format!("blocker-{}", id);
                                    let is_ignored = is_task_resolved_or_ignored(
                                        &remediation_id,
                                        &tasks_content,
                                    );
                                    if !is_ignored && !is_blocker_waived {
                                        open_blockers += 1;
                                        if severity == "P0" {
                                            has_open_p0_blocker = true;
                                        }
                                    }
                                }

                                for link in find_wiki_links(&body) {
                                    all_wiki_links.push((file_rel.clone(), link));
                                }

                                blockers.push(BlockerEntry {
                                    path: file_rel,
                                    id,
                                    title,
                                    severity,
                                    status,
                                    owner,
                                    source_files,
                                    stale,
                                    last_verified,
                                    waived: is_blocker_waived,
                                });
                            }
                            Err(e) => {
                                let remediation_id = format!("validation-err-{}", file_rel);
                                let is_ignored =
                                    is_task_resolved_or_ignored(&remediation_id, &tasks_content);
                                let severity = if is_ignored { "warning" } else { "error" };
                                validation_errors.push(GantryIssue {
                                    severity: severity.to_string(),
                                    file: file_rel.clone(),
                                    code: "INVALID_FRONTMATTER".to_string(),
                                    message: format!("Invalid YAML frontmatter: {}", e),
                                    field: None,
                                    evidence: Some(create_evidence_v2(
                                        &format!("invalid-frontmatter-{}", file_rel),
                                        "doc",
                                        &file_rel,
                                        &format!("Invalid YAML frontmatter: {}", e),
                                        severity,
                                        Some("INVALID_FRONTMATTER"),
                                        Some("Fix syntax error in YAML frontmatter block"),
                                        Some(1.0),
                                        None,
                                        None,
                                        None,
                                        None,
                                        None,
                                        Some(&remediation_id),
                                    )),
                                    waived: false,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Parse QA checklists
    let mut qa_suites = Vec::new();
    let mut qa_total = 0;
    let mut qa_completed = 0;
    let qa_dir = vault_path.join("qa");

    if qa_dir.exists() && qa_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(qa_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                    let Some(file_name) = path.file_name() else {
                        continue;
                    };
                    let Some(file_stem) = path.file_stem() else {
                        continue;
                    };
                    let file_stem = file_stem.to_string_lossy().to_string();
                    let file_rel = format!("qa/{}", file_name.to_string_lossy());
                    valid_targets.insert(file_stem.clone());
                    valid_targets.insert(file_rel.clone());

                    if let Ok(content) = fs::read_to_string(&path) {
                        match parse_frontmatter(&content) {
                            Ok((fm, body)) => {
                                validate_file_schema(
                                    vault_path,
                                    &file_rel,
                                    &fm,
                                    "qa",
                                    &mut validation_errors,
                                    &tasks_content,
                                );

                                let id = fm
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| file_stem.clone());
                                valid_targets.insert(id.clone());

                                let title = fm
                                    .get("title")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("Untitled QA Plan")
                                    .to_string();
                                let owner = fm
                                    .get("owner")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("@unassigned")
                                    .to_string();

                                let mut total = 0;
                                let mut completed = 0;
                                for line in body.lines() {
                                    let t = line.trim();
                                    if t.starts_with("- [ ]")
                                        || t.starts_with("- [x]")
                                        || t.starts_with("- [/]")
                                    {
                                        total += 1;
                                        if t.starts_with("- [x]") {
                                            completed += 1;
                                        }
                                    }
                                }

                                for link in find_wiki_links(&body) {
                                    all_wiki_links.push((file_rel.clone(), link));
                                }

                                let last_verified = fm
                                    .get("last_verified")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let mut source_files = Vec::new();
                                if let Some(src_val) = fm.get("source_files") {
                                    if let Some(arr) = src_val.as_array() {
                                        for v in arr {
                                            if let Some(s) = v.as_str() {
                                                source_files.push(s.to_string());
                                            }
                                        }
                                    }
                                }

                                let mut stale = false;
                                for src in &source_files {
                                    match check_file_freshness(&last_verified, src) {
                                        Ok(is_stale) => {
                                            if is_stale {
                                                let remediation_id =
                                                    format!("stale-doc-{}", file_rel);
                                                let is_ignored = is_task_resolved_or_ignored(
                                                    &remediation_id,
                                                    &tasks_content,
                                                );
                                                if !is_ignored {
                                                    stale = true;
                                                    stale_docs.push(file_rel.clone());
                                                    let commit_hash =
                                                        safe_resolve_project_path(src)
                                                            .ok()
                                                            .and_then(|p| get_git_commit_hash(&p));
                                                    validation_errors.push(GantryIssue {
                                                            severity: "warning".to_string(),
                                                            file: file_rel.clone(),
                                                            code: "STALE_DOC".to_string(),
                                                            message: format!("Document may be out of date because source file changed: {}", src),
                                                            field: Some("source_files".to_string()),
                                                            evidence: Some(create_evidence_v2(
                                                                &format!("stale-doc-{}", src),
                                                                "source_file",
                                                                src,
                                                                &format!("Document {} may be out of date because related source file changed: {}", file_rel, src),
                                                                "warning",
                                                                Some("STALE_DOC"),
                                                                Some("Re-verify document contents and update its last_verified metadata"),
                                                                Some(1.0),
                                                                Some(&owner),
                                                                commit_hash.as_deref(),
                                                                None,
                                                                None,
                                                                None,
                                                                Some(&remediation_id),
                                                            )),
                                                            waived: false,
                                                        });
                                                }
                                            }
                                        }
                                        Err(err_msg) => {
                                            let remediation_id = format!("qa-{}", id);
                                            let is_ignored = is_task_resolved_or_ignored(
                                                &remediation_id,
                                                &tasks_content,
                                            );
                                            let severity =
                                                if is_ignored { "warning" } else { "error" };
                                            source_missing.push(GantryIssue {
                                                    severity: severity.to_string(),
                                                    file: file_rel.clone(),
                                                    code: "SOURCE_MISSING".to_string(),
                                                    message: err_msg.clone(),
                                                    field: Some("source_files".to_string()),
                                                    evidence: Some(create_evidence_v2(
                                                        &format!("source-missing-{}", src),
                                                        "source_file",
                                                        src,
                                                        &err_msg,
                                                        severity,
                                                        Some("SOURCE_MISSING"),
                                                        Some("Ensure the source file exists in the workspace or update QA's source_files list"),
                                                        Some(1.0),
                                                        Some(&owner),
                                                        None,
                                                        None,
                                                        None,
                                                        None,
                                                        Some(&remediation_id),
                                                    )),
                                                    waived: false,
                                                });
                                            stale = true;
                                        }
                                    }
                                }

                                let mut is_qa_waived = false;
                                let qa_waived_opt = find_matching_waiver(
                                    "qa",
                                    Some(&id),
                                    Some(&file_rel),
                                    None,
                                    &active_waivers,
                                );
                                if let Some(ref w) = qa_waived_opt {
                                    is_qa_waived = true;
                                    waived_items.push(WaivedItem {
                                        target_type: "qa".to_string(),
                                        target_id: Some(id.clone()),
                                        target_path: Some(file_rel.clone()),
                                        target_code: None,
                                        reason: w.reason.clone(),
                                        approved_by: w.approved_by.clone(),
                                    });
                                }

                                qa_total += total;
                                if is_qa_waived {
                                    qa_completed += total;
                                } else {
                                    qa_completed += completed;
                                }

                                qa_suites.push(QAEntry {
                                    path: file_rel,
                                    id,
                                    title,
                                    owner,
                                    total_tests: total,
                                    completed_tests: if is_qa_waived { total } else { completed },
                                    stale,
                                    last_verified,
                                    source_files,
                                    waived: is_qa_waived,
                                });
                            }
                            Err(e) => {
                                let remediation_id = format!("validation-err-{}", file_rel);
                                let is_ignored =
                                    is_task_resolved_or_ignored(&remediation_id, &tasks_content);
                                let severity = if is_ignored { "warning" } else { "error" };
                                validation_errors.push(GantryIssue {
                                    severity: severity.to_string(),
                                    file: file_rel.clone(),
                                    code: "INVALID_FRONTMATTER".to_string(),
                                    message: format!("Invalid YAML frontmatter: {}", e),
                                    field: None,
                                    evidence: Some(create_evidence_v2(
                                        &format!("invalid-frontmatter-{}", file_rel),
                                        "doc",
                                        &file_rel,
                                        &format!("Invalid YAML frontmatter: {}", e),
                                        severity,
                                        Some("INVALID_FRONTMATTER"),
                                        Some("Fix syntax error in YAML frontmatter block"),
                                        Some(1.0),
                                        None,
                                        None,
                                        None,
                                        None,
                                        None,
                                        Some(&remediation_id),
                                    )),
                                    waived: false,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // 5. Parse Go/No-Go Room
    let gng_path = vault_path.join("go_no_go.md");
    let mut target_qa_completion = 100;
    let mut go_no_go_gates = Vec::new();
    let mut signoffs = Vec::new();

    if gng_path.exists() {
        if let Ok(content) = fs::read_to_string(&gng_path) {
            match parse_frontmatter(&content) {
                Ok((fm, body)) => {
                    validate_file_schema(
                        vault_path,
                        "go_no_go.md",
                        &fm,
                        "go-no-go",
                        &mut validation_errors,
                        &tasks_content,
                    );

                    if let Some(r) = fm.get("release").and_then(|v| v.as_str()) {
                        version = r.to_string();
                    }
                    if let Some(td) = fm.get("target_date").and_then(|v| v.as_str()) {
                        release_date = td.to_string();
                    }
                    if let Some(qa_tgt) = fm.get("required_qa_completion").and_then(|v| v.as_i64())
                    {
                        target_qa_completion = qa_tgt as usize;
                    }

                    let (mut g, mut s) = parse_gates_and_signoffs(&content);
                    for gate in &mut g {
                        let gate_waived_opt = find_matching_waiver(
                            "gate",
                            Some(&gate.name),
                            Some("go_no_go.md"),
                            None,
                            &active_waivers,
                        );
                        if gate_waived_opt.is_some() {
                            gate.waived = true;
                        }
                    }
                    for signoff in &mut s {
                        let signoff_waived_opt = find_matching_waiver(
                            "gate",
                            Some(&signoff.name),
                            Some("go_no_go.md"),
                            None,
                            &active_waivers,
                        )
                        .or_else(|| {
                            find_matching_waiver(
                                "signoff",
                                Some(&signoff.name),
                                Some("go_no_go.md"),
                                None,
                                &active_waivers,
                            )
                        });
                        if signoff_waived_opt.is_some() {
                            signoff.waived = true;
                        }
                    }
                    go_no_go_gates = g;
                    signoffs = s;

                    for link in find_wiki_links(&body) {
                        all_wiki_links.push(("go_no_go.md".to_string(), link));
                    }
                }
                Err(e) => {
                    let remediation_id = "validation-err-go_no_go.md";
                    let is_ignored = is_task_resolved_or_ignored(remediation_id, &tasks_content);
                    let severity = if is_ignored { "warning" } else { "error" };
                    validation_errors.push(GantryIssue {
                        severity: severity.to_string(),
                        file: "go_no_go.md".to_string(),
                        code: "INVALID_FRONTMATTER".to_string(),
                        message: format!("Invalid YAML frontmatter: {}", e),
                        field: None,
                        evidence: Some(create_evidence_v2(
                            "invalid-frontmatter-go_no_go.md",
                            "doc",
                            "go_no_go.md",
                            &format!("Invalid YAML frontmatter: {}", e),
                            severity,
                            Some("INVALID_FRONTMATTER"),
                            Some("Fix syntax error in YAML frontmatter block of go_no_go.md"),
                            Some(1.0),
                            None,
                            None,
                            None,
                            None,
                            None,
                            Some(remediation_id),
                        )),
                        waived: false,
                    });
                }
            }
        }
    }

    let runbook_path = vault_path.join("runbook.md");
    if runbook_path.exists() {
        if let Ok(content) = fs::read_to_string(&runbook_path) {
            if let Ok((fm, body)) = parse_frontmatter(&content) {
                validate_file_schema(
                    vault_path,
                    "runbook.md",
                    &fm,
                    "runbook",
                    &mut validation_errors,
                    &tasks_content,
                );
                for link in find_wiki_links(&body) {
                    all_wiki_links.push(("runbook.md".to_string(), link));
                }
            }
        }
    }

    let metadata_dir = vault_path.join("store_metadata");
    if metadata_dir.exists() && metadata_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(metadata_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                    let Some(file_name) = path.file_name() else {
                        continue;
                    };
                    let file_rel = format!("store_metadata/{}", file_name.to_string_lossy());
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            validate_file_schema(
                                vault_path,
                                &file_rel,
                                &json,
                                "store-metadata",
                                &mut validation_errors,
                                &tasks_content,
                            );
                        }
                    }
                }
            }
        }
    }

    for (source_file, target) in all_wiki_links {
        if !valid_targets.contains(&target) {
            let remediation_id = format!("broken-link-{}", source_file);
            let is_ignored = is_task_resolved_or_ignored(&remediation_id, &tasks_content);
            let severity = if is_ignored { "warning" } else { "error" };
            broken_links.push(GantryIssue {
                severity: severity.to_string(),
                file: source_file.clone(),
                code: "BROKEN_LINK".to_string(),
                message: format!(
                    "Broken wiki link targets unknown file or ID: [[{}]]",
                    target
                ),
                field: None,
                evidence: Some(create_evidence_v2(
                    &format!("broken-link-{}-{}", source_file, target.replace("/", "-")),
                    "doc",
                    &source_file,
                    &format!("Wiki link [[{}]] is broken", target),
                    severity,
                    Some("BROKEN_LINK"),
                    Some(&format!(
                        "Update the wiki link [[{}]] or create the referenced document",
                        target
                    )),
                    Some(1.0),
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(&remediation_id),
                )),
                waived: false,
            });
        }
    }

    // Post-process validation_errors, broken_links, and source_missing for waivers
    for err in &mut validation_errors {
        let target_type = if err.code == "MISSING_DOC" {
            "document"
        } else {
            "validation"
        };
        let opt_w = find_matching_waiver(
            target_type,
            None,
            Some(&err.file),
            Some(&err.code),
            &active_waivers,
        )
        .or_else(|| {
            find_matching_waiver(
                "validation",
                err.field.as_deref(),
                Some(&err.file),
                Some(&err.code),
                &active_waivers,
            )
        });
        if let Some(w) = opt_w {
            err.waived = true;
            waived_items.push(WaivedItem {
                target_type: target_type.to_string(),
                target_id: err.field.clone(),
                target_path: Some(err.file.clone()),
                target_code: Some(err.code.clone()),
                reason: w.reason.clone(),
                approved_by: w.approved_by.clone(),
            });
        }
    }
    for err in &mut broken_links {
        if let Some(w) = find_matching_waiver(
            "validation",
            None,
            Some(&err.file),
            Some(&err.code),
            &active_waivers,
        ) {
            err.waived = true;
            waived_items.push(WaivedItem {
                target_type: "validation".to_string(),
                target_id: None,
                target_path: Some(err.file.clone()),
                target_code: Some(err.code.clone()),
                reason: w.reason.clone(),
                approved_by: w.approved_by.clone(),
            });
        }
    }
    for err in &mut source_missing {
        if let Some(w) = find_matching_waiver(
            "validation",
            None,
            Some(&err.file),
            Some(&err.code),
            &active_waivers,
        ) {
            err.waived = true;
            waived_items.push(WaivedItem {
                target_type: "validation".to_string(),
                target_id: None,
                target_path: Some(err.file.clone()),
                target_code: Some(err.code.clone()),
                reason: w.reason.clone(),
                approved_by: w.approved_by.clone(),
            });
        }
    }

    let qa_percentage = (qa_completed * 100).checked_div(qa_total).unwrap_or(100);
    let missing_required_doc = validation_errors
        .iter()
        .any(|err| err.code == "MISSING_DOC" && err.severity == "error" && !err.waived);
    let has_schema_errors = validation_errors
        .iter()
        .any(|err| err.code == "INVALID_SCHEMA" && err.severity == "error" && !err.waived);
    let has_failing_tests = validation_errors
        .iter()
        .any(|err| err.code == "FAILING_TEST" && !err.waived);

    let telemetry_path = vault_path.join("observed.json");
    let telemetry_obs_map: Option<TelemetryObsMap> = if telemetry_path.exists() {
        if let Ok(content) = fs::read_to_string(&telemetry_path) {
            serde_json::from_str::<TelemetryObsMap>(&content).ok()
        } else {
            None
        }
    } else {
        None
    };

    let mut evidence_records = Vec::new();
    let evidence_dir = vault_path.join("evidence");
    if evidence_dir.exists() && evidence_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(evidence_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                    let Some(file_name) = path.file_name() else {
                        continue;
                    };
                    let file_rel = format!("evidence/{}", file_name.to_string_lossy());
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&content)
                        {
                            validate_file_schema(
                                vault_path,
                                &file_rel,
                                &json_value,
                                "evidence",
                                &mut validation_errors,
                                &tasks_content,
                            );
                            if let Ok(rec) = serde_json::from_value::<EvidenceRecord>(json_value) {
                                evidence_records.push(rec);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut scorecard_rules = Vec::new();
    let mut has_custom_scorecard = false;
    let scorecard_path = vault_path.join("scorecard.json");
    if scorecard_path.exists() {
        if let Ok(raw_scorecard) = fs::read_to_string(&scorecard_path) {
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&raw_scorecard) {
                validate_file_schema(
                    vault_path,
                    "scorecard.json",
                    &json_value,
                    "scorecard",
                    &mut validation_errors,
                    &tasks_content,
                );
                if let Ok(rules) = serde_json::from_value::<Vec<ScorecardRule>>(json_value) {
                    scorecard_rules = rules;
                    has_custom_scorecard = true;
                }
            }
        }
    }

    // Overlay from --policy (CLI sets GANTRY_POLICY_OVERLAY to the cache path).
    // Overlay rules are merged on top of local scorecard: same id → overlay wins; new ids → appended.
    if let Ok(overlay_path) = std::env::var("GANTRY_POLICY_OVERLAY") {
        if let Ok(raw) = fs::read_to_string(&overlay_path) {
            if let Ok(overlay_rules) = serde_json::from_str::<Vec<ScorecardRule>>(&raw) {
                let overlay_ids: std::collections::HashSet<String> =
                    overlay_rules.iter().map(|r| r.id.clone()).collect();
                scorecard_rules.retain(|r| !overlay_ids.contains(&r.id));
                scorecard_rules.extend(overlay_rules);
                has_custom_scorecard = true;
            }
        }
    }

    let mut environments_to_eval = gantry_config.environments.clone();
    if environments_to_eval.is_empty() {
        let prod_profile = EnvironmentProfile {
            required_qa_completion: Some(target_qa_completion),
            block_on_p0: true,
            min_crash_free_rate: gantry_config
                .sentry
                .as_ref()
                .and_then(|s| s.target_crash_free_rate),
            require_signoffs: true,
            max_observed_age_hours: Some(24),
            block_on_missing_telemetry: Some(true),
            telemetry_rules: Vec::new(),
        };
        environments_to_eval.insert("production".to_string(), prod_profile);
    }

    // Validate explicit active_env if provided
    if let Some(explicit_env) = active_env {
        if !environments_to_eval.contains_key(explicit_env) {
            return Err(format!("Unknown environment '{}'.", explicit_env));
        }
    }

    let active_environment = active_env.map(|s| s.to_string()).unwrap_or_else(|| {
        let env_opt = gantry_config.default_environment.clone();
        if environments_to_eval.contains_key(&env_opt) {
            env_opt
        } else if environments_to_eval.contains_key("production") {
            "production".to_string()
        } else {
            environments_to_eval
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| "production".to_string())
        }
    });

    let mut active_scorecard_results = Vec::new();
    let mut environment_results = std::collections::HashMap::new();

    for (env_name, profile) in &environments_to_eval {
        let mut readiness = "Go".to_string();
        let mut readiness_reasons = Vec::new();

        if profile.block_on_p0 && has_open_p0_blocker {
            readiness = "No-Go".to_string();
            readiness_reasons.push("Launch blocked by open P0 blockers.".to_string());
        }

        if let Some(req_qa) = profile.required_qa_completion {
            if qa_percentage < req_qa {
                readiness = "No-Go".to_string();
                readiness_reasons.push(format!(
                    "QA completion ({}%) is below launch target ({}%).",
                    qa_percentage, req_qa
                ));
            }
        }

        if missing_required_doc {
            readiness = "No-Go".to_string();
            readiness_reasons
                .push("Required launch documents are missing from the vault.".to_string());
        }

        if has_schema_errors {
            readiness = "No-Go".to_string();
            readiness_reasons
                .push("Metadata schema validation failures exist on required files.".to_string());
        }

        if has_failing_tests {
            readiness = "No-Go".to_string();
            readiness_reasons.push("Failing unit or integration tests exist.".to_string());
        }

        if profile.require_signoffs {
            let mut incomplete_signoff = false;
            for gate in &go_no_go_gates {
                if gate.status != "checked" && !gate.waived {
                    incomplete_signoff = true;
                    readiness_reasons.push(format!(
                        "Required stakeholder sign-off is incomplete: {} ({})",
                        gate.owner, gate.name
                    ));
                }
            }
            if incomplete_signoff {
                readiness = "No-Go".to_string();
            }
        } else {
            readiness_reasons.push(format!(
                "Required stakeholder sign-offs bypassed by {} environment policy.",
                env_name
            ));
        }

        let mut monitoring_score = 100;
        let mut mon_reasons = Vec::new();
        let mut telemetry_status = "not_required".to_string();

        let obs_opt = telemetry_obs_map
            .as_ref()
            .and_then(|m| m.observations.get(env_name));

        let has_min_cf = profile.min_crash_free_rate.is_some();
        let has_rules = !profile.telemetry_rules.is_empty();

        if has_min_cf || has_rules {
            match obs_opt {
                None => {
                    telemetry_status = "missing".to_string();
                    let block_missing = profile
                        .block_on_missing_telemetry
                        .unwrap_or_else(|| env_name.to_lowercase() == "production");
                    if block_missing {
                        readiness = "No-Go".to_string();
                        readiness_reasons.push(
                            "Telemetry observations are missing (required by environment policy)."
                                .to_string(),
                        );
                        monitoring_score = 0;
                        mon_reasons.push(
                            "-100: Telemetry observations are missing (required).".to_string(),
                        );
                    } else {
                        if readiness == "Go" {
                            readiness = "At Risk".to_string();
                        }
                        readiness_reasons.push("Telemetry observations are missing.".to_string());
                        monitoring_score -= 50;
                        mon_reasons.push("-50: Telemetry observations are missing.".to_string());
                    }
                }
                Some(telemetry) => {
                    let mut is_stale = false;
                    let mut is_invalid = false;

                    if let Ok(fetched_time) =
                        chrono::DateTime::parse_from_rfc3339(&telemetry.fetched_at)
                    {
                        let fetched_utc = fetched_time.with_timezone(&Utc);
                        let age_hours = Utc::now().signed_duration_since(fetched_utc).num_hours();
                        if let Some(max_age) = profile.max_observed_age_hours {
                            if age_hours >= max_age as i64 {
                                is_stale = true;
                            }
                        }
                    } else {
                        is_invalid = true;
                    }

                    if is_invalid {
                        telemetry_status = "invalid".to_string();
                        readiness = "No-Go".to_string();
                        readiness_reasons
                            .push("Telemetry observations are corrupted or invalid.".to_string());
                        monitoring_score = 0;
                        mon_reasons.push("-100: Telemetry observations are invalid.".to_string());
                    } else if is_stale {
                        telemetry_status = "stale".to_string();
                        if readiness == "Go" {
                            readiness = "At Risk".to_string();
                        }
                        readiness_reasons.push(
                            "Telemetry observations are stale (older than configured max age)."
                                .to_string(),
                        );
                        monitoring_score -= 50;
                        mon_reasons.push("-50: Telemetry observations are stale.".to_string());
                    } else {
                        telemetry_status = "fresh".to_string();

                        // Evaluate legacy min_crash_free_rate first
                        if let Some(target_rate) = profile.min_crash_free_rate {
                            if let Some(cf_rate) = telemetry.crash_free_rate {
                                if cf_rate < target_rate {
                                    readiness = "No-Go".to_string();
                                    readiness_reasons.push(format!(
                                            "Sentry observed crash-free rate ({}%) is below environment threshold ({}%).",
                                            cf_rate, target_rate
                                        ));
                                    monitoring_score = 0;
                                    mon_reasons.push(format!(
                                            "-100: Sentry observed crash-free rate ({}%) is below threshold ({}%).",
                                            cf_rate, target_rate
                                        ));
                                }
                            } else {
                                // Missing crash free rate specifically
                                let block_missing = profile
                                    .block_on_missing_telemetry
                                    .unwrap_or_else(|| env_name.to_lowercase() == "production");
                                if block_missing {
                                    readiness = "No-Go".to_string();
                                    readiness_reasons.push("Sentry session telemetry is missing crash-free rate statistics (required by policy).".to_string());
                                    monitoring_score = 0;
                                    mon_reasons.push(
                                        "-100: Telemetry missing crash rate metrics (required)."
                                            .to_string(),
                                    );
                                } else {
                                    if readiness == "Go" {
                                        readiness = "At Risk".to_string();
                                    }
                                    readiness_reasons.push(
                                        "Sentry session telemetry is missing crash rate metrics."
                                            .to_string(),
                                    );
                                    monitoring_score -= 50;
                                    mon_reasons.push(
                                        "-50: Telemetry missing crash rate metrics.".to_string(),
                                    );
                                }
                            }
                        }

                        // Evaluate generalized telemetry rules
                        if has_rules {
                            evaluate_telemetry_rules(
                                &profile.telemetry_rules,
                                telemetry,
                                &mut readiness_reasons,
                                &mut mon_reasons,
                                &mut monitoring_score,
                                &mut readiness,
                            );
                        }
                    }
                }
            }
        }

        if readiness == "Go" {
            if open_blockers > 0 {
                readiness = "At Risk".to_string();
                readiness_reasons.push(format!(
                    "Launch is at risk because {} open blocker(s) remain.",
                    open_blockers
                ));
            }

            if !stale_docs.is_empty() {
                readiness = "At Risk".to_string();
                readiness_reasons.push(
                    "Launch is at risk because some files need freshness reviews.".to_string(),
                );
            }

            let missing_recommended_doc = validation_errors
                .iter()
                .any(|err| err.code == "MISSING_DOC" && err.severity == "warning");
            if missing_recommended_doc {
                readiness = "At Risk".to_string();
                readiness_reasons.push("Recommended launch documents are missing.".to_string());
            }

            if !source_missing.is_empty() {
                readiness = "At Risk".to_string();
                readiness_reasons.push(
                    "Some blocker source file references are missing from the project.".to_string(),
                );
            }
        }

        if readiness == "Go" {
            readiness_reasons
                .push("All launch gates passed and stakeholders have signed off!".to_string());
        }

        let mut category_scores = std::collections::HashMap::new();
        let mut score_reasons = std::collections::HashMap::new();

        let mut product_score: i32 = 100;
        let mut prod_reasons = Vec::new();
        for err in &validation_errors {
            let is_ignored = err
                .evidence
                .as_ref()
                .and_then(|ev| ev.remediation_task_id.as_ref())
                .map(|tid| is_task_resolved_or_ignored(tid, &tasks_content))
                .unwrap_or(false);
            if is_ignored {
                continue;
            }
            match err.code.as_str() {
                "MISSING_DOC" => {
                    if err.severity == "error" {
                        product_score -= 25;
                        prod_reasons.push(format!("-25: Missing required doc: {}", err.file));
                    } else {
                        product_score -= 10;
                        prod_reasons.push(format!("-10: Missing recommended doc: {}", err.file));
                    }
                }
                "INVALID_SCHEMA" => {
                    product_score -= 15;
                    prod_reasons.push(format!("-15: Schema validation failed in {}", err.file));
                }
                "INVALID_FRONTMATTER" => {
                    product_score -= 15;
                    prod_reasons.push(format!("-15: Invalid YAML frontmatter in {}", err.file));
                }
                _ => {}
            }
        }
        for link in &broken_links {
            let is_ignored = link
                .evidence
                .as_ref()
                .and_then(|ev| ev.remediation_task_id.as_ref())
                .map(|tid| is_task_resolved_or_ignored(tid, &tasks_content))
                .unwrap_or(false);
            if is_ignored {
                continue;
            }
            product_score -= 10;
            prod_reasons.push(format!("-10: Broken wiki link in {}", link.file));
        }
        product_score = product_score.clamp(0, 100);
        if prod_reasons.is_empty() {
            prod_reasons.push(
                "All required and recommended documentation present and formatted correctly."
                    .to_string(),
            );
        }
        score_reasons.insert("Product".to_string(), prod_reasons);

        let mut eng_score: i32 = 100;
        let mut eng_reasons = Vec::new();
        for blocker in &blockers {
            if blocker.status.to_lowercase() != "resolved" {
                let remediation_id = format!("blocker-{}", blocker.id);
                if is_task_resolved_or_ignored(&remediation_id, &tasks_content) {
                    continue;
                }
                match blocker.severity.as_str() {
                    "P0" => {
                        eng_score -= 50;
                        eng_reasons.push(format!("-50: Open P0 Blocker: {}", blocker.title));
                    }
                    "P1" => {
                        eng_score -= 25;
                        eng_reasons.push(format!("-25: Open P1 Blocker: {}", blocker.title));
                    }
                    _ => {
                        eng_score -= 10;
                        eng_reasons.push(format!("-10: Open Blocker: {}", blocker.title));
                    }
                }
            }
        }
        for src in &source_missing {
            let is_ignored = src
                .evidence
                .as_ref()
                .and_then(|ev| ev.remediation_task_id.as_ref())
                .map(|tid| is_task_resolved_or_ignored(tid, &tasks_content))
                .unwrap_or(false);
            if is_ignored {
                continue;
            }
            eng_score -= 15;
            eng_reasons.push(format!(
                "-15: Blocker source file reference is missing: {}",
                src.file
            ));
        }
        eng_score = eng_score.clamp(0, 100);
        if eng_reasons.is_empty() {
            eng_reasons.push("Zero open blockers, all code resources present.".to_string());
        }
        score_reasons.insert("Engineering".to_string(), eng_reasons);

        let qa_score = (qa_completed * 100).checked_div(qa_total).unwrap_or(100);
        let mut qa_reasons = Vec::new();
        if qa_total > 0 {
            qa_reasons.push(format!(
                "QA checklist progress: {}% ({}/{} tests passed).",
                qa_score, qa_completed, qa_total
            ));
            if qa_completed < qa_total {
                qa_reasons.push(format!(
                    "-{}: incomplete regression test checklist items.",
                    100 - qa_score
                ));
            }
        } else {
            qa_reasons.push("No regression tests configured.".to_string());
        }
        score_reasons.insert("QA".to_string(), qa_reasons);

        let mut store_score: i32 = 100;
        let mut store_reasons = Vec::new();
        for err in &validation_errors {
            let is_ignored = err
                .evidence
                .as_ref()
                .and_then(|ev| ev.remediation_task_id.as_ref())
                .map(|tid| is_task_resolved_or_ignored(tid, &tasks_content))
                .unwrap_or(false);
            if is_ignored {
                continue;
            }
            if err.file.starts_with("store_metadata/") {
                store_score -= 25;
                store_reasons.push(format!(
                    "-25: Store metadata validation error in {}",
                    err.file
                ));
            }
        }
        store_score = store_score.clamp(0, 100);
        if store_reasons.is_empty() {
            store_reasons
                .push("Store marketing copy and assets are configured correctly.".to_string());
        }
        score_reasons.insert("Store".to_string(), store_reasons);

        let mut billing_score: i32 = 100;
        let mut bill_reasons = Vec::new();
        let config_has_rc = gantry_config.revenue_cat.is_some();
        if config_has_rc {
            let has_billing_blocker = blockers.iter().any(|b| {
                let remediation_id = format!("blocker-{}", b.id);
                let is_ignored = is_task_resolved_or_ignored(&remediation_id, &tasks_content);
                !is_ignored
                    && b.status.to_lowercase() != "resolved"
                    && (b.title.to_lowercase().contains("billing")
                        || b.title.to_lowercase().contains("revenuecat")
                        || b.title.to_lowercase().contains("purchase")
                        || b.title.to_lowercase().contains("paywall"))
            });
            if has_billing_blocker {
                billing_score -= 50;
                bill_reasons.push(
                    "-50: Open blocker related to RevenueCat/Billing payment flow.".to_string(),
                );
            }
        }
        billing_score = billing_score.clamp(0, 100);
        if bill_reasons.is_empty() {
            bill_reasons
                .push("In-app purchase settings and entitlement logic verified.".to_string());
        }
        score_reasons.insert("Billing".to_string(), bill_reasons);

        monitoring_score = monitoring_score.clamp(0, 100);
        if mon_reasons.is_empty() {
            mon_reasons.push("Sentry session telemetry and crash monitoring active.".to_string());
        }
        score_reasons.insert("Monitoring".to_string(), mon_reasons);

        let mut support_score: i32 = 100;
        let mut sup_reasons = Vec::new();
        let is_runbook_missing_ignored =
            is_task_resolved_or_ignored("missing-doc-runbook.md", &tasks_content);
        if !vault_path.join("runbook.md").exists() && !is_runbook_missing_ignored {
            support_score -= 50;
            sup_reasons.push("-50: Missing launch day runbook.md document.".to_string());
        }
        for err in &validation_errors {
            let is_ignored = err
                .evidence
                .as_ref()
                .and_then(|ev| ev.remediation_task_id.as_ref())
                .map(|tid| is_task_resolved_or_ignored(tid, &tasks_content))
                .unwrap_or(false);
            if is_ignored {
                continue;
            }
            if err.file == "runbook.md" {
                support_score -= 20;
                sup_reasons.push("-20: Formatting or validation issues in runbook.md".to_string());
            }
        }
        support_score = support_score.clamp(0, 100);
        if sup_reasons.is_empty() {
            sup_reasons
                .push("Launch runbook and post-launch operation guides are ready.".to_string());
        }
        score_reasons.insert("Support".to_string(), sup_reasons);

        let total_gates = go_no_go_gates.len();
        let checked_gates = go_no_go_gates
            .iter()
            .filter(|g| g.status == "checked" || g.waived)
            .count();
        let gate_pct = (checked_gates * 100)
            .checked_div(total_gates)
            .unwrap_or(100);

        let mut readiness_score = ((eng_score as f32 * 0.30)
            + (qa_score as f32 * 0.30)
            + (product_score as f32 * 0.20)
            + (gate_pct as f32 * 0.20))
            .round() as u8;

        let mut cap_reasons = Vec::new();
        if readiness == "No-Go" {
            let uncapped_score = readiness_score;
            readiness_score = readiness_score.min(49);
            if uncapped_score > 49 {
                cap_reasons.push(format!(
                        "Score capped at 49 (uncapped calculation was {}%) because launch status is No-Go.",
                        uncapped_score
                    ));
            }
        }
        score_reasons.insert("Cap".to_string(), cap_reasons);

        if has_custom_scorecard {
            let mut custom_product_score = 100;
            let mut custom_eng_score = 100;
            let mut custom_qa_score = 100;
            let mut custom_store_score = 100;
            let mut custom_billing_score = 100;
            let mut custom_monitoring_score = 100;
            let mut custom_support_score = 100;

            let mut custom_score_reasons = std::collections::HashMap::new();
            custom_score_reasons.insert("Product".to_string(), Vec::new());
            custom_score_reasons.insert("Engineering".to_string(), Vec::new());
            custom_score_reasons.insert("QA".to_string(), Vec::new());
            custom_score_reasons.insert("Store".to_string(), Vec::new());
            custom_score_reasons.insert("Billing".to_string(), Vec::new());
            custom_score_reasons.insert("Monitoring".to_string(), Vec::new());
            custom_score_reasons.insert("Support".to_string(), Vec::new());

            let mut env_scorecard_results = Vec::new();
            let mut custom_readiness = "Go".to_string();
            let mut custom_readiness_reasons = Vec::new();

            for rule in &scorecard_rules {
                let rule_waived_opt = find_matching_waiver(
                    "validation",
                    Some(&rule.id),
                    Some("scorecard.json"),
                    None,
                    &active_waivers,
                );
                let is_rule_waived = rule_waived_opt.is_some();

                let (passed, val) = match rule.predicate.as_str() {
                    "no_open_blockers" => (open_blockers == 0, serde_json::json!(open_blockers)),
                    "max_open_blockers" => {
                        let max = rule
                            .params
                            .as_ref()
                            .and_then(|p| p.get("max"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0) as usize;
                        (open_blockers <= max, serde_json::json!(open_blockers))
                    }
                    "min_qa_completion" => {
                        let min = rule
                            .params
                            .as_ref()
                            .and_then(|p| p.get("min_pct").or_else(|| p.get("min")))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(100) as usize;
                        (qa_percentage >= min, serde_json::json!(qa_percentage))
                    }
                    "required_signoff_role" => {
                        let role = rule
                            .params
                            .as_ref()
                            .and_then(|p| p.get("role"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let signoff_ok = signoffs.iter().any(|s| {
                            s.role.to_lowercase() == role.to_lowercase()
                                && (s.status == "Approved" || s.waived)
                        });
                        (signoff_ok, serde_json::json!(signoff_ok))
                    }
                    "no_missing_docs" => {
                        let count = validation_errors
                            .iter()
                            .filter(|err| {
                                err.code == "MISSING_DOC" && err.severity == "error" && !err.waived
                            })
                            .count();
                        (count == 0, serde_json::json!(count))
                    }
                    "no_validation_errors" => {
                        let count = validation_errors
                            .iter()
                            .filter(|err| {
                                err.code == "INVALID_SCHEMA"
                                    && err.severity == "error"
                                    && !err.waived
                            })
                            .count();
                        (count == 0, serde_json::json!(count))
                    }
                    "no_broken_links" => {
                        let count = broken_links.iter().filter(|err| !err.waived).count();
                        (count == 0, serde_json::json!(count))
                    }
                    "telemetry_fresh" => (
                        telemetry_status == "fresh",
                        serde_json::json!(telemetry_status),
                    ),
                    "min_crash_free_rate" => {
                        let min = rule
                            .params
                            .as_ref()
                            .and_then(|p| p.get("min_rate").or_else(|| p.get("min")))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(99.5);
                        let rate_opt = obs_opt.and_then(|t| t.crash_free_rate);
                        let ok = rate_opt.map(|r| r >= min).unwrap_or(false);
                        (ok, serde_json::json!(rate_opt))
                    }
                    "required_evidence_kind" => {
                        let kind = rule
                            .params
                            .as_ref()
                            .and_then(|p| p.get("kind"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let has_kind = evidence_records.iter().any(|e| e.kind == kind);
                        (has_kind, serde_json::json!(has_kind))
                    }
                    "max_stale_docs" => {
                        let max = rule
                            .params
                            .as_ref()
                            .and_then(|p| p.get("max"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0) as usize;
                        let count = validation_errors
                            .iter()
                            .filter(|err| err.code == "STALE_DOC" && !err.waived)
                            .count();
                        (count <= max, serde_json::json!(count))
                    }
                    _ => (false, serde_json::json!("Unknown predicate")),
                };

                if !passed && !is_rule_waived {
                    match rule.category.as_str() {
                        "Product" => {
                            custom_product_score -= rule.weight as i32;
                        }
                        "Engineering" => {
                            custom_eng_score -= rule.weight as i32;
                        }
                        "QA" => {
                            custom_qa_score -= rule.weight as i32;
                        }
                        "Store" => {
                            custom_store_score -= rule.weight as i32;
                        }
                        "Billing" => {
                            custom_billing_score -= rule.weight as i32;
                        }
                        "Monitoring" => {
                            custom_monitoring_score -= rule.weight as i32;
                        }
                        "Support" => {
                            custom_support_score -= rule.weight as i32;
                        }
                        _ => {}
                    }

                    if let Some(reasons) = custom_score_reasons.get_mut(&rule.category) {
                        reasons.push(format!(
                            "-{}: {} ({})",
                            rule.weight, rule.failure_message, rule.id
                        ));
                    }

                    if rule.severity == "error" {
                        custom_readiness = "No-Go".to_string();
                        custom_readiness_reasons.push(rule.failure_message.clone());
                    } else if rule.severity == "warning" && custom_readiness == "Go" {
                        custom_readiness = "At Risk".to_string();
                        custom_readiness_reasons.push(rule.failure_message.clone());
                    }
                } else if is_rule_waived {
                    if let Some(reasons) = custom_score_reasons.get_mut(&rule.category) {
                        reasons.push(format!("Waived: {} ({})", rule.title, rule.id));
                    }
                }

                env_scorecard_results.push(ScorecardRuleResult {
                    rule_id: rule.id.clone(),
                    title: rule.title.clone(),
                    category: rule.category.clone(),
                    severity: rule.severity.clone(),
                    weight: rule.weight,
                    passed,
                    value: Some(val),
                    failure_message: if passed {
                        None
                    } else {
                        Some(rule.failure_message.clone())
                    },
                    suggested_action: if passed {
                        None
                    } else {
                        Some(rule.suggested_action.clone())
                    },
                    waived: is_rule_waived,
                });
            }

            custom_product_score = custom_product_score.clamp(0, 100);
            custom_eng_score = custom_eng_score.clamp(0, 100);
            custom_qa_score = custom_qa_score.clamp(0, 100);
            custom_store_score = custom_store_score.clamp(0, 100);
            custom_billing_score = custom_billing_score.clamp(0, 100);
            custom_monitoring_score = custom_monitoring_score.clamp(0, 100);
            custom_support_score = custom_support_score.clamp(0, 100);

            // Overwrite category_scores
            category_scores.insert("Product".to_string(), custom_product_score as u8);
            category_scores.insert("Engineering".to_string(), custom_eng_score as u8);
            category_scores.insert("QA".to_string(), custom_qa_score as u8);
            category_scores.insert("Store".to_string(), custom_store_score as u8);
            category_scores.insert("Billing".to_string(), custom_billing_score as u8);
            category_scores.insert("Monitoring".to_string(), custom_monitoring_score as u8);
            category_scores.insert("Support".to_string(), custom_support_score as u8);

            // Add default reasons if empty
            for cat in &[
                "Product",
                "Engineering",
                "QA",
                "Store",
                "Billing",
                "Monitoring",
                "Support",
            ] {
                if let Some(reasons) = custom_score_reasons.get_mut(*cat) {
                    if reasons.is_empty() {
                        reasons.push(format!("All {} scorecard rules passed.", cat));
                    }
                }
            }
            score_reasons = custom_score_reasons;

            readiness = custom_readiness;
            readiness_reasons = custom_readiness_reasons;

            let mut custom_readiness_score = ((custom_eng_score as f32 * 0.30)
                + (custom_qa_score as f32 * 0.30)
                + (custom_product_score as f32 * 0.20)
                + (gate_pct as f32 * 0.20))
                .round() as u8;

            if readiness == "No-Go" {
                custom_readiness_score = custom_readiness_score.min(49);
            }
            readiness_score = custom_readiness_score;

            if env_name == &active_environment {
                active_scorecard_results = env_scorecard_results;
            }
        } else {
            category_scores.insert("Product".to_string(), product_score as u8);
            category_scores.insert("Engineering".to_string(), eng_score as u8);
            category_scores.insert("QA".to_string(), qa_score as u8);
            category_scores.insert("Store".to_string(), store_score as u8);
            category_scores.insert("Billing".to_string(), billing_score as u8);
            category_scores.insert("Monitoring".to_string(), monitoring_score as u8);
            category_scores.insert("Support".to_string(), support_score as u8);
        }

        let obs_telemetry = obs_opt.cloned();

        environment_results.insert(
            env_name.clone(),
            EnvironmentPayload {
                profile: profile.clone(),
                readiness,
                readiness_score,
                readiness_reasons,
                category_scores,
                score_reasons,
                telemetry_status,
                observed_telemetry: obs_telemetry,
            },
        );
    }

    let active_environment = active_env.map(|s| s.to_string()).unwrap_or_else(|| {
        let env_opt = gantry_config.default_environment.clone();
        if environment_results.contains_key(&env_opt) {
            env_opt
        } else if environment_results.contains_key("production") {
            "production".to_string()
        } else {
            environment_results
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| "production".to_string())
        }
    });

    let mut active_readiness = "Go".to_string();
    let mut active_readiness_score = 100;
    let mut active_readiness_reasons = Vec::new();
    let mut active_category_scores = std::collections::HashMap::new();
    let mut active_score_reasons = std::collections::HashMap::new();

    if let Some(active_payload) = environment_results.get(&active_environment) {
        active_readiness = active_payload.readiness.clone();
        active_readiness_score = active_payload.readiness_score;
        active_readiness_reasons = active_payload.readiness_reasons.clone();
        active_category_scores = active_payload.category_scores.clone();
        active_score_reasons = active_payload.score_reasons.clone();
    }

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| format!("System clock is before the Unix epoch: {e}"))?
        .as_secs()
        .to_string();

    let mut evidence_graph = Vec::new();
    for err in &validation_errors {
        if let Some(ref ev) = err.evidence {
            evidence_graph.push(ev.clone());
        }
    }
    for err in &broken_links {
        if let Some(ref ev) = err.evidence {
            evidence_graph.push(ev.clone());
        }
    }
    for err in &source_missing {
        if let Some(ref ev) = err.evidence {
            evidence_graph.push(ev.clone());
        }
    }

    let base_git = get_base_git_ref(project_root, None);
    let changed_files = if base_git.is_some() {
        let tag_opt = get_last_release_tag(project_root);
        if let Some(ref tag) = tag_opt {
            get_changed_files_since_tag(project_root, tag)
        } else {
            get_changed_files_recent_commits(project_root)
        }
    } else {
        Vec::new()
    };
    let impact_summary = Some(graph::compute_impact_summary(
        changed_files,
        &blockers,
        &qa_suites,
    ));

    let docs_health_status = if validation_errors.iter().any(|err| err.severity == "error")
        || !broken_links.is_empty()
    {
        "critical".to_string()
    } else if !stale_docs.is_empty()
        || !source_missing.is_empty()
        || validation_errors
            .iter()
            .any(|err| err.severity == "warning")
    {
        "warnings".to_string()
    } else {
        "healthy".to_string()
    };

    Ok(IndexPayload {
        vault_name,
        release: version.clone(),
        readiness: active_readiness,
        target_date: release_date,
        open_blockers,
        qa_total,
        qa_completed,
        stale_docs,
        missing_docs,
        blockers,
        qa_suites,
        last_updated: timestamp,
        validation_errors,
        broken_links,
        source_missing,
        docs_health_status,
        go_no_go_gates,
        signoffs,
        readiness_reasons: active_readiness_reasons,
        readiness_score: active_readiness_score,
        category_scores: active_category_scores,
        evidence_graph,
        score_reasons: active_score_reasons,
        audit_preview: None,
        impact_summary,
        report_url,
        active_environment,
        environment_results,
        active_waivers,
        expired_waivers,
        waived_items,
        scorecard_results: active_scorecard_results,
        evidence_records,
        release_manifest_path: {
            let manifest_rel = format!("releases/{}.json", version);
            if vault_path.join(&manifest_rel).exists() {
                Some(manifest_rel)
            } else {
                None
            }
        },
    })
}

pub fn compile_launch_index_for_environment(
    active_env: Option<&str>,
) -> Result<IndexPayload, String> {
    let ctx = resolve_existing_vault(None)?;
    let opts = EvaluationOptions {
        vault: ctx,
        environment: active_env.map(|s| s.to_string()),
        policy_overlay: None,
    };
    compile_launch_index_ctx(&opts)
}

pub fn compile_launch_index() -> Result<IndexPayload, String> {
    compile_launch_index_for_environment(None)
}

/// Compiles and writes the launch readiness payload to `.gantry/index.json`.
/// Also creates or updates the corresponding release manifest version history.
pub fn write_launch_index_ctx(opts: &EvaluationOptions) -> Result<IndexPayload, String> {
    let mut payload = compile_launch_index_ctx(opts)?;
    let vault_path = &opts.vault.vault_path;
    let project_root = &opts.vault.project_root;

    // Write Release Manifest
    let commit =
        snapshot::get_current_git_commit_ctx(project_root).unwrap_or_else(|| "unknown".to_string());
    let releases_dir = vault_path.join("releases");
    if !releases_dir.exists() {
        fs::create_dir_all(&releases_dir)
            .map_err(|e| format!("Failed to create releases directory: {}", e))?;
    }

    let mut environments = vec![payload.active_environment.clone()];
    for env_key in payload.environment_results.keys() {
        if !environments.contains(env_key) {
            environments.push(env_key.clone());
        }
    }

    let mut write_manifest = true;
    let mut final_created_at = chrono::Local::now().to_rfc3339();
    let manifest_file_path = releases_dir.join(format!("{}.json", payload.release));

    let blockers_list: Vec<String> = payload.blockers.iter().map(|b| b.id.clone()).collect();
    let completed_qa_list: Vec<String> = payload
        .qa_suites
        .iter()
        .filter(|q| q.completed_tests == q.total_tests)
        .map(|q| q.id.clone())
        .collect();
    let signoffs_list: Vec<String> = payload
        .signoffs
        .iter()
        .filter(|s| s.status == "Approved" || s.status == "Approved (Waived)")
        .map(|s| format!("{}:{}", s.role, s.name))
        .collect();
    let waivers_list: Vec<String> = payload
        .active_waivers
        .iter()
        .map(|w| w.id.clone())
        .collect();
    let evidence_ids_list: Vec<String> = payload
        .evidence_records
        .iter()
        .map(|e| e.id.clone())
        .collect();

    if manifest_file_path.exists() {
        if let Ok(existing_content) = fs::read_to_string(&manifest_file_path) {
            if let Ok(existing) = serde_json::from_str::<ReleaseManifest>(&existing_content) {
                if existing.release == payload.release
                    && existing.commit == commit
                    && existing.environments == environments
                    && existing.included_blockers == blockers_list
                    && existing.completed_qa == completed_qa_list
                    && existing.signoffs == signoffs_list
                    && existing.waivers == waivers_list
                    && existing.evidence_ids == evidence_ids_list
                    && existing.readiness == payload.readiness
                    && existing.readiness_score == payload.readiness_score
                {
                    write_manifest = false;
                    final_created_at = existing.created_at;
                }
            }
        }
    }

    let manifest = ReleaseManifest {
        release: payload.release.clone(),
        commit: commit.clone(),
        commit_range: None,
        created_at: final_created_at,
        environments,
        artifact_sha: None,
        build_url: None,
        included_blockers: blockers_list,
        completed_qa: completed_qa_list,
        signoffs: signoffs_list,
        waivers: waivers_list,
        evidence_ids: evidence_ids_list,
        readiness: payload.readiness.clone(),
        readiness_score: payload.readiness_score,
    };

    if write_manifest {
        let manifest_serialized = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("Failed to serialize release manifest: {}", e))?;
        fs::write(&manifest_file_path, manifest_serialized)
            .map_err(|e| format!("Failed to write release manifest: {}", e))?;
    }

    payload.release_manifest_path = Some(format!("releases/{}.json", payload.release));

    let index_file_path = vault_path.join("index.json");
    let serialized = serde_json::to_string_pretty(&payload)
        .map_err(|e| format!("Failed to serialize index: {}", e))?;
    fs::write(index_file_path, serialized)
        .map_err(|e| format!("Failed to write index.json: {}", e))?;

    // Create local snapshot & update state file
    if let Some((filename, _path)) = write_snapshot_ctx(vault_path, &payload)? {
        update_state_after_snapshot_ctx(vault_path, &filename, &commit)?;
    }

    Ok(payload)
}

pub fn write_launch_index_for_environment(env: Option<&str>) -> Result<IndexPayload, String> {
    let ctx = resolve_existing_vault(None)?;
    let opts = EvaluationOptions {
        vault: ctx,
        environment: env.map(|s| s.to_string()),
        policy_overlay: None,
    };
    write_launch_index_ctx(&opts)
}

pub fn write_launch_index() -> Result<IndexPayload, String> {
    write_launch_index_for_environment(None)
}

// --- Starter templates constants for gantry init ---
const CONFIG_TEMPLATE: &str = r#"{
  "vaultName": "My Awesome App Release Vault",
  "version": "1.0.0-beta.3",
  "releaseDate": "2026-06-15",
  "defaultEnvironment": "production",
  "environments": {
    "staging": {
      "requiredQaCompletion": 80,
      "blockOnP0": true,
      "minCrashFreeRate": null,
      "requireSignoffs": false,
      "maxObservedAgeHours": 24,
      "blockOnMissingTelemetry": false
    },
    "production": {
      "requiredQaCompletion": 100,
      "blockOnP0": true,
      "minCrashFreeRate": 99.5,
      "requireSignoffs": true,
      "maxObservedAgeHours": 24,
      "blockOnMissingTelemetry": true
    }
  },
  "sentry": {
    "organization": "my-org",
    "project": "mobile-app",
    "targetCrashFreeRate": 99.5
  },
  "revenueCat": {
    "project": "mobile-app-iap",
    "offerings": ["monthly_pro", "yearly_pro"]
  }
}"#;

const GO_NO_GO_TEMPLATE: &str = r#"---
type: go_no_go
release: 1.0.0-beta.3
target_date: 2026-06-15
required_crash_free_rate: 99.5
required_qa_completion: 100
status: at_risk
---

# Gantry Release Gate: Version 1.0.0-beta.3

This is the official Go/No-Go matrix for the upcoming release. Stakeholders must verify their respective paths and check them off below.

## Release Readiness Gating Checklist

- [ ] **Product:** App Store / Google Play metadata and copy updated. <!-- @pm_dan -->
- [ ] **Engineering:** Zero P0 blockers remaining. Sentry crash-free sessions above 99.5%. <!-- @dev_jane -->
- [ ] **QA:** Regression checklist completed and verified on Release Candidate. <!-- @qa_bob -->
- [ ] **Marketing:** Press release scheduled and email list prepared. <!-- @mkt_alice -->
- [ ] **Billing:** RevenueCat subscription paywalls verified in sandbox. <!-- @pm_dan -->

---

## Decision Dashboard

* **Target Launch Date:** 2026-06-15
* **Current Blockers:** 1 Open (See [[BLK-101-purchase-crash]])
* **Current Crash-free Sessions:** 99.8% (Pass)
* **QA Progress:** 0/7 tests passed
"#;

const TASKS_TEMPLATE: &str = r#"# Gantry Shared Work Queue (tasks.md)

This list contains high-priority tasks that both human team members and AI assistants coordinate on. Update checklist boxes or append task details.

## Launch Setup Checklist

- [ ] Configure Sentry organization and release identifiers in config.json. <!-- owner:@dev_jane priority:P0 -->
- [ ] Check in the validation schemas and AGENTS.md guide. <!-- owner:@dev_jane priority:P0 -->
- [ ] Setup Sentry webhook or automated API token checking. <!-- owner:@dev_jane priority:P1 -->
- [ ] Double-check localized store listings for Spanish (es-ES). <!-- owner:@pm_dan priority:P1 -->
- [ ] Prepare post-launch dashboard alerts for monitoring. <!-- owner:@pm_dan priority:P2 -->

---

## Workspace Documentation Review Tasks

- [ ] Complete engineering risk notes (engineering-notes.md). <!-- owner:@dev_jane priority:P1 -->
- [ ] Finalize launch-day runbook sequence (runbook.md). <!-- owner:@pm_dan priority:P1 -->
- [ ] Draft client-facing release notes (release-notes.md). <!-- owner:@pm_dan priority:P2 -->
"#;

const AGENTS_TEMPLATE: &str = r#"# Gantry AI Agent Contract (AGENTS.md)

Welcome, AI Coding Assistant! This repository uses **Gantry**, a local-first launch tracker and AI-native release workspace. Please adhere to this contract when reading or updating launch documentation.

---

## 1. Vault Directory Mappings

All launch configurations, blocker issues, and checklists live in the `.gantry/` folder:

- `.gantry/AGENTS.md`: This instruction guide. **[DO NOT MODIFY]**
- `.gantry/index.json`: Dynamically compiled index cache. **[DO NOT MODIFY - AUTO-GENERATED]**
- `.gantry/config.json`: Vault parameters and release metrics. **[EDITABLE]**
- `.gantry/tasks.md`: Shared task backlog list. **[EDITABLE]**
- `.gantry/schema/`: Validation schemas. **[DO NOT MODIFY]**
- `.gantry/blockers/`: Individual markdown files for code blockers. **[EDITABLE]**
- `.gantry/qa/`: Manual/automated checklist suites. **[EDITABLE]**
- `.gantry/go_no_go.md`: Gatekeeper checkmarks and sign-offs. **[EDITABLE]**

---

## 2. Formatting Rules & Schemas

Gantry relies on strict frontmatter parsing. When editing markdown files, preserve the YAML header formatting exactly.

### 2.1 Blocker Format (`.gantry/blockers/*.md`)
Must include the following YAML frontmatter keys:
- `type`: Must be `"blocker"`
- `id`: Unique blocker ID (e.g. `BLK-101`)
- `title`: Short descriptive title
- `severity`: `"P0"` (showstopper), `"P1"` (critical), or `"P2"` (minor)
- `status`: `"blocked"`, `"in_progress"`, `"open"`, or `"resolved"`
- `owner`: Author/assignee handle (e.g. `"@jane"`)
- `source_files`: Array of relative codebase paths (used for freshness checks)
- `last_verified`: Date string formatted as `YYYY-MM-DD`

Example:
```yaml
---
type: blocker
id: BLK-101
title: App crashes during monthly subscription purchase
severity: P0
status: blocked
owner: "@dev_jane"
source_files:
  - src/purchases/paywall.ts
last_verified: 2026-05-27
---
```

### 2.2 QA Checklist Format (`.gantry/qa/*.md`)
Checklist items must use standard markdown checkbox lists:
- `[ ]` for incomplete/failed tests
- `[x]` for passing tests

Example:
```md
- [ ] Verify paywall loads properly. <!-- owner:@qa_bob priority:P0 -->
```

---

## 3. Freshness Principle
When you modify a source file listed under a blocker's `source_files` property, that blocker becomes **Stale** inside Gantry.
- After resolving or checking code, you **MUST** update the `last_verified` date string to the current date and set `status` to `"resolved"` to clear Gantry's warning state.

---

## 4. Verification Commands
Run the following CLI commands to test your edits before ending your turn:
1. Re-compile index cache:
   ```bash
   npm run gantry:index
   ```
2. Verify all files are valid and schemas match:
   ```bash
   npm run gantry:validate
   ```
   *Ensure exit code is `0`.*

---

## 5. Model Context Protocol (MCP) Server Integration
Gantry exposes a read-only Model Context Protocol (MCP) server over stdio, which allows AI assistants to query the release readiness state dynamically.

### Configuration
To configure this server for Claude Desktop, add the following to your configuration file (typically `~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "gantry": {
      "command": "gantry",
      "args": ["mcp"]
    }
  }
}
```

To configure it for Cursor or other tools, add a new stdio MCP server pointing to the absolute path of the `gantry` CLI with the `mcp` argument.
"#;

const README_LAUNCH_TEMPLATE: &str = r#"# Gantry Release Overview (README-launch.md)

Welcome to Launch Control. This vault tracks release milestones, manual verification checklists, blockers, and sign-offs for our deployment.

## Getting Started

1. Check open blockers under `.gantry/blockers/`.
2. Update QA plans in `.gantry/qa/`.
3. Provide stakeholder sign-offs in `.gantry/go_no_go.md`.
4. Run validation using `npm run gantry:validate`.
"#;

const CONFIG_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Gantry Configuration Schema",
  "type": "object",
  "properties": {
    "vaultName": {
      "type": "string"
    },
    "version": {
      "type": "string"
    },
    "releaseDate": {
      "type": "string",
      "pattern": "^\\d{4}-\\d{2}-\\d{2}$"
    },
    "defaultEnvironment": {
      "type": "string"
    },
    "environments": {
      "type": "object",
      "additionalProperties": {
        "type": "object",
        "properties": {
          "requiredQaCompletion": {
            "type": "integer",
            "minimum": 0,
            "maximum": 100
          },
          "blockOnP0": {
            "type": "boolean"
          },
          "minCrashFreeRate": {
            "type": ["number", "null"],
            "minimum": 0,
            "maximum": 100
          },
          "requireSignoffs": {
            "type": "boolean"
          },
          "maxObservedAgeHours": {
            "type": ["integer", "null"],
            "minimum": 1
          },
          "blockOnMissingTelemetry": {
            "type": "boolean"
          },
          "telemetryRules": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": {
                "metric": { "type": "string" },
                "comparator": { "type": "string", "enum": ["<", "<=", ">", ">=", "==", "!="] },
                "threshold": { "type": "number" },
                "severity": { "type": "string", "enum": ["required", "warning"] }
              },
              "required": ["metric", "comparator", "threshold", "severity"]
            }
          }
        }
      }
    },
    "sentry": {
      "type": "object",
      "properties": {
        "organization": {
          "type": "string"
        },
        "project": {
          "type": "string"
        },
        "projectId": {
          "type": "string"
        },
        "targetCrashFreeRate": {
          "type": "number",
          "minimum": 0,
          "maximum": 100
        }
      },
      "required": ["organization", "project"]
    },
    "revenueCat": {
      "type": "object",
      "properties": {
        "project": {
          "type": "string"
        },
        "offerings": {
          "type": "array",
          "items": {
            "type": "string"
          }
        }
      },
      "required": ["project"]
    },
    "notifications": {
      "type": "object",
      "properties": {
        "slack": {
          "type": "object",
          "properties": {
            "enabled": { "type": "boolean" },
            "webhookEnv": { "type": "string" },
            "events": {
              "type": "array",
              "items": { "type": "string" }
            }
          },
          "required": ["enabled", "webhookEnv", "events"]
        }
      }
    }
  },
  "required": ["vaultName", "version", "releaseDate"]
}"#;

const BLOCKER_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Gantry Blocker Schema",
  "type": "object",
  "properties": {
    "type": {
      "type": "string",
      "enum": ["blocker"]
    },
    "id": {
      "type": "string",
      "pattern": "^BLK-\\d+$"
    },
    "title": {
      "type": "string",
      "minLength": 3
    },
    "severity": {
      "type": "string",
      "enum": ["P0", "P1", "P2"]
    },
    "status": {
      "type": "string",
      "enum": ["open", "in_progress", "blocked", "resolved"]
    },
    "owner": {
      "type": "string",
      "pattern": "^@[a-zA-Z0-9_.-]+$"
    },
    "source_files": {
      "type": "array",
      "items": {
        "type": "string"
      }
    },
    "sentry_issue": {
      "type": "string",
      "format": "uri"
    },
    "last_verified": {
      "type": "string",
      "pattern": "^\\d{4}-\\d{2}-\\d{2}$"
    },
    "created_at": {
      "type": "string",
      "pattern": "^\\d{4}-\\d{2}-\\d{2}$"
    }
  },
  "required": ["type", "id", "title", "severity", "status", "owner"]
}"#;

const QA_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Gantry QA Schema",
  "type": "object",
  "properties": {
    "type": {
      "type": "string",
      "enum": ["qa"]
    },
    "id": {
      "type": "string"
    },
    "title": {
      "type": "string"
    },
    "owner": {
      "type": "string",
      "pattern": "^@[a-zA-Z0-9_.-]+$"
    },
    "platforms": {
      "type": "array",
      "items": {
        "type": "string",
        "enum": ["ios", "android", "web", "desktop"]
      }
    },
    "source_files": {
      "type": "array",
      "items": {
        "type": "string"
      }
    },
    "last_verified": {
      "type": "string",
      "pattern": "^\\d{4}-\\d{2}-\\d{2}$"
    }
  },
  "required": ["type", "id", "title"]
}"#;

const GNG_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Gantry Go/No-Go Schema",
  "type": "object",
  "properties": {
    "type": {
      "type": "string",
      "enum": ["go_no_go"]
    },
    "release": {
      "type": "string"
    },
    "target_date": {
      "type": "string",
      "pattern": "^\\d{4}-\\d{2}-\\d{2}$"
    },
    "required_crash_free_rate": {
      "type": "number",
      "minimum": 0,
      "maximum": 100
    },
    "required_qa_completion": {
      "type": "integer",
      "minimum": 0,
      "maximum": 100
    },
    "status": {
      "type": "string",
      "enum": ["go", "at_risk", "no_go"]
    }
  },
  "required": ["type", "release", "target_date"]
}"#;

const RUNBOOK_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Gantry Runbook Schema",
  "type": "object",
  "properties": {
    "type": {
      "type": "string",
      "enum": ["runbook"]
    },
    "last_verified": {
      "type": "string",
      "pattern": "^\\d{4}-\\d{2}-\\d{2}$"
    }
  },
  "required": ["type"]
}"#;

const STORE_METADATA_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Gantry Store Metadata Schema",
  "type": "object",
  "properties": {
    "type": {
      "type": "string",
      "enum": ["store-metadata"]
    },
    "appName": {
      "type": "string",
      "maxLength": 30
    },
    "subtitle": {
      "type": "string",
      "maxLength": 30
    },
    "description": {
      "type": "string",
      "maxLength": 4000
    },
    "keywords": {
      "type": "array",
      "items": {
        "type": "string"
      }
    }
  },
  "required": ["appName", "description"]
}"#;

const WAIVERS_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Gantry Waivers Schema",
  "type": "array",
  "items": {
    "type": "object",
    "properties": {
      "id": { "type": "string" },
      "target_type": { "type": "string", "enum": ["blocker", "qa", "document", "gate", "validation", "signoff"] },
      "target_id": { "type": ["string", "null"] },
      "target_path": { "type": ["string", "null"] },
      "target_code": { "type": ["string", "null"] },
      "reason": { "type": "string" },
      "approved_by": { "type": "string" },
      "created_at": { "type": "string", "pattern": "^\\d{4}-\\d{2}-\\d{2}$" },
      "expires_at": { "type": "string", "pattern": "^\\d{4}-\\d{2}-\\d{2}$" },
      "status": { "type": "string", "enum": ["active", "expired"] }
    },
    "required": ["id", "target_type", "reason", "approved_by", "created_at", "expires_at", "status"]
  }
}"#;

const SCORECARD_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Gantry Scorecard Rules Schema",
  "type": "array",
  "items": {
    "type": "object",
    "properties": {
      "id": { "type": "string" },
      "category": { "type": "string" },
      "title": { "type": "string" },
      "severity": { "type": "string", "enum": ["error", "warning"] },
      "weight": { "type": "integer", "minimum": 0 },
      "predicate": {
        "type": "string",
        "enum": [
          "no_open_blockers",
          "max_open_blockers",
          "min_qa_completion",
          "required_signoff_role",
          "no_missing_docs",
          "no_validation_errors",
          "no_broken_links",
          "telemetry_fresh",
          "min_crash_free_rate",
          "required_evidence_kind",
          "max_stale_docs"
        ]
      },
      "params": {
        "type": ["object", "null"],
        "properties": {
          "max": { "type": "integer" },
          "min": { "type": "number" },
          "min_pct": { "type": "number" },
          "role": { "type": "string" },
          "min_rate": { "type": "number" },
          "kind": { "type": "string" }
        }
      },
      "failure_message": { "type": "string" },
      "suggested_action": { "type": "string" }
    },
    "required": ["id", "category", "title", "severity", "weight", "predicate", "failure_message", "suggested_action"]
  }
}"#;

const EVIDENCE_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Gantry Evidence Record Schema",
  "type": "object",
  "properties": {
    "id": { "type": "string" },
    "kind": { "type": "string", "enum": ["test_report", "security_scan", "telemetry_pull", "approval", "artifact", "app_store_build", "ci_run", "manual_note"] },
    "title": { "type": "string" },
    "source": { "type": "string" },
    "path": { "type": ["string", "null"] },
    "url": { "type": ["string", "null"] },
    "artifact_sha": { "type": ["string", "null"] },
    "commit": { "type": ["string", "null"] },
    "environment": { "type": ["string", "null"] },
    "release": { "type": ["string", "null"] },
    "created_at": { "type": "string" },
    "expires_at": { "type": ["string", "null"] },
    "metadata": { "type": ["object", "null"] },
    "attachments": {
      "type": "array",
      "items": { "type": "string" }
    }
  },
  "required": ["id", "kind", "title", "source", "created_at"]
}"#;

const RELEASE_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Gantry Release Manifest Schema",
  "type": "object",
  "properties": {
    "release": { "type": "string" },
    "commit": { "type": "string" },
    "commit_range": { "type": ["string", "null"] },
    "created_at": { "type": "string" },
    "environments": {
      "type": "array",
      "items": { "type": "string" }
    },
    "artifact_sha": { "type": ["string", "null"] },
    "build_url": { "type": ["string", "null"] },
    "included_blockers": {
      "type": "array",
      "items": { "type": "string" }
    },
    "completed_qa": {
      "type": "array",
      "items": { "type": "string" }
    },
    "signoffs": {
      "type": "array",
      "items": { "type": "string" }
    },
    "waivers": {
      "type": "array",
      "items": { "type": "string" }
    },
    "evidence_ids": {
      "type": "array",
      "items": { "type": "string" }
    },
    "readiness": { "type": "string" },
    "readiness_score": { "type": "integer", "minimum": 0, "maximum": 100 }
  },
  "required": ["release", "commit", "created_at", "readiness", "readiness_score"]
}"#;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GantryTask {
    pub id: String,
    pub title: String,
    pub source: String,
    pub severity: String,
    pub evidence: String,
    pub created_at: String,
    pub completed: bool,
    pub status: String, // "pending", "fixed", "stale", "ignored", "manually_completed"
    pub reason: Option<String>,
}

pub fn list_pending_tasks_ctx(vault_path: &Path) -> Result<Vec<GantryTask>, String> {
    let tasks_path = vault_path.join("tasks.md");
    if !tasks_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&tasks_path).map_err(|e| e.to_string())?;
    let mut tasks = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.contains("gantry:id:") {
            let completed = trimmed.starts_with("- [x]");
            let is_checkbox = trimmed.starts_with("- [ ]") || completed;
            if is_checkbox {
                let checkbox_len = 5;
                let end_title_pos = trimmed.find("<!--").unwrap_or(trimmed.len());
                if checkbox_len <= end_title_pos {
                    let title = trimmed[checkbox_len..end_title_pos].trim().to_string();
                    if let Some(start_comment) = trimmed.find("<!--") {
                        if let Some(end_comment) = trimmed[start_comment..].find("-->") {
                            let comment = &trimmed[start_comment + 4..start_comment + end_comment];
                            let (id, source, severity, evidence, created_at, status, reason) =
                                parse_comment_metadata(comment);

                            if !id.is_empty() {
                                let mut final_status = status.clone();
                                if final_status.is_empty() {
                                    final_status = if completed {
                                        "fixed".to_string()
                                    } else {
                                        "pending".to_string()
                                    };
                                }
                                let is_completed = completed
                                    || final_status == "ignored"
                                    || final_status == "manually_completed"
                                    || final_status == "fixed";
                                tasks.push(GantryTask {
                                    id,
                                    title,
                                    source,
                                    severity,
                                    evidence,
                                    created_at,
                                    completed: is_completed,
                                    status: final_status,
                                    reason,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(tasks)
}

pub fn list_pending_tasks() -> Result<Vec<GantryTask>, String> {
    let vault_path = get_vault_path();
    list_pending_tasks_ctx(&vault_path)
}

pub fn complete_task_by_id_ctx(vault_path: &Path, id: &str) -> Result<(), String> {
    let tasks_path = vault_path.join("tasks.md");
    if !tasks_path.exists() {
        return Err("tasks.md not found".to_string());
    }
    let content = fs::read_to_string(&tasks_path).map_err(|e| e.to_string())?;
    let mut modified = false;
    let mut lines: Vec<String> = content.split('\n').map(|s| s.to_string()).collect();
    let target_pattern = format!("gantry:id:{}", id);
    for line in &mut lines {
        if line.contains(&target_pattern) {
            if line.contains("- [ ]") {
                *line = line.replace("- [ ]", "- [x]");
            }
            *line = update_task_comment(line, "manually_completed", None);
            modified = true;
            break;
        }
    }
    if modified {
        let new_content = lines.join("\n");
        fs::write(&tasks_path, new_content).map_err(|e| e.to_string())?;
        let project_root = vault_path.parent().unwrap_or(vault_path);
        let opts = EvaluationOptions {
            vault: VaultContext {
                vault_path: vault_path.to_path_buf(),
                project_root: project_root.to_path_buf(),
            },
            environment: None,
            policy_overlay: None,
        };
        let _ = write_launch_index_ctx(&opts);
    } else {
        return Err(format!(
            "Task with ID '{}' not found or already completed",
            id
        ));
    }
    Ok(())
}

pub fn complete_task_by_id(id: &str) -> Result<(), String> {
    let vault_path = get_vault_path();
    complete_task_by_id_ctx(&vault_path, id)
}

pub fn ignore_task_by_id_ctx(vault_path: &Path, id: &str, reason: &str) -> Result<(), String> {
    let tasks_path = vault_path.join("tasks.md");
    if !tasks_path.exists() {
        return Err("tasks.md not found".to_string());
    }
    let content = fs::read_to_string(&tasks_path).map_err(|e| e.to_string())?;
    let mut modified = false;
    let mut lines: Vec<String> = content.split('\n').map(|s| s.to_string()).collect();
    let target_pattern = format!("gantry:id:{}", id);
    for line in &mut lines {
        if line.contains(&target_pattern) {
            if line.contains("- [ ]") {
                *line = line.replace("- [ ]", "- [x]");
            }
            *line = update_task_comment(line, "ignored", Some(reason));
            modified = true;
            break;
        }
    }
    if modified {
        let new_content = lines.join("\n");
        fs::write(&tasks_path, new_content).map_err(|e| e.to_string())?;
        let project_root = vault_path.parent().unwrap_or(vault_path);
        let opts = EvaluationOptions {
            vault: VaultContext {
                vault_path: vault_path.to_path_buf(),
                project_root: project_root.to_path_buf(),
            },
            environment: None,
            policy_overlay: None,
        };
        let _ = write_launch_index_ctx(&opts);
    } else {
        return Err(format!("Task with ID '{}' not found", id));
    }
    Ok(())
}

pub fn ignore_task_by_id(id: &str, reason: &str) -> Result<(), String> {
    let vault_path = get_vault_path();
    ignore_task_by_id_ctx(&vault_path, id, reason)
}

pub fn init_vault() -> Result<(), String> {
    init_vault_with_template("mobile-app", false)
}

pub fn init_vault_with_template(template_type: &str, force: bool) -> Result<(), String> {
    let vault_path = get_vault_path();
    init_vault_with_template_ctx(&vault_path, template_type, force)
}

pub fn init_vault_with_template_ctx(
    vault_path: &Path,
    template_type: &str,
    force: bool,
) -> Result<(), String> {
    if vault_path.exists() && !force {
        if let Ok(entries) = fs::read_dir(vault_path) {
            let mut has_files = false;
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name != "." && name != ".." {
                    has_files = true;
                    break;
                }
            }
            if has_files {
                return Err("Gantry vault already initialized. Use force to overwrite.".to_string());
            }
        }
    }

    fs::create_dir_all(vault_path).map_err(|e| e.to_string())?;
    fs::create_dir_all(vault_path.join("schema")).map_err(|e| e.to_string())?;
    fs::create_dir_all(vault_path.join("blockers")).map_err(|e| e.to_string())?;
    fs::create_dir_all(vault_path.join("qa")).map_err(|e| e.to_string())?;
    fs::create_dir_all(vault_path.join("store_metadata")).map_err(|e| e.to_string())?;

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let target_date = (chrono::Local::now() + chrono::Duration::days(14))
        .format("%Y-%m-%d")
        .to_string();

    let write_or_overwrite = |path: PathBuf, content: &str| -> Result<(), String> {
        let processed = content
            .replace("2026-06-15", &target_date)
            .replace("2026-06-30", &target_date)
            .replace("2026-06-10", &target_date)
            .replace("2026-06-25", &target_date)
            .replace("2026-06-12", &target_date)
            .replace("2026-05-27", &today);
        fs::write(path, processed).map_err(|e| e.to_string())?;
        Ok(())
    };

    // Select contents based on template_type
    let (config, go_no_go, tasks, agents, qa_plan, blocker) = match template_type {
        "saas-app" => (
            r#"{
  "vaultName": "Web SaaS App Release Vault",
  "version": "1.0.0",
  "releaseDate": "2026-06-30",
  "sentry": {
    "organization": "my-org",
    "project": "saas-frontend",
    "targetCrashFreeRate": 99.9
  }
}"#,
            r#"---
type: go_no_go
release: 1.0.0
target_date: 2026-06-30
required_qa_completion: 100
---

# SaaS App Go/No-Go Room

## Required Launch Gates

- [ ] **Engineering:** SSL certificates active, DB migrations run successfully. <!-- owner:@dev_jane -->
- [ ] **Product:** Landing page copy live, user analytics configured. <!-- owner:@pm_dan -->
- [ ] **QA:** Stripe checkout flows verified in sandbox. <!-- owner:@qa_bob -->
"#,
            r#"# Gantry Shared Work Queue (tasks.md) - SaaS App

## Launch Setup Checklist

- [ ] Configure Stripe webhook verify signing keys. <!-- owner:@dev_jane priority:P0 -->
- [ ] Check DB migration rollback script. <!-- owner:@dev_jane priority:P0 -->
- [ ] Verify DNS records propagate and SSL is active. <!-- owner:@pm_dan priority:P1 -->
"#,
            AGENTS_TEMPLATE,
            r#"---
type: qa
id: QA-SAAS-REG
title: SaaS Release Checklist
owner: "@qa_bob"
platforms:
  - web
source_files:
  - src/checkout.js
last_verified: 2026-05-27
---

# QA SaaS Regression Plan

## Payments Flow
- [ ] Verify checkout loads and updates Stripe dashboard. <!-- owner:@qa_bob -->
- [ ] Test invoice receipts sending. <!-- owner:@qa_bob -->
"#,
            r#"---
type: blocker
id: BLK-101
title: Checkout Page Paywall Loading Failure on Firefox
severity: P1
status: open
owner: "@dev_jane"
source_files:
  - src/checkout.js
last_verified: 2026-05-27
---

# Blocker: Checkout Page Paywall Loading Failure on Firefox
Checkout page fails to compile ES module syntax in older Firefox versions.
"#,
        ),
        "ai-app" => (
            r#"{
  "vaultName": "AI Native App Release Vault",
  "version": "0.5.0-beta",
  "releaseDate": "2026-06-10",
  "aiProviders": {
    "defaultModel": "gpt-4o",
    "fallbackModel": "gpt-3.5-turbo"
  }
}"#,
            r#"---
type: go_no_go
release: 0.5.0-beta
target_date: 2026-06-10
required_qa_completion: 100
---

# AI App Go/No-Go Room

## Required Launch Gates

- [ ] **Engineering:** Model API fallback tested under simulated timeout. <!-- owner:@dev_jane -->
- [ ] **Product:** Prompt inject protection guidelines set up. <!-- owner:@pm_dan -->
- [ ] **QA:** Token budget and system prompts verified. <!-- owner:@qa_bob -->
"#,
            r#"# Gantry Shared Work Queue (tasks.md) - AI App

## Launch Setup Checklist

- [ ] Verify API keys rate limits with provider. <!-- owner:@dev_jane priority:P0 -->
- [ ] Test system prompt jailbreaks. <!-- owner:@qa_bob priority:P0 -->
- [ ] Setup budget monitoring limits on LLM costs. <!-- owner:@pm_dan priority:P1 -->
"#,
            r#"# Gantry AI Agent Contract (AGENTS.md) - AI App

AI agents must verify model inputs, prompts safety, and budget constraints before checking in.
"#,
            r#"---
type: qa
id: QA-AI-SECURITY
title: AI Prompts Vulnerability Check
owner: "@qa_bob"
platforms:
  - web
source_files:
  - src/ai/llm.py
last_verified: 2026-05-27
---

# QA AI Prompts Security Plan

## Injection Tests
- [ ] Attempt system prompt leak injection. <!-- owner:@qa_bob -->
- [ ] Verify model output moderation active. <!-- owner:@qa_bob -->
"#,
            r#"---
type: blocker
id: BLK-101
title: Hallucinations on Edge Case Queries
severity: P1
status: open
owner: "@dev_jane"
source_files:
  - src/ai/llm.py
last_verified: 2026-05-27
---

# Blocker: Hallucinations on Edge Case Queries
The system prompt needs tighter formatting constraints.
"#,
        ),
        "backend-api" => (
            r#"{
  "vaultName": "Backend API Release Vault",
  "version": "2.4.0",
  "releaseDate": "2026-06-20",
  "sentry": {
    "organization": "my-org",
    "project": "backend-api",
    "targetCrashFreeRate": 99.95
  }
}"#,
            r#"---
type: go_no_go
release: 2.4.0
target_date: 2026-06-20
required_qa_completion: 100
---

# Backend API Go/No-Go Room

## Required Launch Gates

- [ ] **Engineering:** Database schema migrations run with rollback validation. <!-- owner:@dev_jane -->
- [ ] **QA:** Load testing completed with 10k req/sec peak. <!-- owner:@qa_bob -->
- [ ] **Security:** API authentication and OAuth scopes audit passed. <!-- owner:@dev_jane -->
"#,
            r#"# Gantry Shared Work Queue (tasks.md) - Backend API

## Launch Setup Checklist

- [ ] Run database migration dry-run on staging DB. <!-- owner:@dev_jane priority:P0 -->
- [ ] Verify Sentry environment configuration variables. <!-- owner:@dev_jane priority:P0 -->
- [ ] Verify API rate limiting headers on public endpoints. <!-- owner:@dev_jane priority:P1 -->
"#,
            AGENTS_TEMPLATE,
            r#"---
type: qa
id: QA-API-INTEGRATION
title: API Endpoints Integration Tests
owner: "@qa_bob"
platforms:
  - web
source_files:
  - src/middleware/ratelimit.go
last_verified: 2026-05-27
---

# QA API Endpoints Integration Tests

## Public Endpoints
- [ ] Verify `/v1/auth` handles invalid OAuth tokens gracefully. <!-- owner:@qa_bob -->
- [ ] Verify `/v1/users` returns correct paginated structures. <!-- owner:@qa_bob -->
"#,
            r#"---
type: blocker
id: BLK-101
title: Rate Limiter Redis Connection Leak on Shutdown
severity: P1
status: open
owner: "@dev_jane"
source_files:
  - src/middleware/ratelimit.go
last_verified: 2026-05-27
---

# Blocker: Rate Limiter Redis Connection Leak on Shutdown
Graceful shutdown sequence fails to release Redis connections when middleware is aborted.
"#,
        ),
        "desktop-app" => (
            r#"{
  "vaultName": "Desktop App Release Vault",
  "version": "1.2.0",
  "releaseDate": "2026-06-25",
  "sentry": {
    "organization": "my-org",
    "project": "desktop-client",
    "targetCrashFreeRate": 99.8
  }
}"#,
            r#"---
type: go_no_go
release: 1.2.0
target_date: 2026-06-25
required_qa_completion: 100
---

# Desktop App Go/No-Go Room

## Required Launch Gates

- [ ] **Engineering:** macOS and Windows installer packages signed successfully. <!-- owner:@dev_jane -->
- [ ] **QA:** Auto-update mechanism verified across platforms. <!-- owner:@qa_bob -->
- [ ] **Product:** Offline mode sync checks verified. <!-- owner:@pm_dan -->
"#,
            r#"# Gantry Shared Work Queue (tasks.md) - Desktop App

## Launch Setup Checklist

- [ ] Import developer signing certificates to build machine. <!-- owner:@dev_jane priority:P0 -->
- [ ] Configure update server release channel version. <!-- owner:@dev_jane priority:P0 -->
- [ ] Update desktop app client system tray icons. <!-- owner:@pm_dan priority:P1 -->
"#,
            AGENTS_TEMPLATE,
            r#"---
type: qa
id: QA-DESKTOP-SMOKE
title: Desktop Installer Smoke Tests
owner: "@qa_bob"
platforms:
  - desktop
source_files:
  - src-tauri/src/updater.rs
last_verified: 2026-05-27
---

# QA Desktop Installer Smoke Tests

## Installation & Launch
- [ ] Test clean installation on macOS Sequoia. <!-- owner:@qa_bob -->
- [ ] Test clean installation on Windows 11. <!-- owner:@qa_bob -->
"#,
            r#"---
type: blocker
id: BLK-101
title: Auto-updater Fails on Windows due to File Permission
severity: P0
status: blocked
owner: "@dev_jane"
source_files:
  - src-tauri/src/updater.rs
last_verified: 2026-05-27
---

# Blocker: Auto-updater Fails on Windows due to File Permission
The updater process tries to write to the installation directory without elevated permissions on Windows.
"#,
        ),
        "chrome-extension" => (
            r#"{
  "vaultName": "Chrome Extension Release Vault",
  "version": "1.0.0",
  "releaseDate": "2026-06-12",
  "sentry": {
    "organization": "my-org",
    "project": "chrome-extension",
    "targetCrashFreeRate": 99.7
  }
}"#,
            r#"---
type: go_no_go
release: 1.0.0
target_date: 2026-06-12
required_qa_completion: 100
---

# Chrome Extension Go/No-Go Room

## Required Launch Gates

- [ ] **Engineering:** Extension package passes Google Manifest V3 schema validator. <!-- owner:@dev_jane -->
- [ ] **QA:** Content scripts load correctly on all matches pages. <!-- owner:@qa_bob -->
- [ ] **Store:** Marketing screenshots and extension description approved. <!-- owner:@pm_dan -->
"#,
            r#"# Gantry Shared Work Queue (tasks.md) - Chrome Extension

## Launch Setup Checklist

- [ ] Run chrome extension manifest lint validation. <!-- owner:@dev_jane priority:P0 -->
- [ ] Verify OAuth client ID has chrome-extension:// scheme allowed. <!-- owner:@dev_jane priority:P0 -->
- [ ] Prepare Web Store visual promotional tiles. <!-- owner:@pm_dan priority:P1 -->
"#,
            AGENTS_TEMPLATE,
            r#"---
type: qa
id: QA-EXTENSION-INJECTION
title: Content Script Injection Audit
owner: "@qa_bob"
platforms:
  - web
source_files:
  - src/background.js
last_verified: 2026-05-27
---

# QA Content Script Injection Audit

## Script Injection
- [ ] Verify script injects into target frame under iframe sandbox. <!-- owner:@qa_bob -->
- [ ] Confirm no memory leaks on SPA page transitions. <!-- owner:@qa_bob -->
"#,
            r#"---
type: blocker
id: BLK-101
title: Service Worker Fails to Wake Up on DeclarativeNetRequest Event
severity: P0
status: open
owner: "@dev_jane"
source_files:
  - src/background.js
last_verified: 2026-05-27
---

# Blocker: Service Worker Fails to Wake Up on DeclarativeNetRequest Event
Background script does not persist listener registrations on declarativeNetRequest changes.
"#,
        ),
        "mobile-app" => (
            CONFIG_TEMPLATE,
            GO_NO_GO_TEMPLATE,
            TASKS_TEMPLATE,
            AGENTS_TEMPLATE,
            r#"---
type: qa
id: QA-REGRESSION
title: Release Regression Plan
owner: "@qa_bob"
platforms:
  - ios
  - android
source_files:
  - src/purchases/paywall.ts
last_verified: 2026-05-27
---

# QA Release Regression Plan (v1.0.0-beta.3)

## Core Features
- [ ] **Authentication:** Log in via Google, Apple, and Email Sign-Up. <!-- @qa_bob -->
- [ ] **Onboarding Flow:** Verify questionnaire screens. <!-- @qa_bob -->
"#,
            r#"---
type: blocker
id: BLK-101
title: App Crashes on Purchasing Monthly Subscription
severity: P0
status: blocked
owner: "@dev_jane"
source_files:
  - src/purchases/paywall.ts
last_verified: 2026-05-27
---

# App Crashes on Purchasing Monthly Subscription
When running the release candidate build, selecting the monthly subscription card on the paywall crashes the app immediately.
"#,
        ),
        other => {
            return Err(format!(
                "Unknown template '{}'. Valid templates: mobile-app, saas-app, ai-app, backend-api, desktop-app, chrome-extension",
                other
            ));
        }
    };

    write_or_overwrite(vault_path.join("config.json"), config)?;
    write_or_overwrite(vault_path.join("go_no_go.md"), go_no_go)?;
    write_or_overwrite(vault_path.join("tasks.md"), tasks)?;
    write_or_overwrite(vault_path.join("AGENTS.md"), agents)?;
    write_or_overwrite(vault_path.join("README-launch.md"), README_LAUNCH_TEMPLATE)?;

    write_or_overwrite(
        vault_path.join("schema").join("config.schema.json"),
        CONFIG_SCHEMA,
    )?;
    write_or_overwrite(
        vault_path.join("schema").join("blocker.schema.json"),
        BLOCKER_SCHEMA,
    )?;
    write_or_overwrite(vault_path.join("schema").join("qa.schema.json"), QA_SCHEMA)?;
    write_or_overwrite(
        vault_path.join("schema").join("go-no-go.schema.json"),
        GNG_SCHEMA,
    )?;
    write_or_overwrite(
        vault_path.join("schema").join("runbook.schema.json"),
        RUNBOOK_SCHEMA,
    )?;
    write_or_overwrite(
        vault_path.join("schema").join("store-metadata.schema.json"),
        STORE_METADATA_SCHEMA,
    )?;
    write_or_overwrite(
        vault_path.join("schema").join("waivers.schema.json"),
        WAIVERS_SCHEMA,
    )?;
    write_or_overwrite(
        vault_path.join("schema").join("scorecard.schema.json"),
        SCORECARD_SCHEMA,
    )?;
    write_or_overwrite(
        vault_path.join("schema").join("evidence.schema.json"),
        EVIDENCE_SCHEMA,
    )?;
    write_or_overwrite(
        vault_path.join("schema").join("release.schema.json"),
        RELEASE_SCHEMA,
    )?;

    write_or_overwrite(
        vault_path
            .join("blockers")
            .join("BLK-101-purchase-crash.md"),
        blocker,
    )?;
    write_or_overwrite(vault_path.join("qa").join("regression.md"), qa_plan)?;
    Ok(())
}

pub fn detect_project_type_ctx(project_root: &Path) -> &'static str {
    let cwd = project_root;

    // 1. Mobile Check
    if cwd.join("pubspec.yaml").exists()
        || (cwd.join("android").exists() && cwd.join("ios").exists())
    {
        return "mobile-app";
    }

    // 2. Desktop Check
    if cwd.join("src-tauri").join("tauri.conf.json").exists()
        || cwd.join("electron-builder.json").exists()
        || cwd.join("electron").exists()
    {
        return "desktop-app";
    }

    // 3. Chrome Extension Check
    if cwd.join("manifest.json").exists() {
        if let Ok(content) = fs::read_to_string(cwd.join("manifest.json")) {
            if content.contains("manifest_version") {
                return "chrome-extension";
            }
        }
    }

    // 4. package.json Check (SaaS vs AI)
    let package_json_path = cwd.join("package.json");
    if package_json_path.exists() {
        if let Ok(content) = fs::read_to_string(&package_json_path) {
            let content_lower = content.to_lowercase();
            // AI dependencies in package.json
            if content_lower.contains("openai")
                || content_lower.contains("anthropic")
                || content_lower.contains("langchain")
                || content_lower.contains("@google/generative-ai")
            {
                return "ai-app";
            }
            // SaaS dependencies or frontend framework
            if content_lower.contains("next")
                || content_lower.contains("vite")
                || content_lower.contains("react")
                || content_lower.contains("svelte")
                || content_lower.contains("vue")
                || content_lower.contains("angular")
            {
                return "saas-app";
            }
        }
    }

    // 5. AI prompts / agents folders check
    if cwd.join("prompts").exists() || cwd.join("agents").exists() || cwd.join("openai").exists() {
        return "ai-app";
    }

    // 6. Backend Check
    if cwd.join("openapi.yaml").exists()
        || cwd.join("openapi.json").exists()
        || cwd.join("go.mod").exists()
        || cwd.join("Cargo.toml").exists()
        || cwd.join("requirements.txt").exists()
    {
        return "backend-api";
    }

    // Default Fallback
    "mobile-app"
}

pub fn detect_project_type() -> &'static str {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    detect_project_type_ctx(&cwd)
}

#[derive(Debug, serde::Serialize)]
pub struct InitReport {
    pub detected_project_type: String,
    pub vault_path: String,
    pub dry_run: bool,
    pub files_to_create: Vec<String>,
}

pub fn init_vault_auto_ctx(
    vault_path: &Path,
    project_root: &Path,
    dry_run: bool,
    force: bool,
) -> Result<InitReport, String> {
    let detected_type = detect_project_type_ctx(project_root);

    let files_to_create = vec![
        format!("{}/config.json", vault_path.to_string_lossy()),
        format!("{}/go_no_go.md", vault_path.to_string_lossy()),
        format!("{}/tasks.md", vault_path.to_string_lossy()),
        format!("{}/AGENTS.md", vault_path.to_string_lossy()),
        format!("{}/qa/regression.md", vault_path.to_string_lossy()),
        format!(
            "{}/blockers/BLK-101-purchase-crash.md",
            vault_path.to_string_lossy()
        ),
    ];

    if dry_run {
        return Ok(InitReport {
            detected_project_type: detected_type.to_string(),
            vault_path: vault_path.to_string_lossy().into_owned(),
            dry_run: true,
            files_to_create,
        });
    }

    init_vault_with_template_ctx(vault_path, detected_type, force)?;
    record_telemetry_event_ctx(vault_path, "vault_initialized");

    Ok(InitReport {
        detected_project_type: detected_type.to_string(),
        vault_path: vault_path.to_string_lossy().into_owned(),
        dry_run: false,
        files_to_create: Vec::new(),
    })
}

pub fn init_vault_auto(dry_run: bool, force: bool) -> Result<InitReport, String> {
    let vault_path = get_vault_path();
    let project_root = std::env::current_dir().unwrap_or_default();
    init_vault_auto_ctx(&vault_path, &project_root, dry_run, force)
}

// --- Recommended draft templates for audit mode ---
const RELEASE_NOTES_TEMPLATE: &str = r#"---
type: release-notes
last_verified: 2026-05-27
---

# Release Notes

## New Features
- [Draft] Initial release features.

## Improvements & Fixes
- [Draft] General stability improvements.
"#;

const ENGINEERING_NOTES_TEMPLATE: &str = r#"---
type: engineering-notes
last_verified: 2026-05-27
---

# Engineering Notes

## System Architecture
- Describe the key architecture elements.

## Risks & Mitigations
- Identify technical risks and how they are mitigated.
"#;

const RUNBOOK_TEMPLATE: &str = r#"---
type: runbook
id: RB-LAUNCH
title: Launch Runbook
last_verified: 2026-05-27
---

# Launch Runbook

## Pre-Launch Steps
- [ ] Database migrations verified.
- [ ] Environment variables set up in production.

## Launch Deployment
- [ ] Deploy backend service.
- [ ] Publish frontend assets.

## Post-Launch Verification
- [ ] Run production smoke tests.
"#;

const POSTLAUNCH_TEMPLATE: &str = r#"---
type: postlaunch
last_verified: 2026-05-27
---

# Post-Launch Runbook & Monitoring

## Alerting Thresholds
- CPU usage < 80%
- Error rate < 1%

## Rollback Trigger Checklist
- [ ] Critical purchase service failing.
- [ ] App crashing on startup.
"#;

pub(crate) fn get_last_release_tag(project_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .current_dir(project_root)
        .output()
        .ok()?;
    if output.status.success() {
        let tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !tag.is_empty() {
            return Some(tag);
        }
    }
    None
}

pub(crate) fn get_changed_files_since_tag(project_root: &Path, tag: &str) -> Vec<String> {
    let mut files = Vec::new();
    let output = Command::new("git")
        .args(["diff", "--name-only", &format!("{}..HEAD", tag)])
        .current_dir(project_root)
        .output();
    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    files.push(trimmed.to_string());
                }
            }
        }
    }
    files
}

pub(crate) fn get_changed_files_recent_commits(project_root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    let output = Command::new("git")
        .args(["log", "--name-only", "-20", "--pretty=format:"])
        .current_dir(project_root)
        .output();
    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    files.push(trimmed.to_string());
                }
            }
            files.sort();
            files.dedup();
        }
    }
    files
}

fn get_recent_commits_for_release_notes(project_root: &Path, tag_opt: Option<&str>) -> Vec<String> {
    let mut commits = Vec::new();
    let mut args = vec!["log", "--oneline"];
    let range;
    if let Some(tag) = tag_opt {
        range = format!("{}..HEAD", tag);
        args.push(&range);
    } else {
        args.push("-20");
    }

    let output = Command::new("git")
        .args(&args)
        .current_dir(project_root)
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    commits.push(trimmed.to_string());
                }
            }
        }
    }
    commits
}

fn detect_risky_areas(
    changed_files: &[String],
    blockers: &[BlockerEntry],
    qa_suites: &[QAEntry],
    issues: &mut Vec<GantryIssue>,
) {
    let mut covered_paths = HashSet::new();
    for blocker in blockers {
        for src in &blocker.source_files {
            covered_paths.insert(src.clone());
            if let Some(parent) = Path::new(src).parent().and_then(|p| p.to_str()) {
                covered_paths.insert(parent.to_string());
            }
        }
    }
    for qa in qa_suites {
        covered_paths.insert(qa.path.clone());
        for src in &qa.source_files {
            covered_paths.insert(src.clone());
            if let Some(parent) = Path::new(src).parent().and_then(|p| p.to_str()) {
                if !parent.is_empty() {
                    covered_paths.insert(parent.to_string());
                }
            }
        }
    }

    let mut changed_dirs = std::collections::HashMap::new();
    for file in changed_files {
        if file.starts_with(".gantry") || file.starts_with("node_modules") {
            continue;
        }
        if let Some(parent) = Path::new(file).parent().and_then(|p| p.to_str()) {
            if !parent.is_empty() {
                let count = changed_dirs.entry(parent.to_string()).or_insert(0);
                *count += 1;
            }
        }
    }

    for (dir, count) in changed_dirs {
        let dir_path = Path::new(&dir);
        let is_covered = covered_paths.contains(&dir)
            || covered_paths
                .iter()
                .any(|p| Path::new(p).starts_with(dir_path))
            || covered_paths.iter().any(|p| dir_path.starts_with(p));
        if !is_covered && count >= 2 {
            issues.push(GantryIssue {
                severity: "warning".to_string(),
                file: dir.clone(),
                code: "UNCOVERED_CODE_CHANGES".to_string(),
                message: format!(
                    "Component '{}' has {} modified files since last release but no blocker or QA checklist covers it.",
                    dir, count
                ),
                field: None,
                evidence: Some(create_evidence(
                    &format!("uncovered-change-{}", dir.replace("/", "-")),
                    "source_file",
                    &dir,
                    &format!("Component '{}' has {} uncovered modified files", dir, count),
                    "warning",
                    Some("UNCOVERED_CODE_CHANGES"),
                    Some("Create a QA checklist or open blocker to cover this component")
                )),
                waived: false,
            });
        }
    }
}

// --- Test report parsers for ingestion ---
fn parse_junit_xml(
    content: &str,
    file_name: &str,
    issues: &mut Vec<GantryIssue>,
    tasks_to_add: &mut Vec<String>,
    tasks_content: &str,
) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if let Ok(doc) = roxmltree::Document::parse(content) {
        for node in doc.descendants() {
            if node.has_tag_name("testcase") {
                let has_failure = node
                    .children()
                    .any(|c| c.has_tag_name("failure") || c.has_tag_name("error"));
                if has_failure {
                    let name = node.attribute("name").unwrap_or("Unknown test");
                    let classname = node.attribute("classname").unwrap_or("");
                    let test_id = if classname.is_empty() {
                        name.to_string()
                    } else {
                        format!("{}.{}", classname, name)
                    };

                    let failure_msg = node
                        .children()
                        .find(|c| c.has_tag_name("failure") || c.has_tag_name("error"))
                        .and_then(|c| c.attribute("message").or_else(|| c.text()))
                        .unwrap_or("Test failed");

                    let test_id_clean = test_id
                        .replace(" ", "-")
                        .replace(":", "-")
                        .replace("/", "-");
                    let gantry_id = format!("failed-test-{}", test_id_clean);
                    let is_ignored = is_task_resolved_or_ignored(&gantry_id, tasks_content);
                    let severity = if is_ignored { "warning" } else { "error" };

                    issues.push(GantryIssue {
                        severity: severity.to_string(),
                        file: file_name.to_string(),
                        code: "FAILING_TEST".to_string(),
                        message: format!("Failing unit test: {} - {}", test_id, failure_msg),
                        field: None,
                        evidence: Some(create_evidence_v2(
                            &gantry_id,
                            "test_report",
                            file_name,
                            &format!("Test {} failed: {}", test_id, failure_msg),
                            severity,
                            Some("FAILING_TEST"),
                            Some(&format!(
                                "Investigate and fix failing test case: {}",
                                test_id
                            )),
                            Some(1.0),
                            None,
                            None,
                            Some(&test_id),
                            None,
                            None,
                            Some(&gantry_id),
                        )),
                        waived: false,
                    });

                    let task_text = format!("Fix failing test: {}", test_id);
                    if !tasks_content.contains(&format!("gantry:id:{}", gantry_id)) {
                        tasks_to_add.push(format!("- [ ] {} <!-- gantry:id:{} source:test_report severity:P0 evidence:{} created_at:{} -->", task_text, gantry_id, file_name, today));
                    }
                }
            }
        }
    }
}

fn parse_playwright_json(
    json: &JsonValue,
    file_name: &str,
    issues: &mut Vec<GantryIssue>,
    tasks_to_add: &mut Vec<String>,
    tasks_content: &str,
) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    fn traverse_suites(
        val: &JsonValue,
        file_name: &str,
        issues: &mut Vec<GantryIssue>,
        tasks_to_add: &mut Vec<String>,
        tasks_content: &str,
        today: &str,
    ) {
        if let Some(suites) = val.get("suites").and_then(|v| v.as_array()) {
            for suite in suites {
                traverse_suites(suite, file_name, issues, tasks_to_add, tasks_content, today);
            }
        }
        if let Some(specs) = val.get("specs").and_then(|v| v.as_array()) {
            for spec in specs {
                let title = spec
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("Unknown test");
                if let Some(tests) = spec.get("tests").and_then(|v| v.as_array()) {
                    for test in tests {
                        if let Some(results) = test.get("results").and_then(|v| v.as_array()) {
                            for res in results {
                                if let Some(status) = res.get("status").and_then(|s| s.as_str()) {
                                    if status == "failed" {
                                        let clean_title = title
                                            .replace(" ", "-")
                                            .replace(":", "-")
                                            .replace("/", "-");
                                        let gantry_id = format!("failed-test-{}", clean_title);
                                        let is_ignored =
                                            is_task_resolved_or_ignored(&gantry_id, tasks_content);
                                        let severity = if is_ignored { "warning" } else { "error" };
                                        issues.push(GantryIssue {
                                            severity: severity.to_string(),
                                            file: file_name.to_string(),
                                            code: "FAILING_TEST".to_string(),
                                            message: format!("Failing E2E test: {}", title),
                                            field: None,
                                            evidence: Some(create_evidence_v2(
                                                &gantry_id,
                                                "test_report",
                                                file_name,
                                                &format!("E2E test failed: {}", title),
                                                severity,
                                                Some("FAILING_TEST"),
                                                Some(&format!("Fix E2E test failure: {}", title)),
                                                Some(1.0),
                                                None,
                                                None,
                                                Some(title),
                                                None,
                                                None,
                                                Some(&gantry_id),
                                            )),
                                            waived: false,
                                        });
                                        let task_text = format!("Fix failing test: {}", title);
                                        if !tasks_content
                                            .contains(&format!("gantry:id:{}", gantry_id))
                                        {
                                            tasks_to_add.push(format!("- [ ] {} <!-- gantry:id:{} source:test_report severity:P0 evidence:{} created_at:{} -->", task_text, gantry_id, file_name, today));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    traverse_suites(json, file_name, issues, tasks_to_add, tasks_content, &today);
}

fn parse_jest_json(
    json: &JsonValue,
    file_name: &str,
    issues: &mut Vec<GantryIssue>,
    tasks_to_add: &mut Vec<String>,
    tasks_content: &str,
) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if let Some(test_results) = json.get("testResults").and_then(|v| v.as_array()) {
        for tr in test_results {
            if let Some(assertions) = tr.get("assertionResults").and_then(|v| v.as_array()) {
                for ass in assertions {
                    if let Some(status) = ass.get("status").and_then(|s| s.as_str()) {
                        if status == "failed" {
                            let name = ass
                                .get("fullName")
                                .and_then(|n| n.as_str())
                                .unwrap_or("Unknown test");
                            let clean_name =
                                name.replace(" ", "-").replace(":", "-").replace("/", "-");
                            let gantry_id = format!("failed-test-{}", clean_name);
                            let is_ignored = is_task_resolved_or_ignored(&gantry_id, tasks_content);
                            let severity = if is_ignored { "warning" } else { "error" };
                            issues.push(GantryIssue {
                                severity: severity.to_string(),
                                file: file_name.to_string(),
                                code: "FAILING_TEST".to_string(),
                                message: format!("Failing JS test: {}", name),
                                field: None,
                                evidence: Some(create_evidence_v2(
                                    &gantry_id,
                                    "test_report",
                                    file_name,
                                    &format!("JS test failed: {}", name),
                                    severity,
                                    Some("FAILING_TEST"),
                                    Some(&format!("Fix JavaScript test failure: {}", name)),
                                    Some(1.0),
                                    None,
                                    None,
                                    Some(name),
                                    None,
                                    None,
                                    Some(&gantry_id),
                                )),
                                waived: false,
                            });
                            let task_text = format!("Fix failing test: {}", name);
                            if !tasks_content.contains(&format!("gantry:id:{}", gantry_id)) {
                                tasks_to_add.push(format!("- [ ] {} <!-- gantry:id:{} source:test_report severity:P0 evidence:{} created_at:{} -->", task_text, gantry_id, file_name, today));
                            }
                        }
                    }
                }
            }
        }
    }
}

fn parse_flutter_jsonl(
    content: &str,
    file_name: &str,
    issues: &mut Vec<GantryIssue>,
    tasks_to_add: &mut Vec<String>,
    tasks_content: &str,
) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut test_names = std::collections::HashMap::new();
    for line in content.lines() {
        if let Ok(json) = serde_json::from_str::<JsonValue>(line) {
            if let Some(event) = json.get("event").and_then(|e| e.as_str()) {
                if event == "testStart" {
                    if let Some(test_json) = json.get("test") {
                        if let (Some(id), Some(name)) = (
                            test_json.get("id").and_then(|i| i.as_u64()),
                            test_json.get("name").and_then(|n| n.as_str()),
                        ) {
                            test_names.insert(id, name.to_string());
                        }
                    }
                } else if event == "testDone" {
                    if let Some(result) = json.get("result").and_then(|r| r.as_str()) {
                        if result == "failure" || result == "error" {
                            if let Some(id) = json.get("testID").and_then(|i| i.as_u64()) {
                                let name = test_names
                                    .get(&id)
                                    .cloned()
                                    .unwrap_or_else(|| "Flutter Test".to_string());
                                let clean_name =
                                    name.replace(" ", "-").replace(":", "-").replace("/", "-");
                                let gantry_id = format!("failed-test-{}", clean_name);
                                let is_ignored =
                                    is_task_resolved_or_ignored(&gantry_id, tasks_content);
                                let severity = if is_ignored { "warning" } else { "error" };
                                issues.push(GantryIssue {
                                    severity: severity.to_string(),
                                    file: file_name.to_string(),
                                    code: "FAILING_TEST".to_string(),
                                    message: format!("Failing Flutter test: {}", name),
                                    field: None,
                                    evidence: Some(create_evidence_v2(
                                        &gantry_id,
                                        "test_report",
                                        file_name,
                                        &format!("Flutter test failed: {}", name),
                                        severity,
                                        Some("FAILING_TEST"),
                                        Some(&format!("Fix Flutter test failure: {}", name)),
                                        Some(1.0),
                                        None,
                                        None,
                                        Some(&name),
                                        None,
                                        None,
                                        Some(&gantry_id),
                                    )),
                                    waived: false,
                                });
                                let task_text = format!("Fix failing test: {}", name);
                                if !tasks_content.contains(&format!("gantry:id:{}", gantry_id)) {
                                    tasks_to_add.push(format!("- [ ] {} <!-- gantry:id:{} source:test_report severity:P0 evidence:{} created_at:{} -->", task_text, gantry_id, file_name, today));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn ingest_test_results_ctx(
    vault_path: &Path,
    project_root: &Path,
    issues: &mut Vec<GantryIssue>,
    tasks_to_add: &mut Vec<String>,
    tasks_content: &str,
) {
    let mut patterns = Vec::new();
    let config_path = vault_path.join("config.json");
    if config_path.exists() {
        if let Ok(raw) = fs::read_to_string(&config_path) {
            if let Ok(json) = serde_json::from_str::<JsonValue>(&raw) {
                if let Some(arr) = json.get("testReports").and_then(|a| a.as_array()) {
                    for v in arr {
                        if let Some(s) = v.as_str() {
                            patterns.push(s.to_string());
                        }
                    }
                }
            }
        }
    }

    if patterns.is_empty() {
        let defaults = vec![
            "playwright-report/results.json".to_string(),
            "test-results/results.json".to_string(),
            "vitest-report.json".to_string(),
            "jest-report.json".to_string(),
            "flutter_test_report.json".to_string(),
            "target/surefire-reports/**/*.xml".to_string(),
            "target/failsafe-reports/**/*.xml".to_string(),
            "build/test-results/**/*.xml".to_string(),
        ];
        patterns = defaults;
    }

    let mut files_to_check = Vec::new();
    for pattern in patterns {
        let resolved = resolve_path_pattern_ctx(project_root, &pattern);
        for r in resolved {
            files_to_check.push(r);
        }
    }

    files_to_check.sort();
    files_to_check.dedup();

    for path in files_to_check {
        if !path.exists() || !path.is_file() {
            continue;
        }

        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if let Ok(content) = fs::read_to_string(&path) {
            if path.extension().and_then(|e| e.to_str()) == Some("xml") {
                parse_junit_xml(&content, &file_name, issues, tasks_to_add, tasks_content);
            } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(json) = serde_json::from_str::<JsonValue>(&content) {
                    if json.get("suites").is_some() && json.get("specs").is_some() {
                        parse_playwright_json(
                            &json,
                            &file_name,
                            issues,
                            tasks_to_add,
                            tasks_content,
                        );
                    } else if json.get("testResults").is_some() {
                        parse_jest_json(&json, &file_name, issues, tasks_to_add, tasks_content);
                    } else {
                        parse_flutter_jsonl(
                            &content,
                            &file_name,
                            issues,
                            tasks_to_add,
                            tasks_content,
                        );
                    }
                } else {
                    parse_flutter_jsonl(&content, &file_name, issues, tasks_to_add, tasks_content);
                }
            }
        }
    }
}

pub fn run_launch_audit() -> Result<IndexPayload, String> {
    run_launch_audit_ex(true)
}

pub fn run_launch_audit_ex(write: bool) -> Result<IndexPayload, String> {
    run_launch_audit_ex_for_environment(write, None)
}

pub fn run_launch_audit_ex_for_environment(
    write: bool,
    env: Option<&str>,
) -> Result<IndexPayload, String> {
    let ctx = resolve_existing_vault(None)?;
    let opts = EvaluationOptions {
        vault: ctx,
        environment: env.map(|s| s.to_string()),
        policy_overlay: None,
    };
    run_launch_audit_ctx(&opts, write)
}

pub fn run_launch_audit_ctx(opts: &EvaluationOptions, write: bool) -> Result<IndexPayload, String> {
    let vault_path = &opts.vault.vault_path;
    let project_root = &opts.vault.project_root;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let mut documents_to_create = Vec::new();
    let mut affected_files = Vec::new();

    // 1. Initialize vault if missing
    if !vault_path.exists() {
        documents_to_create.push(".gantry/config.json".to_string());
        documents_to_create.push(".gantry/go_no_go.md".to_string());
        documents_to_create.push(".gantry/tasks.md".to_string());
        documents_to_create.push(".gantry/AGENTS.md".to_string());
        documents_to_create.push(".gantry/README-launch.md".to_string());
        documents_to_create.push(".gantry/schema/blocker.schema.json".to_string());
        documents_to_create.push(".gantry/schema/qa.schema.json".to_string());
        documents_to_create.push(".gantry/schema/go-no-go.schema.json".to_string());
        documents_to_create.push(".gantry/schema/runbook.schema.json".to_string());
        documents_to_create.push(".gantry/schema/store-metadata.schema.json".to_string());
        documents_to_create.push(".gantry/blockers/BLK-101-purchase-crash.md".to_string());
        documents_to_create.push(".gantry/qa/regression.md".to_string());

        for doc in &documents_to_create {
            affected_files.push(doc.clone());
        }

        if write {
            init_vault_with_template_ctx(vault_path, "mobile-app", false)?;
        } else {
            let args: Vec<String> = std::env::args().collect();
            let mut report_url = None;
            if let Some(pos) = args.iter().position(|x| x == "--report-url") {
                if pos + 1 < args.len() {
                    report_url = Some(args[pos + 1].clone());
                }
            }

            let payload = IndexPayload {
                vault_name: "Mock Vault (Dry Run)".to_string(),
                release: "1.0.0".to_string(),
                readiness: "No-Go".to_string(),
                target_date: today.clone(),
                open_blockers: 1,
                qa_total: 7,
                qa_completed: 0,
                stale_docs: Vec::new(),
                missing_docs: vec![
                    "config.json".to_string(),
                    "go_no_go.md".to_string(),
                    "tasks.md".to_string(),
                    "AGENTS.md".to_string(),
                    "README-launch.md".to_string(),
                ],
                blockers: Vec::new(),
                qa_suites: Vec::new(),
                last_updated: today.clone(),
                validation_errors: Vec::new(),
                broken_links: Vec::new(),
                source_missing: Vec::new(),
                docs_health_status: "critical".to_string(),
                go_no_go_gates: Vec::new(),
                signoffs: Vec::new(),
                readiness_reasons: vec![
                    "Gantry vault has not been initialized. Run gantry init first.".to_string(),
                ],
                readiness_score: 0,
                category_scores: std::collections::HashMap::new(),
                evidence_graph: Vec::new(),
                score_reasons: std::collections::HashMap::new(),
                audit_preview: Some(AuditPreview {
                    documents_to_create,
                    tasks_to_create: Vec::new(),
                    tasks_to_auto_resolve: Vec::new(),
                    affected_files,
                }),
                impact_summary: None,
                report_url,
                active_environment: "production".to_string(),
                environment_results: std::collections::HashMap::new(),
                active_waivers: Vec::new(),
                expired_waivers: Vec::new(),
                waived_items: Vec::new(),
                scorecard_results: Vec::new(),
                evidence_records: Vec::new(),
                release_manifest_path: None,
            };
            return Ok(payload);
        }
    }

    // 2. Generate missing recommended documents as draft templates
    let tag_opt = get_last_release_tag(project_root);
    let commits = get_recent_commits_for_release_notes(project_root, tag_opt.as_deref());
    let release_notes_content = if !commits.is_empty() {
        let mut formatted_commits = String::new();
        for commit in commits {
            let parts: Vec<&str> = commit.splitn(2, ' ').collect();
            let msg = if parts.len() > 1 { parts[1] } else { parts[0] };
            formatted_commits.push_str(&format!("- {}\n", msg));
        }
        format!(
            r#"---
type: release-notes
last_verified: {}
---

# Release Notes

## New Features & Changes (Drafted from Git)
{}
"#,
            today, formatted_commits
        )
    } else {
        RELEASE_NOTES_TEMPLATE
            .to_string()
            .replace("2026-05-27", &today)
    };

    if !vault_path.join("release-notes.md").exists() {
        documents_to_create.push(".gantry/release-notes.md".to_string());
        affected_files.push(".gantry/release-notes.md".to_string());
        if write {
            fs::write(vault_path.join("release-notes.md"), &release_notes_content)
                .map_err(|e| e.to_string())?;
        }
    }
    if !vault_path.join("engineering-notes.md").exists() {
        documents_to_create.push(".gantry/engineering-notes.md".to_string());
        affected_files.push(".gantry/engineering-notes.md".to_string());
        if write {
            fs::write(
                vault_path.join("engineering-notes.md"),
                ENGINEERING_NOTES_TEMPLATE.replace("2026-05-27", &today),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    if !vault_path.join("runbook.md").exists() {
        documents_to_create.push(".gantry/runbook.md".to_string());
        affected_files.push(".gantry/runbook.md".to_string());
        if write {
            fs::write(
                vault_path.join("runbook.md"),
                RUNBOOK_TEMPLATE.replace("2026-05-27", &today),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    if !vault_path.join("postlaunch.md").exists() {
        documents_to_create.push(".gantry/postlaunch.md".to_string());
        affected_files.push(".gantry/postlaunch.md".to_string());
        if write {
            fs::write(
                vault_path.join("postlaunch.md"),
                POSTLAUNCH_TEMPLATE.replace("2026-05-27", &today),
            )
            .map_err(|e| e.to_string())?;
        }
    }

    // 3. Compile launch index first to find validation issues
    let mut payload = compile_launch_index_ctx(opts)?;

    // 4. Run Git intelligence risk checking & append warning issues to validation_errors
    let changed_files = if let Some(ref tag) = tag_opt {
        get_changed_files_since_tag(project_root, tag)
    } else {
        get_changed_files_recent_commits(project_root)
    };
    detect_risky_areas(
        &changed_files,
        &payload.blockers,
        &payload.qa_suites,
        &mut payload.validation_errors,
    );

    // 5. Generate repair tasks and append to tasks.md
    let tasks_path = vault_path.join("tasks.md");
    let mut tasks_to_create = Vec::new();
    let mut tasks_to_auto_resolve = Vec::new();

    if tasks_path.exists() {
        let mut tasks_content = fs::read_to_string(&tasks_path).map_err(|e| e.to_string())?;
        let mut new_tasks = Vec::new();

        // Ingest test reports and append failed tests to validation errors and tasks
        ingest_test_results_ctx(
            vault_path,
            project_root,
            &mut payload.validation_errors,
            &mut new_tasks,
            &tasks_content,
        );

        // Check for missing required docs
        for doc in &payload.missing_docs {
            let is_required = doc == "README-launch.md"
                || doc == "go_no_go.md"
                || doc == "tasks.md"
                || doc == "AGENTS.md";
            let severity = if is_required { "P0" } else { "P1" };
            let gantry_id = format!("missing-doc-{}", doc);
            let task_text = format!("Create missing required document: {}", doc);
            if !tasks_content.contains(&format!("gantry:id:{}", gantry_id)) {
                new_tasks.push(format!("- [ ] {} <!-- gantry:id:{} source:audit severity:{} evidence:{} created_at:{} -->", task_text, gantry_id, severity, doc, today));
            }
        }

        // Check for validation errors (e.g. invalid frontmatter or schema)
        for err in &payload.validation_errors {
            if err.code == "INVALID_FRONTMATTER" || err.code == "INVALID_SCHEMA" {
                let gantry_id = format!("validation-err-{}", err.file);
                let task_text = format!("Fix metadata validation error in {}", err.file);
                if !tasks_content.contains(&format!("gantry:id:{}", gantry_id)) {
                    new_tasks.push(format!("- [ ] {} <!-- gantry:id:{} source:audit severity:P0 evidence:{} created_at:{} -->", task_text, gantry_id, err.file, today));
                }
            }
        }

        // Check for broken links
        for link in &payload.broken_links {
            let gantry_id = format!("broken-link-{}", link.file);
            let task_text = format!("Fix broken wiki link in {}", link.file);
            if !tasks_content.contains(&format!("gantry:id:{}", gantry_id)) {
                new_tasks.push(format!("- [ ] {} <!-- gantry:id:{} source:audit severity:P0 evidence:{} created_at:{} -->", task_text, gantry_id, link.file, today));
            }
        }

        // Check for stale documents
        for stale in &payload.stale_docs {
            let gantry_id = format!("stale-doc-{}", stale);
            let task_text = format!("Re-verify stale document: {}", stale);
            if !tasks_content.contains(&format!("gantry:id:{}", gantry_id)) {
                new_tasks.push(format!("- [ ] {} <!-- gantry:id:{} source:audit severity:P1 evidence:{} created_at:{} -->", task_text, gantry_id, stale, today));
            }
        }

        // Check for open blockers
        for blocker in &payload.blockers {
            let status_lower = blocker.status.to_lowercase();
            if status_lower != "resolved" {
                let gantry_id = format!("blocker-{}", blocker.id);
                let task_text = format!("Resolve open blocker: {} ({})", blocker.title, blocker.id);
                if !tasks_content.contains(&format!("gantry:id:{}", gantry_id)) {
                    new_tasks.push(format!("- [ ] {} <!-- gantry:id:{} source:audit severity:{} evidence:{} created_at:{} -->", task_text, gantry_id, blocker.severity, blocker.path, today));
                }
            }
        }

        // Check for unsigned gates
        for gate in &payload.go_no_go_gates {
            if gate.status != "checked" && !gate.waived {
                let gate_clean = gate
                    .name
                    .replace(" ", "-")
                    .replace(":", "-")
                    .replace("/*", "-")
                    .replace("*/", "-");
                let clean_owner = gate.owner.replace("@", "");
                let gantry_id = format!("gate-{}-{}", clean_owner, gate_clean);
                let task_text = format!(
                    "Obtain stakeholder sign-off: {} for {}",
                    gate.owner, gate.name
                );
                if !tasks_content.contains(&format!("gantry:id:{}", gantry_id)) {
                    new_tasks.push(format!("- [ ] {} <!-- gantry:id:{} source:audit severity:P0 evidence:go_no_go.md created_at:{} -->", task_text, gantry_id, today));
                }
            }
        }

        // Check for uncovered code change warnings
        for err in &payload.validation_errors {
            if err.code == "UNCOVERED_CODE_CHANGES" {
                let gantry_id = format!("uncovered-change-{}", err.file);
                let task_text = format!("Cover modified component with QA/Blocker: {}", err.file);
                if !tasks_content.contains(&format!("gantry:id:{}", gantry_id)) {
                    new_tasks.push(format!("- [ ] {} <!-- gantry:id:{} source:audit severity:P1 evidence:{} created_at:{} -->", task_text, gantry_id, err.file, today));
                }
            }
        }

        // Populate tasks_to_create from new_tasks
        for task_line in &new_tasks {
            let checkbox_len = 5;
            let end_title_pos = task_line.find("<!--").unwrap_or(task_line.len());
            if checkbox_len <= end_title_pos {
                let title = task_line[checkbox_len..end_title_pos].trim().to_string();
                tasks_to_create.push(title);
            }
        }

        // Auto-resolve fixed findings!
        let mut active_finding_ids = HashSet::new();
        for doc in &payload.missing_docs {
            active_finding_ids.insert(format!("missing-doc-{}", doc));
        }
        for err in &payload.validation_errors {
            if err.code == "INVALID_FRONTMATTER" || err.code == "INVALID_SCHEMA" {
                active_finding_ids.insert(format!("validation-err-{}", err.file));
            } else if err.code == "UNCOVERED_CODE_CHANGES" {
                active_finding_ids.insert(format!("uncovered-change-{}", err.file));
            } else if err.code == "FAILING_TEST" {
                if let Some(ref ev) = err.evidence {
                    active_finding_ids.insert(ev.id.clone());
                }
            }
        }
        for link in &payload.broken_links {
            active_finding_ids.insert(format!("broken-link-{}", link.file));
        }
        for stale in &payload.stale_docs {
            active_finding_ids.insert(format!("stale-doc-{}", stale));
        }
        for blocker in &payload.blockers {
            let status_lower = blocker.status.to_lowercase();
            if status_lower != "resolved" {
                active_finding_ids.insert(format!("blocker-{}", blocker.id));
            }
        }
        for gate in &payload.go_no_go_gates {
            if gate.status != "checked" && !gate.waived {
                let gate_clean = gate
                    .name
                    .replace(" ", "-")
                    .replace(":", "-")
                    .replace("/*", "-")
                    .replace("*/", "-");
                let clean_owner = gate.owner.replace("@", "");
                active_finding_ids.insert(format!("gate-{}-{}", clean_owner, gate_clean));
            }
        }

        let mut tasks_modified = false;
        let mut lines: Vec<String> = tasks_content.split('\n').map(|s| s.to_string()).collect();
        for line in &mut lines {
            if line.contains("gantry:id:") {
                if let Some(pos) = line.find("gantry:id:") {
                    let rest = &line[pos + 10..];
                    let id_end = rest.find(" ").unwrap_or(rest.len());
                    let id = &rest[..id_end];

                    if !active_finding_ids.contains(id) && line.contains("- [ ]") {
                        *line = line.replace("- [ ]", "- [x]");
                        tasks_modified = true;

                        let checkbox_len = 5;
                        let end_title_pos = line.find("<!--").unwrap_or(line.len());
                        if checkbox_len <= end_title_pos {
                            let title = line[checkbox_len..end_title_pos].trim().to_string();
                            tasks_to_auto_resolve.push(title);
                        }
                    }
                }
            }
        }
        if tasks_modified {
            tasks_content = lines.join("\n");
        }

        if !new_tasks.is_empty() || tasks_modified {
            affected_files.push(".gantry/tasks.md".to_string());
            if write {
                new_tasks.sort();
                new_tasks.dedup();
                let header = "\n\n## Audit Auto-Generated Tasks\n\n";
                if !tasks_content.contains("## Audit Auto-Generated Tasks") {
                    tasks_content.push_str(header);
                } else {
                    tasks_content.push('\n');
                }
                for task in new_tasks {
                    tasks_content.push_str(&task);
                    tasks_content.push('\n');
                }
                fs::write(&tasks_path, &tasks_content).map_err(|e| e.to_string())?;

                payload = compile_launch_index_ctx(opts)?;
                let mut dummy_tasks = Vec::new();
                ingest_test_results_ctx(
                    vault_path,
                    project_root,
                    &mut payload.validation_errors,
                    &mut dummy_tasks,
                    &tasks_content,
                );
                detect_risky_areas(
                    &changed_files,
                    &payload.blockers,
                    &payload.qa_suites,
                    &mut payload.validation_errors,
                );
            }
        }
    }

    affected_files.push(".gantry/index.json".to_string());
    affected_files.sort();
    affected_files.dedup();

    payload.audit_preview = Some(AuditPreview {
        documents_to_create,
        tasks_to_create,
        tasks_to_auto_resolve,
        affected_files,
    });

    if write {
        let index_file_path = vault_path.join("index.json");
        let serialized = serde_json::to_string_pretty(&payload)
            .map_err(|e| format!("Failed to serialize index: {}", e))?;
        fs::write(index_file_path, serialized)
            .map_err(|e| format!("Failed to write index.json: {}", e))?;
    }

    Ok(payload)
}

pub(crate) fn get_base_git_ref(project_root: &Path, base_ref: Option<&str>) -> Option<String> {
    if let Some(r) = base_ref {
        if !r.trim().is_empty() {
            return Some(r.trim().to_string());
        }
    }

    // Try to get last release tag
    let output = std::process::Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .current_dir(project_root)
        .output()
        .ok()?;
    if output.status.success() {
        let tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !tag.is_empty() {
            return Some(tag);
        }
    }

    // Try merge base with main
    let output = std::process::Command::new("git")
        .args(["merge-base", "main", "HEAD"])
        .current_dir(project_root)
        .output()
        .ok()?;
    if output.status.success() {
        let base = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !base.is_empty() {
            return Some(base);
        }
    }

    // Try merge base with master
    let output = std::process::Command::new("git")
        .args(["merge-base", "master", "HEAD"])
        .current_dir(project_root)
        .output()
        .ok()?;
    if output.status.success() {
        let base = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !base.is_empty() {
            return Some(base);
        }
    }

    // Fallback to HEAD~1
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD~1"])
        .current_dir(project_root)
        .output()
        .ok()?;
    if output.status.success() {
        let base = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !base.is_empty() {
            return Some(base);
        }
    }

    None
}

// Impact summary logic moved to graph.rs

pub fn run_mcp_server() -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();

    use std::io::BufRead;
    while reader.read_line(&mut line).map_err(|e| e.to_string())? > 0 {
        let request_str = line.trim();
        if !request_str.is_empty() {
            if let Ok(request) = serde_json::from_str::<serde_json::Value>(request_str) {
                if let Some(response) = handle_mcp_request(request) {
                    if let Ok(response_str) = serde_json::to_string(&response) {
                        println!("{}", response_str);
                    }
                }
            }
        }
        line.clear();
    }
    Ok(())
}

pub fn run_mcp_server_ctx(opts: &EvaluationOptions) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();

    use std::io::BufRead;
    while reader.read_line(&mut line).map_err(|e| e.to_string())? > 0 {
        let request_str = line.trim();
        if !request_str.is_empty() {
            if let Ok(request) = serde_json::from_str::<serde_json::Value>(request_str) {
                if let Some(response) = handle_mcp_request_ctx(request, opts) {
                    if let Ok(response_str) = serde_json::to_string(&response) {
                        println!("{}", response_str);
                    }
                }
            }
        }
        line.clear();
    }
    Ok(())
}

fn handle_mcp_request_ctx(
    req: serde_json::Value,
    opts: &EvaluationOptions,
) -> Option<serde_json::Value> {
    let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
    let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");

    match method {
        "initialize" => Some(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "gantry-mcp",
                    "version": "1.0.0"
                }
            }
        })),
        "notifications/initialized" => None,
        "tools/list" => handle_mcp_request(req.clone()),
        "tools/call" => {
            let params = req.get("params");
            let tool_name = params
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let arguments = params.and_then(|p| p.get("arguments"));

            let result_text = execute_mcp_tool_ctx(tool_name, arguments, opts);

            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": result_text
                        }
                    ]
                }
            }))
        }
        _ => handle_mcp_request(req),
    }
}

fn execute_mcp_tool_ctx(
    name: &str,
    args_opt: Option<&serde_json::Value>,
    opts: &EvaluationOptions,
) -> String {
    // Compile once; both the context-aware branches and the fallback need the payload.
    let payload = match compile_launch_index_ctx(opts) {
        Ok(p) => p,
        Err(e) => return format!("Error compiling Gantry vault index: {}", e),
    };

    match name {
        "list_tasks" => match list_pending_tasks_ctx(&opts.vault.vault_path) {
            Ok(tasks) => {
                let pending: Vec<&GantryTask> = tasks.iter().filter(|t| !t.completed).collect();
                if pending.is_empty() {
                    "No pending repair tasks found in tasks.md.".to_string()
                } else {
                    let mut out = format!("Pending Tasks ({}):\n", pending.len());
                    for t in pending {
                        out.push_str(&format!(
                            "- [{}] (Severity: {}) {}\n",
                            t.id, t.severity, t.title
                        ));
                        out.push_str(&format!("  Evidence: {}\n", t.evidence));
                    }
                    out
                }
            }
            Err(e) => format!("Failed to list pending tasks: {}", e),
        },
        "get_impact_summary" => {
            let base_ref = args_opt
                .and_then(|a| a.get("base_ref"))
                .and_then(|b| b.as_str());
            match get_impact_summary_ctx(&opts.vault, base_ref) {
                Ok(summary) => {
                    let mut out = "=== Git Change Impact Summary ===\n".to_string();
                    out.push_str(&format!(
                        "Changed files count: {}\n",
                        summary.changed_files.len()
                    ));
                    out.push_str(&format!(
                        "Impacted Blockers:  {:?}\n",
                        summary.impacted_blockers
                    ));
                    out.push_str(&format!("Impacted QA Suites: {:?}\n", summary.impacted_qa));
                    out.push_str(&format!(
                        "Uncovered Folders:  {:?}\n",
                        summary.uncovered_changes
                    ));
                    out.push_str("Suggested verification tasks:\n");
                    for t in &summary.suggested_tasks {
                        out.push_str(&format!("  - {}\n", t));
                    }
                    out
                }
                Err(e) => format!("Failed to calculate change impact: {}", e),
            }
        }
        // All remaining tools (get_launch_status, list_evidence, get_pr_comment, …) use the payload.
        _ => execute_mcp_tool_with_payload(name, args_opt, &payload),
    }
}

fn handle_mcp_request(req: serde_json::Value) -> Option<serde_json::Value> {
    let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
    let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");

    match method {
        "initialize" => Some(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "gantry-mcp",
                    "version": "1.0.0"
                }
            }
        })),
        "notifications/initialized" => None,
        "tools/list" => Some(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [
                    {
                        "name": "get_launch_status",
                        "description": "Get overall launch readiness status, scores, reasons and category breakdowns",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "list_blockers",
                        "description": "Get detailed list of launch blockers with severities, statuses, and owners",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "list_stale_docs",
                        "description": "Get list of stale documents/files needing review",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "list_tasks",
                        "description": "Get active tasks from tasks.md",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "list_evidence",
                        "description": "Get launch evidence graph details (test results, required gates, signoffs)",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "get_impact_summary",
                        "description": "Get Git codebase change impact mapping, changed files, and uncovered folders",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "base_ref": {
                                    "type": "string",
                                    "description": "Optional Git base tag or commit ref to diff against"
                                }
                            }
                        }
                    },
                    {
                        "name": "get_pr_comment",
                        "description": "Get a complete Gantry launch report formatted as a Markdown comment suitable for GitHub PRs",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    }
                ]
            }
        })),
        "tools/call" => {
            let params = req.get("params");
            let tool_name = params
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let arguments = params.and_then(|p| p.get("arguments"));

            let result_text = execute_mcp_tool(tool_name, arguments);

            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": result_text
                        }
                    ]
                }
            }))
        }
        _ => Some(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("Method '{}' not found", method)
            }
        })),
    }
}

fn execute_mcp_tool(name: &str, args_opt: Option<&serde_json::Value>) -> String {
    let payload = match compile_launch_index() {
        Ok(p) => p,
        Err(e) => return format!("Error compiling Gantry vault index: {}", e),
    };
    execute_mcp_tool_with_payload(name, args_opt, &payload)
}

/// Shared tool dispatch that uses a pre-compiled payload.
fn execute_mcp_tool_with_payload(
    name: &str,
    args_opt: Option<&serde_json::Value>,
    payload: &IndexPayload,
) -> String {
    match name {
        "get_launch_status" => {
            let mut summary = "=== Gantry Launch Readiness Status ===\n".to_string();
            summary.push_str(&format!("Vault Name:      {}\n", payload.vault_name));
            summary.push_str(&format!("Release Target:  {}\n", payload.release));
            summary.push_str(&format!("Target Date:     {}\n", payload.target_date));
            summary.push_str(&format!("Readiness State: {}\n", payload.readiness));
            summary.push_str(&format!(
                "Readiness Score: {}/100\n",
                payload.readiness_score
            ));
            summary.push_str("Category Scores:\n");
            let mut cats: Vec<(&String, &u8)> = payload.category_scores.iter().collect();
            cats.sort_by(|a, b| a.0.cmp(b.0));
            for (cat, score) in cats {
                summary.push_str(&format!("  - {}: {}/100\n", cat, score));
            }
            if !payload.readiness_reasons.is_empty() {
                summary.push_str("Readiness Reasons / Gate Warnings:\n");
                for reason in &payload.readiness_reasons {
                    summary.push_str(&format!("  - {}\n", reason));
                }
            }
            summary
        }
        "list_blockers" => {
            let active_blockers: Vec<&BlockerEntry> = payload
                .blockers
                .iter()
                .filter(|b| b.status.to_lowercase() != "resolved")
                .collect();
            if active_blockers.is_empty() {
                "No active launch blockers.".to_string()
            } else {
                let mut out = format!("Active Launch Blockers ({}):\n", active_blockers.len());
                for b in active_blockers {
                    out.push_str(&format!(
                        "- [{}] {} (Severity: {}, Owner: {}, Status: {})\n",
                        b.id, b.title, b.severity, b.owner, b.status
                    ));
                    if !b.source_files.is_empty() {
                        out.push_str(&format!("  Source files: {:?}\n", b.source_files));
                    }
                }
                out
            }
        }
        "list_stale_docs" => {
            if payload.stale_docs.is_empty() {
                "No stale documents found.".to_string()
            } else {
                let mut out = "Stale Documents / Checklists needing review:\n".to_string();
                for doc in &payload.stale_docs {
                    out.push_str(&format!("  - {}\n", doc));
                }
                out
            }
        }
        "list_tasks" => match list_pending_tasks() {
            Ok(tasks) => {
                let pending: Vec<&GantryTask> = tasks.iter().filter(|t| !t.completed).collect();
                if pending.is_empty() {
                    "No pending repair tasks found in tasks.md.".to_string()
                } else {
                    let mut out = format!("Pending Tasks ({}):\n", pending.len());
                    for t in pending {
                        out.push_str(&format!(
                            "- [{}] (Severity: {}) {}\n",
                            t.id, t.severity, t.title
                        ));
                        out.push_str(&format!("  Evidence: {}\n", t.evidence));
                    }
                    out
                }
            }
            Err(e) => format!("Failed to list pending tasks: {}", e),
        },
        "list_evidence" => {
            let mut out = "=== Gantry Launch Evidence Graph ===\n".to_string();
            out.push_str(&format!(
                "QA Regression: {}/{} tests passed\n",
                payload.qa_completed, payload.qa_total
            ));
            out.push_str(&format!(
                "Validation issues: {}\n",
                payload.validation_errors.len()
            ));
            for err in &payload.validation_errors {
                out.push_str(&format!(
                    "  - [{}] {}: {}\n",
                    err.code, err.file, err.message
                ));
            }
            out.push_str(&format!(
                "Broken wiki links: {}\n",
                payload.broken_links.len()
            ));
            for link in &payload.broken_links {
                out.push_str(&format!("  - {} in {}\n", link.message, link.file));
            }
            out.push_str("Stakeholder Sign-offs:\n");
            for gate in &payload.go_no_go_gates {
                out.push_str(&format!(
                    "  - {}: {} ({})\n",
                    gate.owner, gate.name, gate.status
                ));
            }
            out
        }
        "get_impact_summary" => {
            let base_ref = args_opt
                .and_then(|a| a.get("base_ref"))
                .and_then(|b| b.as_str());
            match get_impact_summary_ex(base_ref) {
                Ok(summary) => {
                    let mut out = "=== Git Change Impact Summary ===\n".to_string();
                    out.push_str(&format!(
                        "Changed files count: {}\n",
                        summary.changed_files.len()
                    ));
                    out.push_str(&format!(
                        "Impacted Blockers:  {:?}\n",
                        summary.impacted_blockers
                    ));
                    out.push_str(&format!("Impacted QA Suites: {:?}\n", summary.impacted_qa));
                    out.push_str(&format!(
                        "Uncovered Folders:  {:?}\n",
                        summary.uncovered_changes
                    ));
                    out.push_str("Suggested verification tasks:\n");
                    for t in &summary.suggested_tasks {
                        out.push_str(&format!("  - {}\n", t));
                    }
                    out
                }
                Err(e) => format!("Failed to calculate change impact: {}", e),
            }
        }
        "get_pr_comment" => generate_markdown_report(payload),
        _ => format!("Unknown tool name: {}", name),
    }
}

pub fn generate_markdown_report(payload: &IndexPayload) -> String {
    let mut md = String::new();

    md.push_str("# Gantry Launch Readiness Report\n\n");

    // Status & Readiness
    md.push_str("## Status & Readiness\n\n");
    md.push_str("| Metric | Value |\n");
    md.push_str("| :--- | :--- |\n");
    md.push_str(&format!("| **Vault Name** | {} |\n", payload.vault_name));
    md.push_str(&format!("| **Release Target** | `{}` |\n", payload.release));
    md.push_str(&format!(
        "| **Target Date** | `{}` |\n",
        payload.target_date
    ));

    let status_label = match payload.readiness.as_str() {
        "Go" => "Go",
        "At Risk" => "At Risk",
        _ => "No-Go",
    };
    md.push_str(&format!("| **Readiness State** | **{}** |\n", status_label));

    // Progress bar for score
    let score = payload.readiness_score;
    let filled_blocks = (score / 5) as usize;
    let empty_blocks = 20 - filled_blocks;
    let bar = format!("{}{}", "█".repeat(filled_blocks), "░".repeat(empty_blocks));
    md.push_str(&format!(
        "| **Readiness Score** | `[{}]` **{}/100** |\n\n",
        bar, score
    ));

    // Reasons
    if !payload.readiness_reasons.is_empty() {
        md.push_str("### Readiness Reasons / Gate Warnings\n");
        for reason in &payload.readiness_reasons {
            md.push_str(&format!("- {}\n", reason));
        }
        md.push('\n');
    }

    // Blocking Causes
    md.push_str("## Blocking Causes\n\n");
    let mut printed = 0;
    let mut blocking_causes = Vec::new();
    for blocker in &payload.blockers {
        if blocker.status.to_lowercase() != "resolved" {
            blocking_causes.push(format!(
                "- **[BLOCKER]** {}: {} ({})",
                blocker.id, blocker.title, blocker.owner
            ));
            printed += 1;
            if printed >= 5 {
                break;
            }
        }
    }
    if printed < 5 {
        for gate in &payload.go_no_go_gates {
            if gate.status != "checked" && !gate.waived {
                blocking_causes.push(format!(
                    "- **[GATE]** {}: {} ({})",
                    gate.status.to_uppercase(),
                    gate.name,
                    gate.owner
                ));
                printed += 1;
                if printed >= 5 {
                    break;
                }
            }
        }
    }
    if printed < 5 {
        for signoff in &payload.signoffs {
            if signoff.status != "Approved" && !signoff.waived {
                blocking_causes.push(format!(
                    "- **[SIGNOFF]** {}: {} ({})",
                    signoff.status.to_uppercase(),
                    signoff.name,
                    signoff.role
                ));
                printed += 1;
                if printed >= 5 {
                    break;
                }
            }
        }
    }
    if printed < 5 {
        for err in &payload.validation_errors {
            blocking_causes.push(format!("- **[ERROR]** {}: {}", err.file, err.message));
            printed += 1;
            if printed >= 5 {
                break;
            }
        }
    }

    if blocking_causes.is_empty() {
        md.push_str("All checks nominal. Clear for launch!\n\n");
    } else {
        md.push_str("Top Actionable Blockers:\n");
        for cause in blocking_causes {
            md.push_str(&format!("{}\n", cause));
        }
        md.push('\n');
    }

    // Evidence & Verification Paths
    md.push_str("## Evidence & Verification Paths\n\n");

    // QA Suites
    md.push_str("### QA Regression Test Suites\n\n");
    if payload.qa_suites.is_empty() {
        md.push_str("No regression test suites configured.\n\n");
    } else {
        md.push_str("| Suite | Tests Passed | Status |\n");
        md.push_str("| :--- | :--- | :--- |\n");
        for qa in &payload.qa_suites {
            let status = if qa.stale {
                "Stale (Source Modified)"
            } else if qa.completed_tests == qa.total_tests {
                "Complete"
            } else {
                "In Progress"
            };
            md.push_str(&format!(
                "| `{}` | {}/{} | {} |\n",
                qa.title, qa.completed_tests, qa.total_tests, status
            ));
        }
        md.push('\n');
    }

    // Docs Health
    md.push_str("### Documentation & Validation Health\n\n");
    md.push_str(&format!(
        "Health Status: **{}**\n\n",
        payload.docs_health_status.to_uppercase()
    ));

    if !payload.missing_docs.is_empty()
        || !payload.stale_docs.is_empty()
        || !payload.validation_errors.is_empty()
        || !payload.broken_links.is_empty()
    {
        if !payload.missing_docs.is_empty() {
            md.push_str("#### Missing Recommended/Required Files\n");
            for doc in &payload.missing_docs {
                md.push_str(&format!("- `{}`\n", doc));
            }
            md.push('\n');
        }
        if !payload.stale_docs.is_empty() {
            md.push_str("#### Stale Documents (Needs review because source file changed)\n");
            for doc in &payload.stale_docs {
                md.push_str(&format!("- `{}`\n", doc));
            }
            md.push('\n');
        }
        if !payload.validation_errors.is_empty() {
            md.push_str("#### Validation & Schema Errors\n");
            for err in &payload.validation_errors {
                if err.code != "MISSING_DOC" && err.code != "STALE_DOC" {
                    md.push_str(&format!(
                        "- **{}** (`{}`): {}\n",
                        err.file, err.code, err.message
                    ));
                }
            }
            md.push('\n');
        }
        if !payload.broken_links.is_empty() {
            md.push_str("#### Broken Wiki Links\n");
            for link in &payload.broken_links {
                md.push_str(&format!("- **{}**: {}\n", link.file, link.message));
            }
            md.push('\n');
        }
    } else {
        md.push_str("Documentation is healthy and up-to-date!\n\n");
    }

    // Telemetry Status
    md.push_str("### Telemetry Status\n\n");
    if let Some(env_payload) = payload.environment_results.get(&payload.active_environment) {
        md.push_str(&format!(
            "Environment: **{}**\n",
            payload.active_environment
        ));
        md.push_str(&format!(
            "Telemetry Status: **{}**\n",
            env_payload.telemetry_status.to_uppercase()
        ));
        if let Some(ref obs) = env_payload.observed_telemetry {
            md.push_str(&format!("Source: `{}`\n", obs.source));
            if let Some(cfr) = obs.crash_free_rate {
                md.push_str(&format!("Crash Free Rate: `{:.2}%`\n", cfr));
            }
            if !obs.metrics.is_empty() {
                md.push_str("Metrics:\n");
                for (k, v) in &obs.metrics {
                    md.push_str(&format!("- `{}`: `{}`\n", k, v));
                }
            }
            md.push_str(&format!("Fetched At: `{}`\n", obs.fetched_at));
        }
    } else {
        md.push_str("No telemetry data available for the active environment.\n");
    }
    md.push('\n');

    // Next Actions
    md.push_str("## Next Actions\n\n");
    let mut tasks_shown = 0;

    // First check if we have any tasks from impact summary
    if let Some(ref impact) = payload.impact_summary {
        for task in &impact.suggested_tasks {
            md.push_str(&format!("- [ ] {} (Change Impact)\n", task));
            tasks_shown += 1;
            if tasks_shown >= 3 {
                break;
            }
        }
    }

    // If we need more, query from tasks.md
    if tasks_shown < 3 {
        if let Ok(tasks) = list_pending_tasks() {
            for task in tasks.iter().filter(|t| !t.completed) {
                md.push_str(&format!(
                    "- [ ] **{}** ({}) - Severity: `{}`\n",
                    task.title, task.id, task.severity
                ));
                tasks_shown += 1;
                if tasks_shown >= 3 {
                    break;
                }
            }
        }
    }

    if tasks_shown == 0 {
        md.push_str("All repair tasks are resolved!\n\n");
    } else {
        md.push('\n');
    }

    md.push_str("### Local CLI Usage\n\n");
    md.push_str("To work with launch readiness locally, run:\n");
    md.push_str("```bash\n");
    md.push_str("# Initialize/repair vault\n");
    md.push_str("gantry init --auto\n\n");
    md.push_str("# Audit launch gates and generate repair tasks\n");
    md.push_str("gantry audit\n\n");
    md.push_str("# Run launch validation gate (CI check)\n");
    md.push_str("gantry validate --ci\n");
    md.push_str("```\n\n");
    md.push_str("Gantry Launch Readiness System - Open Source Edition\n");

    md
}

/// Generate a self-contained audit certificate for the current release.
///
/// `format` is one of `"html"` (default, print-to-PDF ready), `"json"`, or `"md"`.
/// The certificate includes gate verdicts, sign-offs, waivers, evidence, and the
/// tamper-verification status of the local snapshot chain.
pub fn export_audit_certificate(payload: &IndexPayload, format: &str) -> String {
    let chain_report = snapshot::verify_chain().unwrap_or(snapshot::ChainReport {
        intact: false,
        snapshots_checked: 0,
        broken_at: None,
        broken_reason: Some("Could not read snapshot directory.".to_string()),
    });

    match format {
        "json" => {
            let cert = serde_json::json!({
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "vault_name": payload.vault_name,
                "release": payload.release,
                "target_date": payload.target_date,
                "readiness": payload.readiness,
                "readiness_score": payload.readiness_score,
                "open_blockers": payload.open_blockers,
                "qa_total": payload.qa_total,
                "qa_completed": payload.qa_completed,
                "signoffs": payload.signoffs,
                "active_waivers": payload.active_waivers,
                "evidence_records": payload.evidence_records,
                "go_no_go_gates": payload.go_no_go_gates,
                "readiness_reasons": payload.readiness_reasons,
                "chain_verification": {
                    "intact": chain_report.intact,
                    "snapshots_checked": chain_report.snapshots_checked,
                    "broken_at": chain_report.broken_at,
                    "broken_reason": chain_report.broken_reason,
                }
            });
            serde_json::to_string_pretty(&cert).unwrap_or_default()
        }
        "md" => {
            let mut md = generate_markdown_report(payload);
            let chain_status = if chain_report.intact {
                format!(
                    "✓ Intact ({} snapshots verified)",
                    chain_report.snapshots_checked
                )
            } else {
                format!(
                    "✗ Broken at `{}` — {}",
                    chain_report.broken_at.as_deref().unwrap_or("unknown"),
                    chain_report.broken_reason.as_deref().unwrap_or("")
                )
            };
            md.push_str("\n## Audit Certificate\n\n");
            md.push_str(&format!(
                "- **Generated:** {}\n",
                chrono::Utc::now().format("%Y-%m-%d %H:%M UTC")
            ));
            md.push_str(&format!("- **History chain:** {}\n", chain_status));
            md
        }
        _ => {
            // HTML — standalone, print-to-PDF ready
            let status_color = match payload.readiness.as_str() {
                "Go" => "#16a34a",
                "At Risk" => "#d97706",
                _ => "#dc2626",
            };
            let chain_status_html = if chain_report.intact {
                format!(
                    "<span style='color:#16a34a'>✓ Intact ({} snapshots verified)</span>",
                    chain_report.snapshots_checked
                )
            } else {
                format!(
                    "<span style='color:#dc2626'>✗ Broken at <code>{}</code> — {}</span>",
                    html_escape(chain_report.broken_at.as_deref().unwrap_or("unknown")),
                    html_escape(chain_report.broken_reason.as_deref().unwrap_or(""))
                )
            };

            let generated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

            let mut signoffs_html = String::new();
            for s in &payload.signoffs {
                let badge = if s.status == "Approved" {
                    "<span style='color:#16a34a'>✓ Approved</span>"
                } else if s.waived {
                    "<span style='color:#d97706'>⚠ Waived</span>"
                } else {
                    "<span style='color:#dc2626'>✗ Pending</span>"
                };
                signoffs_html.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                    html_escape(&s.name),
                    html_escape(&s.role),
                    badge
                ));
            }
            if signoffs_html.is_empty() {
                signoffs_html =
                    "<tr><td colspan='3' style='color:#6b7280'>No sign-offs configured.</td></tr>"
                        .to_string();
            }

            let mut waivers_html = String::new();
            for w in &payload.active_waivers {
                waivers_html.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    html_escape(&w.id),
                    html_escape(&w.reason),
                    html_escape(&w.approved_by),
                    html_escape(&w.expires_at)
                ));
            }
            if waivers_html.is_empty() {
                waivers_html =
                    "<tr><td colspan='4' style='color:#6b7280'>No active waivers.</td></tr>"
                        .to_string();
            }

            let mut evidence_html = String::new();
            for e in &payload.evidence_records {
                evidence_html.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    html_escape(&e.id),
                    html_escape(&e.kind),
                    html_escape(&e.title),
                    html_escape(&e.created_at)
                ));
            }
            if evidence_html.is_empty() {
                evidence_html =
                    "<tr><td colspan='4' style='color:#6b7280'>No evidence records.</td></tr>"
                        .to_string();
            }

            let mut gates_html = String::new();
            for g in &payload.go_no_go_gates {
                let badge = if g.status == "checked" {
                    "<span style='color:#16a34a'>✓ Pass</span>"
                } else if g.waived {
                    "<span style='color:#d97706'>⚠ Waived</span>"
                } else {
                    "<span style='color:#dc2626'>✗ Blocked</span>"
                };
                gates_html.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                    html_escape(&g.name),
                    html_escape(&g.owner),
                    badge
                ));
            }
            if gates_html.is_empty() {
                gates_html =
                    "<tr><td colspan='3' style='color:#6b7280'>No gates configured.</td></tr>"
                        .to_string();
            }

            format!(
                r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>Gantry Audit Certificate — {release} {readiness}</title>
<style>
  body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; max-width: 860px; margin: 40px auto; padding: 0 24px; color: #111; font-size: 14px; }}
  h1 {{ font-size: 22px; margin-bottom: 4px; }}
  h2 {{ font-size: 15px; margin: 28px 0 8px; border-bottom: 1px solid #e5e7eb; padding-bottom: 4px; color: #374151; }}
  .verdict {{ display: inline-block; padding: 4px 14px; border-radius: 4px; font-weight: 700; font-size: 18px; color: #fff; background: {status_color}; margin: 8px 0 16px; }}
  .meta {{ color: #6b7280; font-size: 12px; margin-bottom: 24px; }}
  .score-bar {{ display: inline-block; font-family: monospace; background: #f3f4f6; padding: 2px 8px; border-radius: 3px; }}
  table {{ width: 100%; border-collapse: collapse; margin-bottom: 16px; }}
  th {{ text-align: left; padding: 6px 10px; background: #f9fafb; font-size: 12px; color: #6b7280; border-bottom: 1px solid #e5e7eb; }}
  td {{ padding: 6px 10px; border-bottom: 1px solid #f3f4f6; vertical-align: top; }}
  .chain-box {{ background: #f9fafb; border: 1px solid #e5e7eb; border-radius: 6px; padding: 12px 16px; margin-bottom: 24px; font-size: 13px; }}
  .footer {{ margin-top: 40px; padding-top: 16px; border-top: 1px solid #e5e7eb; color: #9ca3af; font-size: 11px; }}
  @media print {{ body {{ margin: 20px; }} }}
</style>
</head>
<body>
<h1>Gantry Audit Certificate</h1>
<p class="meta">Vault: <strong>{vault_name}</strong> &nbsp;|&nbsp; Release: <strong>{release}</strong> &nbsp;|&nbsp; Generated: {generated_at}</p>
<div class="verdict">{readiness}</div>
<p>Readiness score: <span class="score-bar">{score}/100</span> &nbsp;|&nbsp; Open blockers: <strong>{open_blockers}</strong> &nbsp;|&nbsp; QA: <strong>{qa_completed}/{qa_total}</strong></p>

<div class="chain-box">
  <strong>History chain verification:</strong> {chain_status_html}
</div>

<h2>Launch Gates</h2>
<table><thead><tr><th>Gate</th><th>Owner</th><th>Status</th></tr></thead><tbody>{gates_html}</tbody></table>

<h2>Sign-offs</h2>
<table><thead><tr><th>Name</th><th>Role</th><th>Status</th></tr></thead><tbody>{signoffs_html}</tbody></table>

<h2>Active Risk Waivers</h2>
<table><thead><tr><th>ID</th><th>Reason</th><th>Approved By</th><th>Expires</th></tr></thead><tbody>{waivers_html}</tbody></table>

<h2>Evidence Records</h2>
<table><thead><tr><th>ID</th><th>Kind</th><th>Title</th><th>Created</th></tr></thead><tbody>{evidence_html}</tbody></table>

<div class="footer">Generated by Gantry · <a href="https://gantryapp.com">gantryapp.com</a> · This certificate reflects vault state at time of export. Chain verification confirms the local snapshot history has not been edited.</div>
</body>
</html>"#,
                release = html_escape(&payload.release),
                readiness = html_escape(&payload.readiness),
                vault_name = html_escape(&payload.vault_name),
                status_color = status_color,
                generated_at = generated_at,
                score = payload.readiness_score,
                open_blockers = payload.open_blockers,
                qa_completed = payload.qa_completed,
                qa_total = payload.qa_total,
                chain_status_html = chain_status_html,
                gates_html = gates_html,
                signoffs_html = signoffs_html,
                waivers_html = waivers_html,
                evidence_html = evidence_html,
            )
        }
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    #[test]
    fn test_git_commit_date_preserves_recorded_timezone() {
        // This instant is August 7 in UTC, but the commit was made on August 6
        // in its recorded timezone. Freshness follows the commit's calendar date.
        let committed_at = chrono::DateTime::parse_from_rfc3339("2026-08-06T23:30:00-07:00")
            .expect("valid RFC 3339 commit date");
        assert_eq!(
            committed_at.with_timezone(&Utc).date_naive(),
            NaiveDate::from_ymd_opt(2026, 8, 7).unwrap()
        );
        assert_eq!(
            parse_git_commit_date("2026-08-06T23:30:00-07:00"),
            NaiveDate::from_ymd_opt(2026, 8, 6)
        );
    }

    #[test]
    fn test_generate_markdown_report_no_emojis() {
        let payload = IndexPayload {
            vault_name: "Test Vault".to_string(),
            release: "1.0.0".to_string(),
            readiness: "At Risk".to_string(),
            target_date: "2026-06-15".to_string(),
            open_blockers: 0,
            qa_total: 0,
            qa_completed: 0,
            stale_docs: vec![],
            missing_docs: vec![],
            blockers: vec![],
            qa_suites: vec![],
            last_updated: "12345678".to_string(),
            validation_errors: vec![],
            broken_links: vec![],
            source_missing: vec![],
            docs_health_status: "healthy".to_string(),
            go_no_go_gates: vec![],
            signoffs: vec![],
            readiness_reasons: vec![],
            readiness_score: 85,
            category_scores: std::collections::HashMap::new(),
            evidence_graph: vec![],
            score_reasons: std::collections::HashMap::new(),
            audit_preview: None,
            impact_summary: None,
            report_url: None,
            active_environment: "production".to_string(),
            environment_results: std::collections::HashMap::new(),
            active_waivers: vec![],
            expired_waivers: vec![],
            waived_items: vec![],
            scorecard_results: vec![],
            evidence_records: vec![],
            release_manifest_path: None,
        };

        let report = generate_markdown_report(&payload);

        // Assert headers are restructured and have no emojis
        assert!(report.contains("# Gantry Launch Readiness Report"));
        assert!(report.contains("## Status & Readiness"));
        assert!(report.contains("## Blocking Causes"));
        assert!(report.contains("## Evidence & Verification Paths"));
        assert!(report.contains("## Next Actions"));

        // Assert no emojis in common headings or status indicators
        assert!(!report.contains("🚀"));
        assert!(!report.contains("🛑"));
        assert!(!report.contains("🧪"));
        assert!(!report.contains("📂"));
        assert!(!report.contains("📋"));
        assert!(!report.contains("💻"));
        assert!(!report.contains("🎉"));
        assert!(!report.contains("🟢"));
        assert!(!report.contains("🟡"));
        assert!(!report.contains("🔴"));
    }

    #[test]
    fn test_safe_resolve_vault_path_guards() {
        // Traversal rejection
        assert!(safe_resolve_vault_path("../outside.txt").is_err());
        assert!(safe_resolve_vault_path("blockers/../../outside.txt").is_err());

        // Absolute path rejection
        #[cfg(not(windows))]
        assert!(safe_resolve_vault_path("/etc/passwd").is_err());
        #[cfg(windows)]
        assert!(safe_resolve_vault_path("C:\\Windows").is_err());

        // Valid path within vault
        let resolved = safe_resolve_vault_path("blockers/BLK-101.md");
        assert!(resolved.is_ok());
        let path = resolved.unwrap();
        assert!(path.ends_with(".gantry/blockers/BLK-101.md"));
    }

    #[test]
    fn test_safe_resolve_project_path_guards() {
        // Traversal rejection
        assert!(safe_resolve_project_path("../outside.txt").is_err());

        // Absolute path rejection
        #[cfg(not(windows))]
        assert!(safe_resolve_project_path("/etc/passwd").is_err());
        #[cfg(windows)]
        assert!(safe_resolve_project_path("C:\\Windows").is_err());

        // Valid path within project
        let resolved = safe_resolve_project_path("src/purchases/paywall.ts");
        assert!(resolved.is_ok());
        let path = resolved.unwrap();
        assert!(path.ends_with("src/purchases/paywall.ts"));
    }

    #[test]
    fn test_parse_frontmatter_valid() {
        let content = "---\ntype: blocker\nid: BLK-101\n---\n# Blocker Title";
        let parsed = parse_frontmatter(content);
        assert!(parsed.is_ok());
        let (val, body) = parsed.unwrap();
        assert_eq!(val.get("type").and_then(|v| v.as_str()), Some("blocker"));
        assert_eq!(val.get("id").and_then(|v| v.as_str()), Some("BLK-101"));
        assert_eq!(body.trim(), "# Blocker Title");
    }

    #[test]
    fn test_parse_frontmatter_empty() {
        let content = "# Plain markdown no frontmatter";
        let parsed = parse_frontmatter(content);
        assert!(parsed.is_ok());
        let (val, body) = parsed.unwrap();
        assert!(val.is_object());
        assert_eq!(body, content);
    }

    #[test]
    fn test_find_wiki_links() {
        let content = "Check [[BLK-101]] and [[qa/regression]] for details.";
        let links = find_wiki_links(content);
        assert_eq!(
            links,
            vec!["BLK-101".to_string(), "qa/regression".to_string()]
        );
    }

    #[test]
    fn test_parse_gates_and_signoffs() {
        let content = "- [ ] **Product:** Copy updated. <!-- @pm_dan -->\n- [x] **QA:** Verified. <!-- @qa_bob 2026-05-27 -->";
        let (gates, signoffs) = parse_gates_and_signoffs(content);

        assert_eq!(gates.len(), 2);
        assert_eq!(gates[0].name, "**Product:** Copy updated. <!-- @pm_dan -->");
        assert_eq!(gates[0].status, "unchecked");
        assert_eq!(gates[0].owner, "@pm_dan");

        assert_eq!(
            gates[1].name,
            "**QA:** Verified. <!-- @qa_bob 2026-05-27 -->"
        );
        assert_eq!(gates[1].status, "checked");
        assert_eq!(gates[1].owner, "@qa_bob");

        assert_eq!(signoffs.len(), 2);
        assert_eq!(signoffs[0].name, "@pm_dan");
        assert_eq!(signoffs[0].role, "Product");
        assert_eq!(signoffs[0].status, "Pending");
        assert_eq!(signoffs[0].date, "");

        assert_eq!(signoffs[1].name, "@qa_bob");
        assert_eq!(signoffs[1].role, "QA");
        assert_eq!(signoffs[1].status, "Approved");
        assert_eq!(signoffs[1].date, "2026-05-27");
    }

    #[test]
    fn test_readiness_score_calculation() {
        // Run index compilation on current vault to verify it runs and calculates scores
        let res = compile_launch_index();
        if let Ok(payload) = res {
            assert!(payload.readiness_score <= 100);
            assert!(payload.category_scores.contains_key("Product"));
            assert!(payload.category_scores.contains_key("Engineering"));
            assert!(payload.category_scores.contains_key("QA"));
            assert!(payload.category_scores.contains_key("Store"));
            assert!(payload.category_scores.contains_key("Billing"));
            assert!(payload.category_scores.contains_key("Monitoring"));
            assert!(payload.category_scores.contains_key("Support"));

            let prod_score = payload.category_scores.get("Product").unwrap();
            let eng_score = payload.category_scores.get("Engineering").unwrap();
            let qa_score = payload.category_scores.get("QA").unwrap();

            assert!(*prod_score <= 100);
            assert!(*eng_score <= 100);
            assert!(*qa_score <= 100);
        }
    }

    #[test]
    fn test_parse_junit_xml_failures() {
        let xml_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="com.example.AppTest" tests="2" failures="1" errors="0" time="0.05">
    <testcase name="testSuccess" classname="com.example.AppTest" time="0.02"/>
    <testcase name="testFailure" classname="com.example.AppTest" time="0.03">
        <failure message="Assertion failed: expected true but got false">details</failure>
    </testcase>
</testsuite>"#;
        let mut issues = Vec::new();
        let mut tasks_to_add = Vec::new();
        parse_junit_xml(
            xml_content,
            "test-report.xml",
            &mut issues,
            &mut tasks_to_add,
            "",
        );

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "FAILING_TEST");
        assert!(issues[0].message.contains("testFailure"));
        assert!(issues[0].message.contains("Assertion failed"));

        assert_eq!(tasks_to_add.len(), 1);
        assert!(tasks_to_add[0].contains("gantry:id:failed-test-com.example.AppTest.testFailure"));
    }

    #[test]
    fn test_readiness_score_no_go_capping() {
        let mut category_scores = std::collections::HashMap::new();
        category_scores.insert("Product".to_string(), 90);
        category_scores.insert("Engineering".to_string(), 95);
        category_scores.insert("QA".to_string(), 100);

        let eng_score = 95;
        let qa_score = 100;
        let product_score = 90;
        let gate_pct = 100;

        let mut readiness_score = ((eng_score as f32 * 0.30)
            + (qa_score as f32 * 0.30)
            + (product_score as f32 * 0.20)
            + (gate_pct as f32 * 0.20))
            .round() as u8;

        let readiness = "No-Go";
        if readiness == "No-Go" {
            readiness_score = readiness_score.min(49);
        }

        assert_eq!(readiness_score, 49);
    }

    #[test]
    fn test_task_metadata_comment_parsing() {
        let task_line = "- [ ] Fix failing test: com.example.AppTest.testFailure <!-- gantry:id:failed-test-com.example.AppTest.testFailure source:test_report severity:P0 evidence:test-report.xml created_at:2026-05-27 -->";

        let mut id = String::new();
        let mut source = String::new();
        let mut severity = String::new();
        let mut evidence = String::new();

        if let Some(start_comment) = task_line.find("<!--") {
            if let Some(end_comment) = task_line[start_comment..].find("-->") {
                let comment = &task_line[start_comment + 4..start_comment + end_comment];
                for part in comment.split_whitespace() {
                    if part.starts_with("gantry:id:") {
                        id = part.replace("gantry:id:", "");
                    } else if part.starts_with("source:") {
                        source = part.replace("source:", "");
                    } else if part.starts_with("severity:") {
                        severity = part.replace("severity:", "");
                    } else if part.starts_with("evidence:") {
                        evidence = part.replace("evidence:", "");
                    }
                }
            }
        }

        assert_eq!(id, "failed-test-com.example.AppTest.testFailure");
        assert_eq!(source, "test_report");
        assert_eq!(severity, "P0");
        assert_eq!(evidence, "test-report.xml");
    }

    #[test]
    fn test_parse_playwright_json_failures() {
        let content = r#"{
            "suites": [
                {
                    "specs": [
                        {
                            "title": "should display home page logo",
                            "tests": [
                                {
                                    "results": [
                                        {
                                            "status": "failed"
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                }
            ]
        }"#;
        let json: JsonValue = serde_json::from_str(content).unwrap();
        let mut issues = Vec::new();
        let mut tasks_to_add = Vec::new();
        parse_playwright_json(
            &json,
            "playwright-report.json",
            &mut issues,
            &mut tasks_to_add,
            "",
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "FAILING_TEST");
        assert!(issues[0].message.contains("should display home page logo"));
        assert_eq!(tasks_to_add.len(), 1);
        assert!(tasks_to_add[0].contains("gantry:id:failed-test-should-display-home-page-logo"));
    }

    #[test]
    fn test_parse_jest_json_failures() {
        let content = r#"{
            "testResults": [
                {
                    "assertionResults": [
                        {
                            "status": "failed",
                            "fullName": "User authentication flow works with valid credentials"
                        }
                    ]
                }
            ]
        }"#;
        let json: JsonValue = serde_json::from_str(content).unwrap();
        let mut issues = Vec::new();
        let mut tasks_to_add = Vec::new();
        parse_jest_json(
            &json,
            "jest-report.json",
            &mut issues,
            &mut tasks_to_add,
            "",
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "FAILING_TEST");
        assert!(
            issues[0]
                .message
                .contains("User authentication flow works with valid credentials")
        );
        assert_eq!(tasks_to_add.len(), 1);
        assert!(
            tasks_to_add[0]
                .contains("failed-test-User-authentication-flow-works-with-valid-credentials")
        );
    }

    #[test]
    fn test_parse_flutter_jsonl_failures() {
        let content = "{\"event\": \"testStart\", \"test\": {\"id\": 42, \"name\": \"Sign-in flow screen elements layout\"}}\n{\"event\": \"testDone\", \"testID\": 42, \"result\": \"failure\"}";
        let mut issues = Vec::new();
        let mut tasks_to_add = Vec::new();
        parse_flutter_jsonl(
            content,
            "flutter_report.jsonl",
            &mut issues,
            &mut tasks_to_add,
            "",
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "FAILING_TEST");
        assert!(
            issues[0]
                .message
                .contains("Sign-in flow screen elements layout")
        );
        assert_eq!(tasks_to_add.len(), 1);
        assert!(tasks_to_add[0].contains("failed-test-Sign-in-flow-screen-elements-layout"));
    }

    fn test_cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct TestVaultGuard {
        _cwd_lock: std::sync::MutexGuard<'static, ()>,
        old_cwd: PathBuf,
        temp_dir: PathBuf,
    }

    impl TestVaultGuard {
        fn new(name: &str) -> Self {
            let cwd_lock = test_cwd_lock().lock().expect("test cwd lock poisoned");
            let old_cwd = std::env::current_dir().unwrap();
            let mut temp_dir = std::env::temp_dir();
            temp_dir.push(format!(
                "gantry_test_{}_{}",
                name,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&temp_dir).unwrap();

            let gantry_dir = temp_dir.join(".gantry");
            std::fs::create_dir_all(&gantry_dir).unwrap();

            std::env::set_current_dir(&temp_dir).unwrap();
            Self {
                _cwd_lock: cwd_lock,
                old_cwd,
                temp_dir,
            }
        }

        fn write_file(&self, relative_path: &str, content: &str) {
            let path = self.temp_dir.join(relative_path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, content).unwrap();
        }
    }

    impl Drop for TestVaultGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.old_cwd);
            let _ = std::fs::remove_dir_all(&self.temp_dir);
        }
    }

    fn write_base_documents(guard: &TestVaultGuard) {
        guard.write_file(".gantry/README-launch.md", "# README Launch");
        guard.write_file(".gantry/tasks.md", "# Tasks\n- [ ] Fix blockers");
        guard.write_file(".gantry/AGENTS.md", "# Agents\nOwner: @pm_dan");
        guard.write_file(
            ".gantry/go_no_go.md",
            "---\ntype: go_no_go\nrelease: 1.0.0\ntarget_date: 2026-06-15\nrequired_crash_free_rate: 99.5\nrequired_qa_completion: 100\nstatus: at_risk\n---\n# Gates\n- [ ] **Sign-off:** Approved. <!-- @pm_dan -->"
        );
        guard.write_file(".gantry/release-notes.md", "# Release Notes");
        guard.write_file(".gantry/engineering-notes.md", "# Engineering Notes");
        guard.write_file(".gantry/runbook.md", "# Runbook");
        guard.write_file(".gantry/postlaunch.md", "# Post Launch");
    }

    #[test]
    fn test_legacy_config_fallback() {
        let guard = TestVaultGuard::new("legacy_fallback");
        write_base_documents(&guard);

        guard.write_file(
            ".gantry/config.json",
            r#"{
                "vaultName": "Legacy Vault",
                "version": "1.0.0",
                "releaseDate": "2026-06-15",
                "sentry": {
                    "organization": "my-org",
                    "project": "my-proj",
                    "targetCrashFreeRate": 99.7
                }
            }"#,
        );

        let res = compile_launch_index_for_environment(None);
        assert!(res.is_ok());
        let payload = res.unwrap();
        assert_eq!(payload.active_environment, "production");
        assert!(payload.environment_results.contains_key("production"));
        let prod_res = payload.environment_results.get("production").unwrap();
        assert_eq!(prod_res.profile.min_crash_free_rate, Some(99.7));
        assert_eq!(prod_res.profile.required_qa_completion, Some(100));
        assert!(prod_res.profile.require_signoffs);
    }

    #[test]
    fn test_environment_qa_thresholds() {
        let guard = TestVaultGuard::new("qa_thresholds");
        write_base_documents(&guard);

        guard.write_file(
            ".gantry/config.json",
            r#"{
                "vaultName": "Multi-Env Vault",
                "version": "1.0.0",
                "releaseDate": "2026-06-15",
                "environments": {
                    "staging": {
                        "requiredQaCompletion": 80,
                        "blockOnP0": true,
                        "minCrashFreeRate": null,
                        "requireSignoffs": false,
                        "maxObservedAgeHours": 24,
                        "blockOnMissingTelemetry": false
                    },
                    "production": {
                        "requiredQaCompletion": 100,
                        "blockOnP0": true,
                        "minCrashFreeRate": 99.9,
                        "requireSignoffs": true,
                        "maxObservedAgeHours": 24,
                        "blockOnMissingTelemetry": true
                    }
                }
            }"#,
        );

        guard.write_file(
            ".gantry/qa/regression.md",
            r#"---
type: qa
id: regression
title: Regression Suite
owner: "@qa_bob"
total_tests: 10
completed_tests: 8
last_verified: "2026-05-28"
---
# Test details
- [x] Test 1
- [x] Test 2
- [x] Test 3
- [x] Test 4
- [x] Test 5
- [x] Test 6
- [x] Test 7
- [x] Test 8
- [ ] Test 9
- [ ] Test 10"#,
        );

        // Staging: required is 80%, completed is 8/10 (80%). Staging should not block on QA.
        let res_staging = compile_launch_index_for_environment(Some("staging"));
        assert!(res_staging.is_ok());
        let payload_staging = res_staging.unwrap();
        assert_eq!(payload_staging.active_environment, "staging");
        let staging_res = payload_staging.environment_results.get("staging").unwrap();
        // Staging status is Go because requireSignoffs is false (signoffs bypassed) and QA is 80%.
        assert_eq!(staging_res.readiness, "Go");

        // Production: required is 100%, completed is 8/10 (80%). Production should block.
        let res_prod = compile_launch_index_for_environment(Some("production"));
        assert!(res_prod.is_ok());
        let payload_prod = res_prod.unwrap();
        println!(
            "DEBUG: payload_prod.qa_total={}, qa_completed={}",
            payload_prod.qa_total, payload_prod.qa_completed
        );
        println!(
            "DEBUG: payload_prod.validation_errors={:#?}",
            payload_prod.validation_errors
        );
        let prod_res = payload_prod.environment_results.get("production").unwrap();
        println!("DEBUG: prod_res={:#?}", prod_res);
        assert_eq!(prod_res.readiness, "No-Go");
        assert!(
            prod_res
                .readiness_reasons
                .iter()
                .any(|r| r.contains("QA completion"))
        );
    }

    #[test]
    fn test_require_signoffs_bypass() {
        let guard = TestVaultGuard::new("signoffs_bypass");
        write_base_documents(&guard);

        guard.write_file(
            ".gantry/config.json",
            r#"{
                "vaultName": "Signoffs Bypass Vault",
                "version": "1.0.0",
                "releaseDate": "2026-06-15",
                "environments": {
                    "staging": {
                        "requiredQaCompletion": 0,
                        "blockOnP0": true,
                        "minCrashFreeRate": null,
                        "requireSignoffs": false,
                        "maxObservedAgeHours": 24,
                        "blockOnMissingTelemetry": false
                    }
                }
            }"#,
        );

        let res = compile_launch_index_for_environment(Some("staging"));
        assert!(res.is_ok());
        let payload = res.unwrap();
        let staging_res = payload.environment_results.get("staging").unwrap();
        assert_eq!(staging_res.readiness, "Go");
        assert!(
            staging_res
                .readiness_reasons
                .iter()
                .any(|r| r.contains("bypassed by") && r.contains("environment policy"))
        );
    }

    #[test]
    fn test_telemetry_gating_under_threshold() {
        let guard = TestVaultGuard::new("telemetry_gating");
        write_base_documents(&guard);
        let fresh_fetched_at = chrono::Utc::now().to_rfc3339();

        guard.write_file(
            ".gantry/config.json",
            r#"{
                "vaultName": "Telemetry Vault",
                "version": "1.0.0",
                "releaseDate": "2026-06-15",
                "environments": {
                    "production": {
                        "requiredQaCompletion": 0,
                        "blockOnP0": true,
                        "minCrashFreeRate": 99.9,
                        "requireSignoffs": false,
                        "maxObservedAgeHours": 24,
                        "blockOnMissingTelemetry": true
                    }
                }
            }"#,
        );

        let observed = serde_json::json!({
            "observations": {
                "production": {
                    "source": "sentry",
                    "environment": "production",
                    "release": "1.0.0",
                    "crash_free_rate": 99.8,
                    "fetched_at": fresh_fetched_at
                }
            }
        });
        guard.write_file(".gantry/observed.json", &observed.to_string());

        let res = compile_launch_index_for_environment(Some("production"));
        assert!(res.is_ok());
        let payload = res.unwrap();
        let prod_res = payload.environment_results.get("production").unwrap();
        assert_eq!(prod_res.readiness, "No-Go");
        assert_eq!(*prod_res.category_scores.get("Monitoring").unwrap(), 0);
        assert!(
            prod_res
                .score_reasons
                .get("Monitoring")
                .unwrap()
                .iter()
                .any(|r| r.contains("below threshold"))
        );
    }

    #[test]
    fn test_telemetry_missing_and_stale() {
        let guard = TestVaultGuard::new("telemetry_missing_stale");
        write_base_documents(&guard);

        // 1. Missing telemetry but blockOnMissingTelemetry is true -> No-Go
        guard.write_file(
            ".gantry/config.json",
            r#"{
                "vaultName": "Telemetry Vault",
                "version": "1.0.0",
                "releaseDate": "2026-06-15",
                "environments": {
                    "production": {
                        "requiredQaCompletion": 0,
                        "blockOnP0": true,
                        "minCrashFreeRate": 99.9,
                        "requireSignoffs": false,
                        "maxObservedAgeHours": 24,
                        "blockOnMissingTelemetry": true
                    }
                }
            }"#,
        );

        let res1 = compile_launch_index_for_environment(Some("production"));
        let prod_res1 = res1
            .unwrap()
            .environment_results
            .get("production")
            .unwrap()
            .clone();
        assert_eq!(prod_res1.readiness, "No-Go");
        assert_eq!(*prod_res1.category_scores.get("Monitoring").unwrap(), 0);

        // 2. Missing telemetry but blockOnMissingTelemetry is false -> At Risk
        guard.write_file(
            ".gantry/config.json",
            r#"{
                "vaultName": "Telemetry Vault",
                "version": "1.0.0",
                "releaseDate": "2026-06-15",
                "environments": {
                    "production": {
                        "requiredQaCompletion": 0,
                        "blockOnP0": true,
                        "minCrashFreeRate": 99.9,
                        "requireSignoffs": false,
                        "maxObservedAgeHours": 24,
                        "blockOnMissingTelemetry": false
                    }
                }
            }"#,
        );

        let res2 = compile_launch_index_for_environment(Some("production"));
        let prod_res2 = res2
            .unwrap()
            .environment_results
            .get("production")
            .unwrap()
            .clone();
        assert_eq!(prod_res2.readiness, "At Risk");
        assert_eq!(*prod_res2.category_scores.get("Monitoring").unwrap(), 50);

        // 3. Stale telemetry -> At Risk
        let now = chrono::Utc::now();
        let stale_fetched_at = (now - chrono::Duration::hours(25)).to_rfc3339();

        guard.write_file(
            ".gantry/observed.json",
            &format!(
                r#"{{
                    "observations": {{
                        "production": {{
                            "source": "sentry",
                            "environment": "production",
                            "release": "1.0.0",
                            "crash_free_rate": 99.95,
                            "fetched_at": "{}"
                        }}
                    }}
                }}"#,
                stale_fetched_at
            ),
        );

        let res3 = compile_launch_index_for_environment(Some("production"));
        let prod_res3 = res3
            .unwrap()
            .environment_results
            .get("production")
            .unwrap()
            .clone();
        assert_eq!(prod_res3.readiness, "At Risk");
        assert_eq!(*prod_res3.category_scores.get("Monitoring").unwrap(), 50);
        assert!(
            prod_res3
                .score_reasons
                .get("Monitoring")
                .unwrap()
                .iter()
                .any(|r| r.contains("stale"))
        );
    }

    #[test]
    fn test_shared_facts_snapshot() {
        let guard = TestVaultGuard::new("shared_facts");
        write_base_documents(&guard);

        guard.write_file(
            ".gantry/config.json",
            r#"{
                "vaultName": "Shared Facts Vault",
                "version": "1.0.0",
                "releaseDate": "2026-06-15",
                "environments": {
                    "staging": {
                        "requiredQaCompletion": 0,
                        "blockOnP0": true,
                        "requireSignoffs": false
                    },
                    "production": {
                        "requiredQaCompletion": 100,
                        "blockOnP0": true,
                        "requireSignoffs": true
                    }
                }
            }"#,
        );

        let res = compile_launch_index_for_environment(None);
        assert!(res.is_ok());
        let payload = res.unwrap();
        assert!(payload.environment_results.contains_key("staging"));
        assert!(payload.environment_results.contains_key("production"));
        // Facts check
        assert_eq!(payload.vault_name, "Shared Facts Vault");
        assert_eq!(payload.release, "1.0.0");
        assert_eq!(
            payload
                .validation_errors
                .iter()
                .filter(|e| e.code != "SCHEMA_MISSING")
                .count(),
            0
        );
    }

    #[test]
    fn test_unknown_explicit_env() {
        let guard = TestVaultGuard::new("unknown_env");
        write_base_documents(&guard);

        guard.write_file(
            ".gantry/config.json",
            r#"{
                "vaultName": "Explicit Env Vault",
                "version": "1.0.0",
                "releaseDate": "2026-06-15",
                "environments": {
                    "production": {
                        "requiredQaCompletion": 100
                    }
                }
            }"#,
        );

        let res = compile_launch_index_for_environment(Some("definitely-not-real"));
        assert!(res.is_err());
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("Unknown environment"));
    }

    #[test]
    fn test_staging_missing_telemetry_at_risk() {
        let guard = TestVaultGuard::new("staging_missing_telemetry");
        write_base_documents(&guard);

        // Staging has minCrashFreeRate set, but blockOnMissingTelemetry is omitted (default should be false)
        guard.write_file(
            ".gantry/config.json",
            r#"{
                "vaultName": "Staging Defaults Vault",
                "version": "1.0.0",
                "releaseDate": "2026-06-15",
                "environments": {
                    "staging": {
                        "requiredQaCompletion": 0,
                        "requireSignoffs": false,
                        "minCrashFreeRate": 99.9
                    }
                }
            }"#,
        );

        let res = compile_launch_index_for_environment(Some("staging"));
        assert!(res.is_ok());
        let payload = res.unwrap();
        let staging_res = payload.environment_results.get("staging").unwrap();

        // Staging missing telemetry should trigger "At Risk" (not No-Go) since blockOnMissingTelemetry is false by default on staging
        assert_eq!(staging_res.readiness, "At Risk");
        assert_eq!(*staging_res.category_scores.get("Monitoring").unwrap(), 50);
    }

    #[test]
    fn test_snapshot_diffing() {
        use std::collections::HashMap;
        let mut base_envs = HashMap::new();
        base_envs.insert(
            "production".to_string(),
            EnvSnapshotSummary {
                readiness: "At Risk".to_string(),
                readiness_score: 80,
                telemetry_status: "fresh".to_string(),
                readiness_reasons: vec![],
            },
        );

        let base_snap = GantrySnapshot {
            release: "1.0.0".to_string(),
            active_environment: "production".to_string(),
            readiness: "At Risk".to_string(),
            readiness_score: 80,
            environments: base_envs.clone(),
            open_blockers: 1,
            qa_total: 10,
            qa_completed: 5,
            blockers: vec![BlockerEntry {
                path: "blockers/BLK-1.md".to_string(),
                id: "BLK-1".to_string(),
                title: "Blocker 1".to_string(),
                severity: "P0".to_string(),
                status: "open".to_string(),
                owner: "@pm_dan".to_string(),
                source_files: vec![],
                stale: false,
                last_verified: "2026-06-01".to_string(),
                waived: false,
            }],
            qa_suites: vec![QAEntry {
                path: "qa/Q-1.md".to_string(),
                id: "Q-1".to_string(),
                title: "QA 1".to_string(),
                owner: "@pm_dan".to_string(),
                total_tests: 10,
                completed_tests: 5,
                stale: false,
                last_verified: "2026-06-01".to_string(),
                source_files: vec![],
                waived: false,
            }],
            gates: vec![GoNoGoGate {
                name: "Gate 1".to_string(),
                status: "unchecked".to_string(),
                owner: "@pm_dan".to_string(),
                line_index: 0,
                waived: false,
            }],
            signoffs: vec![Signoff {
                name: "Dan".to_string(),
                role: "Product".to_string(),
                status: "Pending".to_string(),
                date: "".to_string(),
                waived: false,
            }],
            timestamp: "2026-06-01T12-00-00Z".to_string(),
            commit: "abc111".to_string(),
            waiver_count: 0,
            evidence_count: 0,
            scorecard_summary: None,
            release_manifest_path: None,
            prev_hash: String::new(),
            entry_hash: String::new(),
        };

        let mut head_envs = HashMap::new();
        head_envs.insert(
            "production".to_string(),
            EnvSnapshotSummary {
                readiness: "Go".to_string(),
                readiness_score: 100,
                telemetry_status: "fresh".to_string(),
                readiness_reasons: vec![],
            },
        );

        let head_snap = GantrySnapshot {
            release: "1.0.0".to_string(),
            active_environment: "production".to_string(),
            readiness: "Go".to_string(),
            readiness_score: 100,
            environments: head_envs,
            open_blockers: 0,
            qa_total: 10,
            qa_completed: 10,
            blockers: vec![BlockerEntry {
                path: "blockers/BLK-1.md".to_string(),
                id: "BLK-1".to_string(),
                title: "Blocker 1".to_string(),
                severity: "P0".to_string(),
                status: "resolved".to_string(),
                owner: "@pm_dan".to_string(),
                source_files: vec![],
                stale: false,
                last_verified: "2026-06-01".to_string(),
                waived: false,
            }],
            qa_suites: vec![QAEntry {
                path: "qa/Q-1.md".to_string(),
                id: "Q-1".to_string(),
                title: "QA 1".to_string(),
                owner: "@pm_dan".to_string(),
                total_tests: 10,
                completed_tests: 10,
                stale: false,
                last_verified: "2026-06-01".to_string(),
                source_files: vec![],
                waived: false,
            }],
            gates: vec![GoNoGoGate {
                name: "Gate 1".to_string(),
                status: "checked".to_string(),
                owner: "@pm_dan".to_string(),
                line_index: 0,
                waived: false,
            }],
            signoffs: vec![Signoff {
                name: "Dan".to_string(),
                role: "Product".to_string(),
                status: "Approved".to_string(),
                date: "2026-06-01".to_string(),
                waived: false,
            }],
            timestamp: "2026-06-01T13-00-00Z".to_string(),
            commit: "abc222".to_string(),
            waiver_count: 0,
            evidence_count: 0,
            scorecard_summary: None,
            release_manifest_path: None,
            prev_hash: String::new(),
            entry_hash: String::new(),
        };

        let diff = diff_snapshots(&base_snap, &head_snap);
        assert_eq!(diff.base_commit, "abc111");
        assert_eq!(diff.head_commit, "abc222");
        assert_eq!(
            diff.readiness_changed,
            Some(("At Risk".to_string(), "Go".to_string()))
        );
        assert_eq!(diff.score_changed, Some((80, 100)));
        assert_eq!(diff.resolved_blockers, vec!["BLK-1".to_string()]);
        assert_eq!(diff.completed_qa_suites, vec!["Q-1".to_string()]);
        assert_eq!(diff.completed_gates, vec!["Gate 1".to_string()]);
        assert_eq!(diff.completed_signoffs, vec!["Product".to_string()]);
    }

    #[test]
    fn test_slack_digest_payload_generation() {
        use std::collections::HashMap;
        let base_snap = GantrySnapshot {
            release: "1.0.0".to_string(),
            active_environment: "production".to_string(),
            readiness: "At Risk".to_string(),
            readiness_score: 80,
            environments: HashMap::new(),
            open_blockers: 1,
            qa_total: 10,
            qa_completed: 5,
            blockers: vec![BlockerEntry {
                path: "blockers/BLK-1.md".to_string(),
                id: "BLK-1".to_string(),
                title: "Blocker 1".to_string(),
                severity: "P0".to_string(),
                status: "open".to_string(),
                owner: "@pm_dan".to_string(),
                source_files: vec![],
                stale: false,
                last_verified: "2026-06-01".to_string(),
                waived: false,
            }],
            qa_suites: vec![],
            gates: vec![],
            signoffs: vec![],
            timestamp: "2026-06-01T12-00-00Z".to_string(),
            commit: "abc111".to_string(),
            waiver_count: 0,
            evidence_count: 0,
            scorecard_summary: None,
            release_manifest_path: None,
            prev_hash: String::new(),
            entry_hash: String::new(),
        };

        let head_snap = GantrySnapshot {
            release: "1.0.0".to_string(),
            active_environment: "production".to_string(),
            readiness: "Go".to_string(),
            readiness_score: 100,
            environments: HashMap::new(),
            open_blockers: 0,
            qa_total: 10,
            qa_completed: 10,
            blockers: vec![BlockerEntry {
                path: "blockers/BLK-1.md".to_string(),
                id: "BLK-1".to_string(),
                title: "Blocker 1".to_string(),
                severity: "P0".to_string(),
                status: "resolved".to_string(),
                owner: "@pm_dan".to_string(),
                source_files: vec![],
                stale: false,
                last_verified: "2026-06-01".to_string(),
                waived: false,
            }],
            qa_suites: vec![],
            gates: vec![],
            signoffs: vec![],
            timestamp: "2026-06-01T13-00-00Z".to_string(),
            commit: "abc222".to_string(),
            waiver_count: 0,
            evidence_count: 0,
            scorecard_summary: None,
            release_manifest_path: None,
            prev_hash: String::new(),
            entry_hash: String::new(),
        };

        let diff = diff_snapshots(&base_snap, &head_snap);
        let payload = compile_slack_digest(&diff, &base_snap, &head_snap);

        let blocks = payload.get("blocks").unwrap().as_array().unwrap();
        assert!(blocks.len() >= 4);

        // Check header block
        assert_eq!(blocks[0].get("type").unwrap().as_str(), Some("header"));

        // Verify no secret leakage or missing fields
        let block_json = serde_json::to_string(&payload).unwrap();
        assert!(block_json.contains("Resolved Blocker:"));
        assert!(block_json.contains("abc111"));
        assert!(block_json.contains("abc222"));
    }

    #[test]
    fn test_telemetry_rules_evaluation() {
        use std::collections::HashMap;
        let rules = vec![
            TelemetryRule {
                metric: "datadog.p99_latency_ms".to_string(),
                comparator: "<=".to_string(),
                threshold: 250.0,
                severity: "required".to_string(),
            },
            TelemetryRule {
                metric: "custom.error_rate".to_string(),
                comparator: "<".to_string(),
                threshold: 1.0,
                severity: "warning".to_string(),
            },
        ];

        let mut metrics = HashMap::new();
        metrics.insert("datadog.p99_latency_ms".to_string(), 210.0);
        metrics.insert("custom.error_rate".to_string(), 1.5); // Should trigger warning failure

        let telemetry = ObservedTelemetry {
            source: "datadog".to_string(),
            environment: "production".to_string(),
            release: "1.0.0".to_string(),
            crash_free_rate: None,
            metrics,
            fetched_at: "2026-06-01T12:00:00Z".to_string(),
        };

        let mut readiness_reasons = Vec::new();
        let mut mon_reasons = Vec::new();
        let mut monitoring_score = 100;
        let mut readiness = "Go".to_string();

        evaluate_telemetry_rules(
            &rules,
            &telemetry,
            &mut readiness_reasons,
            &mut mon_reasons,
            &mut monitoring_score,
            &mut readiness,
        );

        // Warning severity should set readiness to "At Risk" and deduct 50 points
        assert_eq!(readiness, "At Risk");
        assert_eq!(monitoring_score, 50);
        assert!(mon_reasons.iter().any(|r| r.contains("custom.error_rate")));

        // Now test required rule failure
        let mut metrics = HashMap::new();
        metrics.insert("datadog.p99_latency_ms".to_string(), 300.0); // Fails required rule
        metrics.insert("custom.error_rate".to_string(), 0.1); // Passes warning

        let telemetry_fail_req = ObservedTelemetry {
            source: "datadog".to_string(),
            environment: "production".to_string(),
            release: "1.0.0".to_string(),
            crash_free_rate: None,
            metrics,
            fetched_at: "2026-06-01T12:00:00Z".to_string(),
        };

        let mut readiness_reasons = Vec::new();
        let mut mon_reasons = Vec::new();
        let mut monitoring_score = 100;
        let mut readiness = "Go".to_string();

        evaluate_telemetry_rules(
            &rules,
            &telemetry_fail_req,
            &mut readiness_reasons,
            &mut mon_reasons,
            &mut monitoring_score,
            &mut readiness,
        );

        assert_eq!(readiness, "No-Go");
        assert_eq!(monitoring_score, 0);
    }

    #[test]
    fn test_compute_impact_summary() {
        use crate::graph::compute_impact_summary;
        use crate::{BlockerEntry, QAEntry};

        let changed_files = vec!["src/checkout.js".to_string(), "src/auth.js".to_string()];

        let blockers = vec![BlockerEntry {
            path: "blockers/BLK-101.md".to_string(),
            id: "BLK-101".to_string(),
            title: "Fix checkout crash".to_string(),
            severity: "P0".to_string(),
            status: "open".to_string(),
            owner: "@bob".to_string(),
            source_files: vec!["src/checkout.js".to_string()],
            stale: false,
            last_verified: "2026-06-01".to_string(),
            waived: false,
        }];

        let qa_suites = vec![QAEntry {
            path: "qa/regression.md".to_string(),
            id: "QA-PLAN".to_string(),
            title: "Regression suite".to_string(),
            owner: "@alice".to_string(),
            total_tests: 10,
            completed_tests: 5,
            stale: false,
            last_verified: "2026-06-01".to_string(),
            source_files: vec!["src/checkout.js".to_string()],
            waived: false,
        }];

        let summary = compute_impact_summary(changed_files, &blockers, &qa_suites);
        assert_eq!(summary.impacted_blockers, vec!["BLK-101".to_string()]);
        assert_eq!(summary.impacted_qa, vec!["QA-PLAN".to_string()]);
        assert_eq!(summary.changed_files.len(), 2);
    }

    #[test]
    fn test_waiver_matching_and_suppression() {
        use crate::{Waiver, find_matching_waiver};

        let waivers = vec![
            Waiver {
                id: "WAV-001".to_string(),
                target_type: "blocker".to_string(),
                target_id: Some("BLK-101".to_string()),
                target_path: Some("blockers/BLK-101.md".to_string()),
                target_code: None,
                reason: "Test waiver".to_string(),
                approved_by: "@lead_dev".to_string(),
                created_at: "2026-06-01".to_string(),
                expires_at: "2026-06-15".to_string(),
                status: "active".to_string(),
            },
            Waiver {
                id: "WAV-002".to_string(),
                target_type: "validation".to_string(),
                target_id: None,
                target_path: Some("scorecard.json".to_string()),
                target_code: Some("STALE_DOC".to_string()),
                reason: "Test validation waiver".to_string(),
                approved_by: "@lead_dev".to_string(),
                created_at: "2026-06-01".to_string(),
                expires_at: "2026-06-15".to_string(),
                status: "active".to_string(),
            },
            Waiver {
                id: "WAV-003".to_string(),
                target_type: "gate".to_string(),
                target_id: Some("Stakeholder sign-off".to_string()),
                target_path: Some("go_no_go.md".to_string()),
                target_code: None,
                reason: "Test gate waiver".to_string(),
                approved_by: "@lead_dev".to_string(),
                created_at: "2026-06-01".to_string(),
                expires_at: "2026-06-15".to_string(),
                status: "active".to_string(),
            },
        ];

        // 1. Matches blocker
        let res1 = find_matching_waiver(
            "blocker",
            Some("BLK-101"),
            Some("blockers/BLK-101.md"),
            None,
            &waivers,
        );
        assert!(res1.is_some());
        assert_eq!(res1.unwrap().id, "WAV-001");

        // 2. Matches validation error
        let res2 = find_matching_waiver(
            "validation",
            None,
            Some("scorecard.json"),
            Some("STALE_DOC"),
            &waivers,
        );
        assert!(res2.is_some());
        assert_eq!(res2.unwrap().id, "WAV-002");

        // 3. Matches gate
        let res3 = find_matching_waiver(
            "gate",
            Some("Stakeholder sign-off"),
            Some("go_no_go.md"),
            None,
            &waivers,
        );
        assert!(res3.is_some());
        assert_eq!(res3.unwrap().id, "WAV-003");
    }

    #[test]
    fn test_expired_waivers_evaluation() {
        let date_today = "2026-06-20";
        let w = crate::Waiver {
            id: "WAV-001".to_string(),
            target_type: "blocker".to_string(),
            target_id: Some("BLK-101".to_string()),
            target_path: Some("blockers/BLK-101.md".to_string()),
            target_code: None,
            reason: "Test waiver".to_string(),
            approved_by: "@lead_dev".to_string(),
            created_at: "2026-06-01".to_string(),
            expires_at: "2026-06-15".to_string(),
            status: "active".to_string(),
        };
        let is_expired = w.expires_at.as_str() < date_today;
        assert!(is_expired);
    }

    // =========================================================================
    // Phase 1 edge-case tests: snapshot.rs
    // =========================================================================

    #[test]
    fn test_compile_trends_empty_snapshots_dir() {
        let guard = TestVaultGuard::new("trends_empty");
        write_base_documents(&guard);
        guard.write_file(
            ".gantry/config.json",
            r#"{"vaultName":"Trends Vault","version":"1.0.0","releaseDate":"2026-06-15"}"#,
        );
        // No snapshots/ directory at all
        let result = compile_trends();
        assert!(result.is_ok());
        let trends = result.unwrap();
        assert!(trends.dates.is_empty());
        assert!(trends.scores.is_empty());
    }

    #[test]
    fn test_compile_trends_multiple_snapshots() {
        let guard = TestVaultGuard::new("trends_multi");
        write_base_documents(&guard);
        guard.write_file(
            ".gantry/config.json",
            r#"{"vaultName":"Trends Vault","version":"1.0.0","releaseDate":"2026-06-15"}"#,
        );

        // Write two snapshots with different timestamps — verify ordering
        let snap1 = GantrySnapshot {
            release: "1.0.0".to_string(),
            active_environment: "production".to_string(),
            readiness: "At Risk".to_string(),
            readiness_score: 60,
            environments: HashMap::new(),
            open_blockers: 2,
            qa_total: 10,
            qa_completed: 4,
            blockers: vec![],
            qa_suites: vec![],
            gates: vec![],
            signoffs: vec![],
            timestamp: "2026-06-01T10-00-00Z".to_string(),
            commit: "aaa111".to_string(),
            waiver_count: 1,
            evidence_count: 0,
            scorecard_summary: None,
            release_manifest_path: None,
            prev_hash: String::new(),
            entry_hash: String::new(),
        };
        let snap2 = GantrySnapshot {
            release: "1.0.0".to_string(),
            active_environment: "production".to_string(),
            readiness: "Go".to_string(),
            readiness_score: 95,
            environments: HashMap::new(),
            open_blockers: 0,
            qa_total: 10,
            qa_completed: 10,
            blockers: vec![],
            qa_suites: vec![],
            gates: vec![],
            signoffs: vec![],
            timestamp: "2026-06-02T10-00-00Z".to_string(),
            commit: "bbb222".to_string(),
            waiver_count: 0,
            evidence_count: 3,
            scorecard_summary: None,
            release_manifest_path: None,
            prev_hash: String::new(),
            entry_hash: String::new(),
        };

        guard.write_file(
            ".gantry/snapshots/2026-06-01T10-00-00Z-aaa111.json",
            &serde_json::to_string_pretty(&snap1).unwrap(),
        );
        guard.write_file(
            ".gantry/snapshots/2026-06-02T10-00-00Z-bbb222.json",
            &serde_json::to_string_pretty(&snap2).unwrap(),
        );

        let result = compile_trends();
        assert!(result.is_ok());
        let trends = result.unwrap();
        assert_eq!(trends.dates.len(), 2);
        // Ascending order: first is older
        assert_eq!(trends.scores, vec![60, 95]);
        assert_eq!(
            trends.readiness_states,
            vec!["At Risk".to_string(), "Go".to_string()]
        );
        assert_eq!(trends.blockers, vec![2, 0]);
        assert_eq!(trends.waivers, vec![1, 0]);
        assert_eq!(trends.evidence, vec![0, 3]);
    }

    #[test]
    fn test_load_snapshot_by_commit_prefix_matching() {
        let guard = TestVaultGuard::new("snap_prefix");
        write_base_documents(&guard);
        guard.write_file(
            ".gantry/config.json",
            r#"{"vaultName":"Snap Vault","version":"1.0.0","releaseDate":"2026-06-15"}"#,
        );

        let snap = GantrySnapshot {
            release: "1.0.0".to_string(),
            active_environment: "production".to_string(),
            readiness: "Go".to_string(),
            readiness_score: 100,
            environments: HashMap::new(),
            open_blockers: 0,
            qa_total: 5,
            qa_completed: 5,
            blockers: vec![],
            qa_suites: vec![],
            gates: vec![],
            signoffs: vec![],
            timestamp: "2026-06-01T12-00-00Z".to_string(),
            commit: "abcdef1234567890".to_string(),
            waiver_count: 0,
            evidence_count: 0,
            scorecard_summary: None,
            release_manifest_path: None,
            prev_hash: String::new(),
            entry_hash: String::new(),
        };

        guard.write_file(
            ".gantry/snapshots/2026-06-01T12-00-00Z-abcdef1234567890.json",
            &serde_json::to_string_pretty(&snap).unwrap(),
        );

        // Full commit match
        let full = load_snapshot_by_commit("abcdef1234567890");
        assert!(full.is_ok());
        assert_eq!(full.unwrap().readiness_score, 100);

        // Prefix match
        let prefix = load_snapshot_by_commit("abcdef");
        assert!(prefix.is_ok());
        assert_eq!(prefix.unwrap().commit, "abcdef1234567890");

        // Non-existent ref should fail
        let missing = load_snapshot_by_commit("zzz999");
        assert!(missing.is_err());
    }

    // =========================================================================
    // Phase 1 edge-case tests: notify.rs
    // =========================================================================

    #[test]
    fn test_slack_digest_event_filtering() {
        let base_snap = GantrySnapshot {
            release: "1.0.0".to_string(),
            active_environment: "production".to_string(),
            readiness: "At Risk".to_string(),
            readiness_score: 70,
            environments: HashMap::new(),
            open_blockers: 1,
            qa_total: 5,
            qa_completed: 3,
            blockers: vec![BlockerEntry {
                path: "blockers/BLK-1.md".to_string(),
                id: "BLK-1".to_string(),
                title: "Crash on launch".to_string(),
                severity: "P0".to_string(),
                status: "open".to_string(),
                owner: "@dev".to_string(),
                source_files: vec![],
                stale: false,
                last_verified: "2026-06-01".to_string(),
                waived: false,
            }],
            qa_suites: vec![QAEntry {
                path: "qa/Q-1.md".to_string(),
                id: "Q-1".to_string(),
                title: "Smoke tests".to_string(),
                owner: "@qa".to_string(),
                total_tests: 5,
                completed_tests: 3,
                stale: false,
                last_verified: "2026-06-01".to_string(),
                source_files: vec![],
                waived: false,
            }],
            gates: vec![],
            signoffs: vec![],
            timestamp: "2026-06-01T12-00-00Z".to_string(),
            commit: "aaa111".to_string(),
            waiver_count: 0,
            evidence_count: 0,
            scorecard_summary: None,
            release_manifest_path: None,
            prev_hash: String::new(),
            entry_hash: String::new(),
        };

        let head_snap = GantrySnapshot {
            release: "1.0.0".to_string(),
            active_environment: "production".to_string(),
            readiness: "Go".to_string(),
            readiness_score: 100,
            environments: HashMap::new(),
            open_blockers: 0,
            qa_total: 5,
            qa_completed: 5,
            blockers: vec![BlockerEntry {
                path: "blockers/BLK-1.md".to_string(),
                id: "BLK-1".to_string(),
                title: "Crash on launch".to_string(),
                severity: "P0".to_string(),
                status: "resolved".to_string(),
                owner: "@dev".to_string(),
                source_files: vec![],
                stale: false,
                last_verified: "2026-06-01".to_string(),
                waived: false,
            }],
            qa_suites: vec![QAEntry {
                path: "qa/Q-1.md".to_string(),
                id: "Q-1".to_string(),
                title: "Smoke tests".to_string(),
                owner: "@qa".to_string(),
                total_tests: 5,
                completed_tests: 5,
                stale: false,
                last_verified: "2026-06-01".to_string(),
                source_files: vec![],
                waived: false,
            }],
            gates: vec![],
            signoffs: vec![],
            timestamp: "2026-06-01T13-00-00Z".to_string(),
            commit: "bbb222".to_string(),
            waiver_count: 0,
            evidence_count: 0,
            scorecard_summary: None,
            release_manifest_path: None,
            prev_hash: String::new(),
            entry_hash: String::new(),
        };

        let diff = diff_snapshots(&base_snap, &head_snap);

        // Filter to only blockerResolved events — should exclude QA completion
        let payload_filtered = compile_slack_digest_for_events(
            &diff,
            &base_snap,
            &head_snap,
            &["blockerResolved".to_string()],
        );
        let json_str = serde_json::to_string(&payload_filtered).unwrap();
        assert!(json_str.contains("Resolved Blocker:"));
        assert!(!json_str.contains("QA Suite Completed:"));

        // Filter to only qaSuiteComplete events — should exclude blocker
        let payload_qa_only = compile_slack_digest_for_events(
            &diff,
            &base_snap,
            &head_snap,
            &["qaSuiteComplete".to_string()],
        );
        let json_qa = serde_json::to_string(&payload_qa_only).unwrap();
        assert!(!json_qa.contains("Resolved Blocker:"));
        assert!(json_qa.contains("QA Suite Completed:"));
    }

    // =========================================================================
    // Phase 1 edge-case tests: telemetry.rs
    // =========================================================================

    #[test]
    fn test_telemetry_crash_free_rate_normalization() {
        // crash_free_rate should be inserted into the metrics dict as sentry.crash_free_rate
        let rules = vec![TelemetryRule {
            metric: "sentry.crash_free_rate".to_string(),
            comparator: ">=".to_string(),
            threshold: 99.5,
            severity: "required".to_string(),
        }];

        let telemetry = ObservedTelemetry {
            source: "sentry".to_string(),
            environment: "production".to_string(),
            release: "1.0.0".to_string(),
            crash_free_rate: Some(99.8),
            metrics: HashMap::new(), // empty — but crash_free_rate should be normalized in
            fetched_at: "2026-06-01T12:00:00Z".to_string(),
        };

        let mut readiness_reasons = Vec::new();
        let mut mon_reasons = Vec::new();
        let mut monitoring_score = 100;
        let mut readiness = "Go".to_string();

        evaluate_telemetry_rules(
            &rules,
            &telemetry,
            &mut readiness_reasons,
            &mut mon_reasons,
            &mut monitoring_score,
            &mut readiness,
        );

        // 99.8 >= 99.5 → should pass
        assert_eq!(readiness, "Go");
        assert_eq!(monitoring_score, 100);
        assert!(readiness_reasons.is_empty());
    }

    #[test]
    fn test_telemetry_missing_metric_required_severity() {
        let rules = vec![TelemetryRule {
            metric: "totally.missing.metric".to_string(),
            comparator: "<=".to_string(),
            threshold: 100.0,
            severity: "required".to_string(),
        }];

        let telemetry = ObservedTelemetry {
            source: "datadog".to_string(),
            environment: "production".to_string(),
            release: "1.0.0".to_string(),
            crash_free_rate: None,
            metrics: HashMap::new(),
            fetched_at: "2026-06-01T12:00:00Z".to_string(),
        };

        let mut readiness_reasons = Vec::new();
        let mut mon_reasons = Vec::new();
        let mut monitoring_score = 100;
        let mut readiness = "Go".to_string();

        evaluate_telemetry_rules(
            &rules,
            &telemetry,
            &mut readiness_reasons,
            &mut mon_reasons,
            &mut monitoring_score,
            &mut readiness,
        );

        // Missing required metric → No-Go, score 0
        assert_eq!(readiness, "No-Go");
        assert_eq!(monitoring_score, 0);
        assert!(readiness_reasons[0].contains("totally.missing.metric"));
    }

    // =========================================================================
    // Phase 1 edge-case tests: graph.rs
    // =========================================================================

    #[test]
    fn test_compute_impact_summary_uncovered_changes() {
        use crate::graph::compute_impact_summary;

        // Two files in the same directory with no matching blocker/QA coverage → uncovered
        let changed_files = vec![
            "src/payments/checkout.js".to_string(),
            "src/payments/refund.js".to_string(),
        ];

        let summary = compute_impact_summary(changed_files, &[], &[]);
        assert!(summary.impacted_blockers.is_empty());
        assert!(summary.impacted_qa.is_empty());
        // src/payments has 2 changes and no coverage → should be uncovered
        assert!(
            summary
                .uncovered_changes
                .contains(&"src/payments".to_string())
        );
        assert!(!summary.suggested_tasks.is_empty());
    }

    #[test]
    fn test_compute_impact_summary_gantry_dir_excluded() {
        use crate::graph::compute_impact_summary;

        // Changes inside .gantry/ should not count as uncovered
        let changed_files = vec![
            ".gantry/blockers/BLK-1.md".to_string(),
            ".gantry/qa/regression.md".to_string(),
        ];

        let summary = compute_impact_summary(changed_files, &[], &[]);
        assert!(summary.uncovered_changes.is_empty());
    }

    // =========================================================================
    // Phase 1: CLI integration tests (exit-code scenarios)
    // =========================================================================

    #[test]
    fn test_cli_clean_vault_returns_go() {
        let guard = TestVaultGuard::new("cli_go");
        write_base_documents(&guard);

        // All gates checked, no blockers, QA complete
        guard.write_file(
            ".gantry/config.json",
            r#"{
                "vaultName": "Clean Vault",
                "version": "1.0.0",
                "releaseDate": "2026-06-15",
                "environments": {
                    "production": {
                        "requiredQaCompletion": 100,
                        "blockOnP0": true,
                        "requireSignoffs": false,
                        "blockOnMissingTelemetry": false
                    }
                }
            }"#,
        );
        guard.write_file(
            ".gantry/go_no_go.md",
            "---\ntype: go_no_go\nrelease: 1.0.0\ntarget_date: 2026-06-15\nstatus: go\n---\n# Gates\n- [x] **Product:** Approved. <!-- @pm_dan 2026-06-01 -->",
        );
        guard.write_file(
            ".gantry/qa/regression.md",
            "---\ntype: qa\nid: QA-REG\ntitle: Regression\nowner: \"@qa\"\nlast_verified: 2026-06-01\n---\n- [x] Test 1\n- [x] Test 2",
        );

        let res = compile_launch_index_for_environment(Some("production"));
        assert!(res.is_ok());
        let payload = res.unwrap();
        let prod = payload.environment_results.get("production").unwrap();
        assert_eq!(prod.readiness, "Go");
    }

    #[test]
    fn test_cli_p0_blocker_returns_nogo() {
        let guard = TestVaultGuard::new("cli_nogo");
        write_base_documents(&guard);

        guard.write_file(
            ".gantry/config.json",
            r#"{
                "vaultName": "Blocked Vault",
                "version": "1.0.0",
                "releaseDate": "2026-06-15",
                "environments": {
                    "production": {
                        "requiredQaCompletion": 0,
                        "blockOnP0": true,
                        "requireSignoffs": false,
                        "blockOnMissingTelemetry": false
                    }
                }
            }"#,
        );
        guard.write_file(
            ".gantry/blockers/BLK-CRASH.md",
            "---\ntype: blocker\nid: BLK-CRASH\ntitle: App crashes on startup\nseverity: P0\nstatus: blocked\nowner: \"@dev\"\nsource_files:\n  - src/main.rs\nlast_verified: 2026-06-01\n---\nDetails here.",
        );

        let res = compile_launch_index_for_environment(Some("production"));
        assert!(res.is_ok());
        let payload = res.unwrap();
        let prod = payload.environment_results.get("production").unwrap();
        assert_eq!(prod.readiness, "No-Go");
    }

    #[test]
    fn test_cli_malformed_config_returns_error() {
        let guard = TestVaultGuard::new("cli_error");
        write_base_documents(&guard);

        // Write malformed JSON as config
        guard.write_file(".gantry/config.json", "{ this is not valid json }}}");

        let res = compile_launch_index_for_environment(None);
        assert!(res.is_err());
    }

    #[test]
    fn test_waiver_evidence_scorecard_survive_indexing() {
        let guard = TestVaultGuard::new("waiver_evidence_scorecard");
        write_base_documents(&guard);

        guard.write_file(
            ".gantry/config.json",
            r#"{
                "vaultName": "Full Vault",
                "version": "1.0.0",
                "releaseDate": "2026-06-15",
                "environments": {
                    "production": {
                        "requiredQaCompletion": 0,
                        "blockOnP0": true,
                        "requireSignoffs": false,
                        "blockOnMissingTelemetry": false
                    }
                }
            }"#,
        );

        // Add a waiver
        guard.write_file(
            ".gantry/waivers.json",
            r#"[{
                "id": "WAV-TEST-001",
                "target_type": "blocker",
                "target_id": "BLK-KNOWN",
                "target_path": "blockers/BLK-KNOWN.md",
                "target_code": null,
                "reason": "Known issue, workaround deployed.",
                "approved_by": "@release_lead",
                "created_at": "2026-06-01",
                "expires_at": "2026-12-31",
                "status": "active"
            }]"#,
        );

        // Add evidence
        guard.write_file(
            ".gantry/evidence/EVI-001.json",
            r#"{
                "id": "EVI-001",
                "kind": "test_report",
                "title": "Regression run",
                "source": "ci",
                "path": null,
                "url": null,
                "artifact_sha": null,
                "commit": "abc123",
                "environment": "production",
                "release": "1.0.0",
                "created_at": "2026-06-01T10:00:00Z",
                "expires_at": null,
                "metadata": {},
                "attachments": []
            }"#,
        );

        // Add scorecard
        guard.write_file(
            ".gantry/scorecard.json",
            r#"[{
                "id": "qa-complete",
                "category": "QA",
                "title": "All QA tests pass",
                "severity": "warning",
                "weight": 40,
                "predicate": "min_qa_completion",
                "params": { "min_pct": 100 },
                "failure_message": "QA is incomplete.",
                "suggested_action": "Complete QA."
            }]"#,
        );

        let res = compile_launch_index_for_environment(Some("production"));
        assert!(res.is_ok());
        let payload = res.unwrap();

        // Waivers survived
        assert_eq!(payload.active_waivers.len(), 1);
        assert_eq!(payload.active_waivers[0].id, "WAV-TEST-001");

        // Evidence survived
        assert_eq!(payload.evidence_records.len(), 1);
        assert_eq!(payload.evidence_records[0].id, "EVI-001");

        // Scorecard survived
        assert!(!payload.scorecard_results.is_empty());
        assert_eq!(payload.scorecard_results[0].rule_id, "qa-complete");
    }

    #[test]
    fn test_vault_resolvers() {
        let guard = TestVaultGuard::new("resolvers_test");

        let canonical_temp_dir = std::fs::canonicalize(&guard.temp_dir).unwrap();

        // resolve_existing_vault (no override) resolves to CWD/.gantry (which exists)
        let resolved = resolve_existing_vault(None);
        assert!(resolved.is_ok());
        let ctx = resolved.unwrap();
        assert_eq!(
            std::fs::canonicalize(&ctx.vault_path).unwrap(),
            canonical_temp_dir.join(".gantry")
        );
        assert_eq!(
            std::fs::canonicalize(&ctx.project_root).unwrap(),
            canonical_temp_dir
        );

        // resolve_existing_vault fails on non-existent path
        let non_existent = guard.temp_dir.join("non_existent_vault");
        assert!(resolve_existing_vault(Some(&non_existent)).is_err());

        // resolve_init_target (no override) returns CWD/.gantry
        let resolved_init = resolve_init_target(None);
        assert!(resolved_init.is_ok());
        let ctx_init = resolved_init.unwrap();
        assert_eq!(
            std::fs::canonicalize(&ctx_init.vault_path).unwrap(),
            canonical_temp_dir.join(".gantry")
        );
        assert_eq!(
            std::fs::canonicalize(&ctx_init.project_root).unwrap(),
            canonical_temp_dir
        );

        // resolve_init_target (explicit path override)
        let custom_init = guard.temp_dir.join("custom_init");
        let resolved_custom = resolve_init_target(Some(&custom_init));
        assert!(resolved_custom.is_ok());
        let ctx_custom = resolved_custom.unwrap();
        assert_eq!(ctx_custom.vault_path, custom_init);
    }
}
