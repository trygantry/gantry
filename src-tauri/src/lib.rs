// Gantry desktop (Tauri) — thin command layer over gantry-core.
// All launch-readiness logic lives in the GUI-free `gantry-core` crate.
use ::notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use gantry_core::*;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::channel;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

static MOBILE_AUDIT_CANCELLATIONS: OnceLock<
    Mutex<BTreeMap<String, gantry_core::mobile::MobileAuditCancellation>>,
> = OnceLock::new();

#[derive(serde::Deserialize)]
struct InitOptions {
    template_type: String,
    version: String,
    release_date: String,
    lead_owner: String,
    qa_lead: String,
    enabled_gates: Vec<String>,
    create_qa: bool,
    force: bool,
}

#[derive(serde::Serialize)]
struct WorkspaceStatus {
    state: String,
    path: String,
    has_vault: bool,
    persisted_workspace: Option<String>,
}

#[derive(serde::Serialize)]
struct MobileAuditCommandResult {
    schema_version: String,
    report: gantry_core::mobile::MobileAuditReport,
    report_path: Option<PathBuf>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseEvidenceCommandInput {
    release: String,
    platform: Option<String>,
    finding_instance_id: String,
    mobile_evidence_kind: String,
    title: String,
    issuer: String,
    url: Option<String>,
    expires_at: Option<String>,
    actor: Option<String>,
}

fn is_plausible_project_dir(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    if path == Path::new("/") {
        return false;
    }
    if let Ok(home) = std::env::var("HOME")
        && path == Path::new(&home)
    {
        return false;
    }
    if let Ok(home) = std::env::var("USERPROFILE")
        && path == Path::new(&home)
    {
        return false;
    }

    path.join(".git").is_dir()
        || path.join("package.json").is_file()
        || path.join("Cargo.toml").is_file()
        || path.join("go.mod").is_file()
        || path.join(".gitignore").is_file()
        || path.join("README.md").is_file()
}

fn replace_owner_comment(line: &str, new_owner: &str) -> String {
    if let (Some(start), Some(end)) = (line.find("<!--"), line.find("-->")) {
        let before = &line[..start];
        let after = &line[end + 3..];
        format!("{}<!-- owner:{} -->{}", before, new_owner, after)
    } else {
        format!("{} <!-- owner:{} -->", line.trim_end(), new_owner)
    }
}

fn get_gate_category(line: &str) -> Option<String> {
    if let Some(start) = line.find("**") {
        let after_bold = &line[start + 2..];
        if let Some(end) = after_bold.find("**") {
            let cat_content = &after_bold[..end];
            let mut cat = cat_content.trim();
            if cat.ends_with(':') {
                cat = &cat[..cat.len() - 1];
            }
            return Some(cat.trim().to_string());
        }
    }
    None
}

#[tauri::command]
fn detect_project_type_cmd() -> Result<String, String> {
    Ok(detect_project_type().to_string())
}

#[tauri::command]
fn detect_mobile_project_cmd() -> Result<gantry_core::mobile::MobileProject, String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    gantry_core::mobile::detect_mobile_project(&root)
}

#[tauri::command]
async fn run_mobile_audit_cmd(
    artifact_path: Option<String>,
    write: bool,
    request_id: Option<String>,
) -> Result<MobileAuditCommandResult, String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let artifact = artifact_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| safe_resolve_project_path_ctx(&root, value))
        .transpose()?;
    let request_id = request_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if request_id.as_ref().is_some_and(|value| value.len() > 128) {
        return Err("Mobile audit request identifier is too long.".to_string());
    }
    let cancellation = gantry_core::mobile::MobileAuditCancellation::default();
    if let Some(id) = &request_id {
        MOBILE_AUDIT_CANCELLATIONS
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
            .map_err(|_| "Mobile audit cancellation registry is unavailable.".to_string())?
            .insert(id.clone(), cancellation.clone());
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        let report = gantry_core::mobile::run_mobile_audit_with_cancellation(
            &root,
            artifact.as_deref(),
            &cancellation,
        )?;
        let report_path = if write {
            Some(gantry_core::mobile::write_mobile_report(
                &report,
                None,
                gantry_core::mobile::MobileReportFormat::Json,
            )?)
        } else {
            None
        };
        Ok(MobileAuditCommandResult {
            schema_version: gantry_core::mobile::MOBILE_REPORT_SCHEMA_VERSION.to_string(),
            report,
            report_path,
        })
    })
    .await
    .map_err(|error| format!("Mobile audit worker failed: {error}"))?;
    if let Some(id) = request_id
        && let Ok(mut registry) = MOBILE_AUDIT_CANCELLATIONS
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
    {
        registry.remove(&id);
    }
    result
}

#[tauri::command]
fn cancel_mobile_audit_cmd(request_id: String) -> Result<bool, String> {
    let mut registry = MOBILE_AUDIT_CANCELLATIONS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .map_err(|_| "Mobile audit cancellation registry is unavailable.".to_string())?;
    let Some(cancellation) = registry.get(&request_id) else {
        return Ok(false);
    };
    cancellation.cancel();
    registry.remove(&request_id);
    Ok(true)
}

#[tauri::command]
async fn pick_mobile_artifact_cmd(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let selected = app
        .dialog()
        .file()
        .add_filter("Mobile release artifacts", &["ipa", "aab", "apk"])
        .blocking_pick_file();
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected.into_path().map_err(|error| error.to_string())?;
    let root = std::env::current_dir().map_err(|error| error.to_string())?;
    let relative = path
        .strip_prefix(&root)
        .map_err(|_| "Artifact must be inside the current project".to_string())?;
    let resolved = safe_resolve_project_path_ctx(&root, &relative.to_string_lossy())?;
    Ok(Some(
        resolved
            .strip_prefix(&root)
            .map_err(|_| "Artifact must be inside the current project".to_string())?
            .to_string_lossy()
            .replace('\\', "/"),
    ))
}

#[tauri::command]
fn init_mobile_policy_cmd(force: bool) -> Result<String, String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    gantry_core::mobile::init_mobile_policy(&root, force)
        .map(|path| path.to_string_lossy().into_owned())
}

fn desktop_vault_context() -> Result<gantry_core::VaultContext, String> {
    let root = std::env::current_dir().map_err(|error| error.to_string())?;
    let vault = root.join(".gantry");
    if !vault.is_dir() {
        return Err(format!("Gantry vault not found at {}", vault.display()));
    }
    Ok(gantry_core::VaultContext {
        vault_path: vault,
        project_root: root,
    })
}

fn desktop_release_store() -> Result<gantry_core::release::ReleaseStore, String> {
    let vault = desktop_vault_context()?;
    Ok(gantry_core::release::ReleaseStore::new(
        vault.vault_path,
        vault.project_root,
    ))
}

#[tauri::command]
async fn check_vault_cmd(repair: bool) -> Result<gantry_core::release::VaultHealthReport, String> {
    tauri::async_runtime::spawn_blocking(move || desktop_release_store()?.check_vault(repair))
        .await
        .map_err(|error| format!("Vault health worker failed: {error}"))?
}

#[tauri::command]
async fn migrate_vault_cmd(
    dry_run: bool,
) -> Result<gantry_core::release::VaultMigrationReport, String> {
    tauri::async_runtime::spawn_blocking(move || desktop_release_store()?.migrate_vault(dry_run))
        .await
        .map_err(|error| format!("Vault migration worker failed: {error}"))?
}

#[tauri::command]
async fn write_support_bundle_cmd() -> Result<gantry_core::SupportBundleResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let vault = desktop_vault_context()?;
        gantry_core::write_redacted_support_bundle(&vault, None)
    })
    .await
    .map_err(|error| format!("Support bundle worker failed: {error}"))?
}

fn desktop_release_platform(
    value: Option<String>,
) -> Result<Option<gantry_core::release::MobilePlatform>, String> {
    value
        .map(|value| match value.trim().to_ascii_lowercase().as_str() {
            "ios" => Ok(gantry_core::release::MobilePlatform::Ios),
            "android" => Ok(gantry_core::release::MobilePlatform::Android),
            other => Err(format!("Unsupported release platform: {other}")),
        })
        .transpose()
}

#[tauri::command]
async fn list_release_workflows_cmd() -> Result<Vec<gantry_core::release::MobileRelease>, String> {
    tauri::async_runtime::spawn_blocking(|| desktop_release_store()?.list())
        .await
        .map_err(|error| format!("Release list worker failed: {error}"))?
}

#[tauri::command]
async fn ensure_release_workflow_cmd(
    version: String,
    name: Option<String>,
    owner: Option<String>,
    actor: Option<String>,
) -> Result<gantry_core::release::MobileRelease, String> {
    tauri::async_runtime::spawn_blocking(move || {
        desktop_release_store()?.ensure_release(gantry_core::release::NewRelease {
            version,
            name,
            owner,
            actor,
        })
    })
    .await
    .map_err(|error| format!("Release creation worker failed: {error}"))?
}

#[tauri::command]
async fn schedule_release_cmd(
    release: String,
    target_date: Option<String>,
    submission_date: Option<String>,
    rollout_date: Option<String>,
    actor: Option<String>,
) -> Result<gantry_core::release::MobileRelease, String> {
    tauri::async_runtime::spawn_blocking(move || {
        desktop_release_store()?.schedule(gantry_core::release::NewReleaseSchedule {
            release,
            target_date,
            submission_date,
            rollout_date,
            actor,
        })
    })
    .await
    .map_err(|error| format!("Release schedule worker failed: {error}"))?
}

#[tauri::command]
async fn release_portfolio_cmd() -> Result<gantry_core::release::ReleasePortfolio, String> {
    tauri::async_runtime::spawn_blocking(|| desktop_release_store()?.portfolio())
        .await
        .map_err(|error| format!("Release portfolio worker failed: {error}"))?
}

#[tauri::command]
async fn show_release_workflow_cmd(
    release: String,
) -> Result<gantry_core::release::ReleaseView, String> {
    tauri::async_runtime::spawn_blocking(move || desktop_release_store()?.load(&release))
        .await
        .map_err(|error| format!("Release detail worker failed: {error}"))?
}

#[tauri::command]
async fn write_release_packet_cmd(
    release: String,
) -> Result<gantry_core::release::ReleasePacketArtifacts, String> {
    tauri::async_runtime::spawn_blocking(move || desktop_release_store()?.write_packet(&release))
        .await
        .map_err(|error| format!("Release packet worker failed: {error}"))?
}

#[tauri::command]
async fn diff_release_candidates_cmd(
    release: String,
    platform: Option<String>,
    from_candidate_id: Option<String>,
    to_candidate_id: Option<String>,
) -> Result<gantry_core::release::ReleaseCandidateDiff, String> {
    let platform = desktop_release_platform(platform)?;
    tauri::async_runtime::spawn_blocking(move || {
        desktop_release_store()?.diff_candidates(gantry_core::release::NewReleaseDiff {
            release,
            platform,
            from_candidate_id,
            to_candidate_id,
        })
    })
    .await
    .map_err(|error| format!("Release diff worker failed: {error}"))?
}

#[tauri::command]
async fn decide_release_cmd(
    release: String,
    platform: Option<String>,
    decision: String,
    decided_by: String,
    role: String,
    rationale: String,
) -> Result<gantry_core::release::HumanReleaseDecision, String> {
    let platform = desktop_release_platform(platform)?;
    let decision = match decision.trim().to_ascii_lowercase().as_str() {
        "approved" | "approve" => gantry_core::release::HumanDecisionKind::Approved,
        "rejected" | "reject" => gantry_core::release::HumanDecisionKind::Rejected,
        other => return Err(format!("Unsupported release decision: {other}")),
    };
    tauri::async_runtime::spawn_blocking(move || {
        desktop_release_store()?.decide(gantry_core::release::NewHumanDecision {
            release,
            platform,
            decided_by,
            role,
            rationale,
            decision,
        })
    })
    .await
    .map_err(|error| format!("Release decision worker failed: {error}"))?
}

#[tauri::command]
async fn list_release_reports_cmd(
    release: String,
) -> Result<Vec<gantry_core::mobile::MobileAuditReport>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        desktop_release_store()?
            .build_packet(&release)
            .map(|packet| packet.reports)
    })
    .await
    .map_err(|error| format!("Release report worker failed: {error}"))?
}

#[tauri::command]
async fn add_release_candidate_cmd(
    release: String,
    artifact_path: String,
    platform: Option<String>,
    actor: Option<String>,
) -> Result<gantry_core::release::ReleaseCandidate, String> {
    let root = std::env::current_dir().map_err(|error| error.to_string())?;
    let artifact_path = safe_resolve_project_path_ctx(&root, &artifact_path)?;
    let platform = desktop_release_platform(platform)?;
    tauri::async_runtime::spawn_blocking(move || {
        desktop_release_store()?.add_candidate(gantry_core::release::NewCandidate {
            release,
            artifact_path,
            platform,
            actor,
        })
    })
    .await
    .map_err(|error| format!("Candidate binding worker failed: {error}"))?
}

#[tauri::command]
async fn audit_release_candidate_cmd(
    release: String,
    platform: Option<String>,
    actor: Option<String>,
) -> Result<gantry_core::release::ReleaseAuditResult, String> {
    let platform = desktop_release_platform(platform)?;
    tauri::async_runtime::spawn_blocking(move || {
        desktop_release_store()?.audit(gantry_core::release::NewReleaseAudit {
            release,
            platform,
            actor,
        })
    })
    .await
    .map_err(|error| format!("Candidate audit worker failed: {error}"))?
}

#[tauri::command]
async fn add_release_evidence_cmd(
    input: ReleaseEvidenceCommandInput,
) -> Result<gantry_core::release::ReleaseEvidenceResult, String> {
    let platform = desktop_release_platform(input.platform)?;
    tauri::async_runtime::spawn_blocking(move || {
        desktop_release_store()?.add_evidence(gantry_core::release::NewReleaseEvidence {
            release: input.release,
            platform,
            finding_instance_id: input.finding_instance_id,
            mobile_evidence_kind: input.mobile_evidence_kind,
            title: input.title,
            issuer: input.issuer,
            path: None,
            url: input.url,
            expires_at: input.expires_at,
            actor: input.actor,
        })
    })
    .await
    .map_err(|error| format!("Release evidence worker failed: {error}"))?
}

#[tauri::command]
async fn resolve_release_finding_cmd(
    release: String,
    platform: Option<String>,
    finding_instance_id: String,
    reason: String,
    evidence_ids: Vec<String>,
    actor: Option<String>,
) -> Result<gantry_core::release::FindingResolution, String> {
    let platform = desktop_release_platform(platform)?;
    tauri::async_runtime::spawn_blocking(move || {
        desktop_release_store()?.resolve_finding(gantry_core::release::NewFindingResolution {
            release,
            platform,
            finding_instance_id,
            reason,
            evidence_ids,
            actor,
        })
    })
    .await
    .map_err(|error| format!("Finding resolution worker failed: {error}"))?
}

#[tauri::command]
async fn waive_release_finding_cmd(
    release: String,
    platform: Option<String>,
    finding_instance_id: String,
    reason: String,
    approved_by: String,
    expires_at: String,
) -> Result<gantry_core::release::ReleaseFindingWaiver, String> {
    let platform = desktop_release_platform(platform)?;
    tauri::async_runtime::spawn_blocking(move || {
        desktop_release_store()?.waive_finding(gantry_core::release::NewReleaseWaiver {
            release,
            platform,
            finding_instance_id,
            reason,
            approved_by,
            expires_at,
        })
    })
    .await
    .map_err(|error| format!("Finding waiver worker failed: {error}"))?
}

#[tauri::command]
fn open_project_file(path: &str) -> Result<(), String> {
    let full_path = safe_resolve_project_path(path)?;
    let path_str = full_path.to_string_lossy().into_owned();

    if Command::new("code").arg(&path_str).status().is_ok() {
        Ok(())
    } else {
        Command::new("open")
            .arg(&path_str)
            .status()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn init_customized_vault_cmd(options: InitOptions) -> Result<(), String> {
    // 1. Initialize vault with template
    init_vault_with_template(&options.template_type, options.force)?;

    // 2. Resolve paths
    let vault_path = get_vault_path();
    let config_path = vault_path.join("config.json");
    let go_no_go_path = vault_path.join("go_no_go.md");
    let qa_path = vault_path.join("qa").join("regression.md");

    // 3. Customize config.json
    if config_path.exists() {
        let content = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
        if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&content) {
            json["version"] = serde_json::Value::String(options.version.clone());
            json["releaseDate"] = serde_json::Value::String(options.release_date.clone());
            if let Ok(updated) = serde_json::to_string_pretty(&json) {
                fs::write(&config_path, updated).map_err(|e| e.to_string())?;
            }
        }
    }

    // 4. Customize go_no_go.md
    if go_no_go_path.exists() {
        let content = fs::read_to_string(&go_no_go_path).map_err(|e| e.to_string())?;
        let (mut fm, body) = parse_frontmatter(&content)?;

        // Update frontmatter
        fm["release"] = serde_json::Value::String(options.version.clone());
        fm["target_date"] = serde_json::Value::String(options.release_date.clone());

        let fm_str = yaml_serde::to_string(&fm).map_err(|e| e.to_string())?;

        // Customize body gates
        let mut new_body_lines = Vec::new();
        let mut in_gates_section = false;

        for line in body.lines() {
            if line.contains("## Required Launch Gates") || line.contains("## Required Gates") {
                in_gates_section = true;
                new_body_lines.push(line.to_string());
                continue;
            }
            if in_gates_section && line.trim().starts_with('#') {
                in_gates_section = false;
            }

            if in_gates_section && line.trim().starts_with("- [ ]") {
                let mut enabled = false;
                let mut category_found = String::new();
                if let Some(cat) = get_gate_category(line) {
                    for gate_name in &options.enabled_gates {
                        if cat.to_lowercase() == gate_name.to_lowercase() {
                            enabled = true;
                            category_found = cat.clone();
                            break;
                        }
                    }
                }

                if enabled {
                    let target_owner = if category_found.to_lowercase().contains("qa") {
                        &options.qa_lead
                    } else {
                        &options.lead_owner
                    };

                    let updated_line = replace_owner_comment(line, target_owner);
                    new_body_lines.push(updated_line);
                }
            } else {
                new_body_lines.push(line.to_string());
            }
        }

        let new_content = format!("---\n{}---\n{}", fm_str, new_body_lines.join("\n"));
        fs::write(&go_no_go_path, new_content).map_err(|e| e.to_string())?;
    }

    // 5. Customize qa/regression.md
    if qa_path.exists() {
        if options.create_qa {
            let content = fs::read_to_string(&qa_path).map_err(|e| e.to_string())?;
            let (mut fm, body) = parse_frontmatter(&content)?;

            fm["owner"] = serde_json::Value::String(options.qa_lead.clone());
            let fm_str = yaml_serde::to_string(&fm).map_err(|e| e.to_string())?;

            let new_body_lines: Vec<String> = body
                .lines()
                .map(|line| {
                    if line.contains("<!--") && line.contains("-->") {
                        replace_owner_comment(line, &options.qa_lead)
                    } else {
                        line.to_string()
                    }
                })
                .collect();

            let new_content = format!("---\n{}---\n{}", fm_str, new_body_lines.join("\n"));
            fs::write(&qa_path, new_content).map_err(|e| e.to_string())?;
        } else {
            let _ = fs::remove_file(&qa_path);
        }
    }

    // Rebuild index
    write_launch_index()?;
    Ok(())
}

#[tauri::command]
fn get_workspace_status(app: tauri::AppHandle) -> WorkspaceStatus {
    let persisted_workspace = load_workspace_path(&app);
    let cwd = std::env::current_dir().unwrap_or_default();
    let has_vault = cwd.join(".gantry").is_dir();

    let state = if has_vault {
        "vaultLoaded".to_string()
    } else if persisted_workspace.is_some() || is_plausible_project_dir(&cwd) {
        "workspaceWithoutVault".to_string()
    } else {
        "noWorkspace".to_string()
    };

    WorkspaceStatus {
        state,
        path: cwd.to_string_lossy().to_string(),
        has_vault,
        persisted_workspace,
    }
}

#[tauri::command]
fn gantry_audit(write: bool) -> Result<IndexPayload, String> {
    run_launch_audit_ex(write)
}

#[tauri::command]
fn init_vault_with_template_cmd(template_type: &str, force: bool) -> Result<(), String> {
    init_vault_with_template(template_type, force)
}

#[tauri::command]
fn list_pending_tasks_cmd() -> Result<Vec<GantryTask>, String> {
    list_pending_tasks()
}

#[tauri::command]
fn complete_task_by_id_cmd(id: &str) -> Result<(), String> {
    complete_task_by_id(id)
}

#[tauri::command]
fn ignore_task_cmd(id: &str, reason: &str) -> Result<(), String> {
    ignore_task_by_id(id, reason)
}

#[tauri::command]
fn read_vault_file(path: &str) -> Result<String, String> {
    let full_path = safe_resolve_vault_path(path)?;
    fs::read_to_string(&full_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn write_vault_file(path: &str, content: &str) -> Result<(), String> {
    let full_path = safe_resolve_vault_path(path)?;
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&full_path, content).map_err(|e| e.to_string())?;

    // Automatically trigger index rebuild when files are written via the UI
    let _ = write_launch_index();
    Ok(())
}

#[tauri::command]
fn list_vault_files() -> Result<Vec<String>, String> {
    let vault_path = get_vault_path();
    let mut files = vec![];
    collect_vault_files(&vault_path, &vault_path, &mut files);
    files.sort();
    Ok(files)
}

fn collect_vault_files(base: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            collect_vault_files(base, &path, out);
        } else {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let skip = name == "index.json" || name.ends_with(".schema.json");
            let is_md = path.extension().is_some_and(|e| e == "md");
            let is_config = name == "config.json";
            if !skip
                && (is_md || is_config)
                && let Ok(rel) = path.strip_prefix(base)
            {
                out.push(rel.to_string_lossy().into_owned());
            }
        }
    }
}

#[tauri::command]
fn open_in_ide(path: &str) -> Result<(), String> {
    let full_path = safe_resolve_vault_path(path)?;
    let path_str = full_path.to_string_lossy().into_owned();

    // Try VS Code ('code') launcher, else fall back to default macOS handler ('open')
    if Command::new("code").arg(&path_str).status().is_ok() {
        Ok(())
    } else {
        Command::new("open")
            .arg(&path_str)
            .status()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn rebuild_gantry_index(env: Option<String>) -> Result<IndexPayload, String> {
    // Pure compile/validate check - does not write files
    compile_launch_index_for_environment(env.as_deref())
}

#[tauri::command]
fn get_recent_changes() -> Result<Vec<RecentChange>, String> {
    let workspace_root = std::env::current_dir().unwrap_or_default();

    // Try git log first for project-wide recent commits
    let output = Command::new("git")
        .args(["log", "--oneline", "-20", "--format=%H%x1f%ai%x1f%s%x1f%an"])
        .current_dir(&workspace_root)
        .output();

    if let Ok(out) = output
        && out.status.success()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let changes: Vec<RecentChange> = stdout
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(4, '\x1f').collect();
                if parts.len() == 4 {
                    let message = parts[2].to_string();
                    let kind = ChangeKind::from_message(&message);
                    Some(RecentChange {
                        hash: parts[0][..7.min(parts[0].len())].to_string(),
                        date: parts[1].to_string(),
                        message,
                        author: parts[3].to_string(),
                        kind,
                    })
                } else {
                    None
                }
            })
            .collect();
        return Ok(changes);
    }

    // Fallback: list .gantry/ files by modification time
    let vault_path = get_vault_path();
    if !vault_path.exists() {
        return Ok(Vec::new());
    }

    let mut file_changes: Vec<RecentChange> = Vec::new();
    fn collect_files(dir: &Path, changes: &mut Vec<RecentChange>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .file_name()
                    .map(|n| n.to_string_lossy().starts_with('.'))
                    .unwrap_or(false)
                {
                    continue;
                }
                if path.is_dir() {
                    collect_files(&path, changes);
                } else if let Ok(meta) = fs::metadata(&path)
                    && let Ok(modified) = meta.modified()
                    && let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH)
                {
                    let secs = duration.as_secs();
                    let dt = chrono::DateTime::from_timestamp(secs as i64, 0).unwrap_or_default();
                    changes.push(RecentChange {
                        hash: "".to_string(),
                        date: dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                        message: format!(
                            "File modified: {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        author: "filesystem".to_string(),
                        kind: ChangeKind::Docs,
                    });
                }
            }
        }
    }
    collect_files(&vault_path, &mut file_changes);
    file_changes.sort_by(|a, b| b.date.cmp(&a.date));
    file_changes.truncate(20);
    Ok(file_changes)
}

#[derive(serde::Serialize)]
struct CommitDetails {
    hash: String,
    author: String,
    date: String,
    message: String,
    body: String,
    stat: String,
}

#[tauri::command]
fn get_commit_details(hash: &str) -> Result<CommitDetails, String> {
    let workspace_root = std::env::current_dir().unwrap_or_default();

    // Fetch commit info: author, date, subject, body using Unit Separator
    let info_output = Command::new("git")
        .args(["show", "-s", "--format=%an%x1f%ai%x1f%s%x1f%b", hash])
        .current_dir(&workspace_root)
        .output();

    // Fetch changed files stat
    let stat_output = Command::new("git")
        .args(["show", "--stat", "--oneline", hash])
        .current_dir(&workspace_root)
        .output();

    let info_str = match info_output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => return Err("Failed to get commit info".to_string()),
    };

    let stat_str = match stat_output {
        Ok(out) if out.status.success() => {
            let full_stat = String::from_utf8_lossy(&out.stdout).trim().to_string();
            // Skip the first line (the subject line from --oneline)
            full_stat.lines().skip(1).collect::<Vec<&str>>().join("\n")
        }
        _ => "".to_string(),
    };

    let parts: Vec<&str> = info_str.splitn(4, '\x1f').collect();
    if parts.len() >= 3 {
        Ok(CommitDetails {
            hash: hash.to_string(),
            author: parts[0].to_string(),
            date: parts[1].to_string(),
            message: parts[2].to_string(),
            body: parts.get(3).unwrap_or(&"").to_string(),
            stat: stat_str,
        })
    } else {
        Err("Failed to parse commit details".to_string())
    }
}

#[tauri::command]
fn update_blocker_field(path: &str, field: &str, value: &str) -> Result<(), String> {
    // Only allow known safe fields to be updated
    let allowed_fields = ["status", "severity", "owner", "last_verified"];
    if !allowed_fields.contains(&field) {
        return Err(format!(
            "Field '{}' is not editable. Allowed: {:?}",
            field, allowed_fields
        ));
    }

    let full_path = safe_resolve_vault_path(path)?;
    let content = fs::read_to_string(&full_path).map_err(|e| e.to_string())?;

    let (fm, body) = parse_frontmatter(&content)?;
    let mut fm_map = match fm {
        JsonValue::Object(map) => map,
        _ => return Err("Frontmatter is not a valid object".to_string()),
    };

    // Update the specified field
    fm_map.insert(field.to_string(), JsonValue::String(value.to_string()));

    // Reconstruct the frontmatter YAML
    let yaml_value: yaml_serde::Value = serde_json::from_value(JsonValue::Object(fm_map.clone()))
        .map_err(|e| {
        format!(
            "Failed to convert frontmatter for YAML serialization: {}",
            e
        )
    })?;
    let yaml_str = yaml_serde::to_string(&yaml_value)
        .map_err(|e| format!("Failed to serialize YAML: {}", e))?;

    // Write back: --- + yaml + --- + body (preserving body content)
    let new_content = format!("---\n{}---{}", yaml_str, body);
    fs::write(&full_path, new_content).map_err(|e| e.to_string())?;

    // Rebuild index after field update
    let _ = write_launch_index();
    Ok(())
}

fn checklist_comment_owner(comment: &str) -> String {
    comment
        .split_whitespace()
        .find_map(|part| {
            if part.starts_with('@') {
                Some(part.to_string())
            } else {
                part.strip_prefix("owner:").map(|owner| owner.to_string())
            }
        })
        .filter(|owner| !owner.is_empty())
        .unwrap_or_else(|| "@unassigned".to_string())
}

fn git_user_handle() -> String {
    let name = get_git_user().map(|u| u.name).unwrap_or_default();
    let fallback = if name.trim().is_empty() {
        "unknown".to_string()
    } else {
        name
    };
    let handle: String = fallback
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    format!(
        "@{}",
        if handle.is_empty() {
            "unknown"
        } else {
            &handle
        }
    )
}

fn update_checklist_comment(line: &str, checked: bool) -> String {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut owner = "@unassigned".to_string();
    let before_comment;

    if let Some(start) = line.find("<!--")
        && let Some(end_rel) = line[start..].find("-->")
    {
        let end = start + end_rel + 3;
        let comment = if end_rel >= 4 {
            line[start + 4..start + end_rel].trim()
        } else {
            ""
        };
        owner = checklist_comment_owner(comment);
        before_comment = line[..start].trim_end().to_string();
        let new_comment = if checked {
            format!("<!-- {} {} by:{} -->", owner, today, git_user_handle())
        } else {
            format!("<!-- {} -->", owner)
        };
        let trailing = line[end..].trim_start();
        if trailing.is_empty() {
            return format!("{} {}", before_comment, new_comment);
        }
        return format!("{} {} {}", before_comment, new_comment, trailing);
    }

    let new_comment = if checked {
        format!("<!-- {} {} by:{} -->", owner, today, git_user_handle())
    } else {
        format!("<!-- {} -->", owner)
    };
    format!("{} {}", line.trim_end(), new_comment)
}

#[tauri::command]
fn toggle_checklist_item(path: &str, line_index: usize, checked: bool) -> Result<(), String> {
    let full_path = safe_resolve_vault_path(path)?;
    if !full_path.exists() {
        return Err(format!("Vault file '{}' does not exist", path));
    }

    let content = fs::read_to_string(&full_path).map_err(|e| e.to_string())?;
    let mut lines: Vec<String> = content.split('\n').map(|line| line.to_string()).collect();
    let line = lines
        .get_mut(line_index)
        .ok_or_else(|| format!("Line {} is outside '{}'", line_index + 1, path))?;

    let marker_start = line
        .find("- [")
        .ok_or_else(|| format!("Line {} is not a markdown checklist item", line_index + 1))?;
    let bytes = line.as_bytes();
    if bytes.len() <= marker_start + 4 || bytes[marker_start + 4] != b']' {
        return Err(format!(
            "Line {} is not a markdown checklist item",
            line_index + 1
        ));
    }
    let status = bytes[marker_start + 3] as char;
    if !matches!(status, ' ' | 'x' | 'X' | '/') {
        return Err(format!(
            "Line {} has an unsupported checklist state",
            line_index + 1
        ));
    }

    line.replace_range(
        marker_start + 3..marker_start + 4,
        if checked { "x" } else { " " },
    );
    *line = update_checklist_comment(line, checked);

    fs::write(&full_path, lines.join("\n")).map_err(|e| e.to_string())?;
    let _ = write_launch_index();
    Ok(())
}

fn yaml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn safe_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in value.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed
    }
}

fn safe_doc_id(value: &str) -> Result<String, String> {
    let id = value.trim();
    if id.is_empty() {
        return Err("ID is required".to_string());
    }
    if !id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err("ID may only contain letters, numbers, dashes, and underscores".to_string());
    }
    Ok(id.to_string())
}

#[tauri::command]
fn create_blocker_file(
    id: &str,
    title: &str,
    severity: &str,
    owner: &str,
    source_files: Vec<String>,
) -> Result<String, String> {
    let id = safe_doc_id(id)?;
    let title = title.trim();
    if title.is_empty() {
        return Err("Title is required".to_string());
    }
    if !matches!(severity, "P0" | "P1" | "P2") {
        return Err("Severity must be P0, P1, or P2".to_string());
    }

    let owner = owner.trim();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let path = format!("blockers/{}-{}.md", id, safe_slug(title));
    let full_path = safe_resolve_vault_path(&path)?;
    if full_path.exists() {
        return Err(format!("A blocker file already exists at {}", path));
    }
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let sources: Vec<&str> = source_files
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let source_yaml = if sources.is_empty() {
        "source_files: []\n".to_string()
    } else {
        let mut yaml = "source_files:\n".to_string();
        for source in sources {
            yaml.push_str(&format!("  - {}\n", yaml_string(source)));
        }
        yaml
    };

    let content = format!(
        "---\ntype: blocker\nid: {}\ntitle: {}\nseverity: {}\nstatus: open\nowner: {}\n{}last_verified: {}\ncreated_at: {}\n---\n\n# {}\n\n## Description\n\n\n## Action Items\n\n- [ ] Triage impact and reproduction steps. <!-- {} -->\n",
        id,
        yaml_string(title),
        severity,
        yaml_string(owner),
        source_yaml,
        today,
        today,
        title,
        owner
    );

    fs::write(&full_path, content).map_err(|e| e.to_string())?;
    let _ = write_launch_index();
    Ok(path)
}

#[tauri::command]
fn create_qa_file(
    id: &str,
    title: &str,
    owner: &str,
    platforms: Vec<String>,
) -> Result<String, String> {
    let id = safe_doc_id(id)?;
    let title = title.trim();
    if title.is_empty() {
        return Err("Title is required".to_string());
    }

    let owner = owner.trim();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let path = format!("qa/{}.md", safe_slug(title));
    let full_path = safe_resolve_vault_path(&path)?;
    if full_path.exists() {
        return Err(format!("A QA file already exists at {}", path));
    }
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let platform_values: Vec<&str> = platforms
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let platforms_yaml = if platform_values.is_empty() {
        "platforms: []\n".to_string()
    } else {
        let mut yaml = "platforms:\n".to_string();
        for platform in platform_values {
            yaml.push_str(&format!("  - {}\n", yaml_string(platform)));
        }
        yaml
    };

    let content = format!(
        "---\ntype: qa\nid: {}\ntitle: {}\nowner: {}\n{}last_verified: {}\n---\n\n# {}\n\n## Test Cases\n\n- [ ] Verify primary release flow. <!-- {} -->\n- [ ] Verify rollback path is documented. <!-- {} -->\n",
        id,
        yaml_string(title),
        yaml_string(owner),
        platforms_yaml,
        today,
        title,
        owner,
        owner
    );

    fs::write(&full_path, content).map_err(|e| e.to_string())?;
    let _ = write_launch_index();
    Ok(path)
}

static CURRENT_WATCHER: OnceLock<Mutex<Option<RecommendedWatcher>>> = OnceLock::new();

#[tauri::command]
fn watch_vault(app_handle: AppHandle) -> Result<(), String> {
    let project_root = std::env::current_dir().unwrap_or_default();

    // Drop the old watcher first to close its sender and trigger loop termination
    let watcher_mutex = CURRENT_WATCHER.get_or_init(|| Mutex::new(None));
    {
        let mut lock = watcher_mutex.lock().map_err(|_| {
            "The vault watcher lock is unavailable after an earlier failure.".to_string()
        })?;
        *lock = None;
    }

    // Spawn new watcher thread
    let (tx, rx) = channel();
    let mut watcher = RecommendedWatcher::new(tx, Config::default()).map_err(|e| e.to_string())?;
    watcher
        .watch(&project_root, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    // Store in global static to keep it alive
    {
        let mut lock = watcher_mutex.lock().map_err(|_| {
            "The vault watcher lock is unavailable after an earlier failure.".to_string()
        })?;
        *lock = Some(watcher);
    }

    thread::spawn(move || {
        while let Ok(res) = rx.recv() {
            if let Ok(event) = res {
                let mut ignore = false;
                for path in &event.paths {
                    let path_str = path.to_string_lossy();
                    if path_str.contains(".git/")
                        || path_str.contains(".git\\")
                        || path_str.contains("node_modules/")
                        || path_str.contains("node_modules\\")
                        || path_str.ends_with("index.json")
                        || path_str.ends_with("/.gantry/state.json")
                        || path_str.ends_with("\\.gantry\\state.json")
                        || path_str.contains("/.gantry/snapshots/")
                        || path_str.contains("\\.gantry\\snapshots\\")
                    {
                        ignore = true;
                        break;
                    }
                }
                if ignore {
                    continue;
                }

                // Coalesce / debounce rapid IDE saves (150ms delay)
                thread::sleep(Duration::from_millis(150));
                while rx.try_recv().is_ok() {}

                // Trigger dynamic index write
                let _ = write_launch_index();
                // Emit vault-changed event to the UI
                let _ = app_handle.emit("vault-changed", ());
            }
        }
    });

    Ok(())
}

#[derive(serde::Serialize)]
struct GitStatus {
    is_git_repo: bool,
    repo_root: String,
}

#[tauri::command]
fn check_git_status() -> Result<GitStatus, String> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&cwd)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
            Ok(GitStatus {
                is_git_repo: true,
                repo_root: root,
            })
        }
        _ => Ok(GitStatus {
            is_git_repo: false,
            repo_root: String::new(),
        }),
    }
}

#[derive(serde::Serialize)]
struct GitUser {
    name: String,
    email: String,
}

#[derive(serde::Serialize)]
struct CiStatus {
    configured: bool,
    provider: String,
    file_path: String,
}

#[tauri::command]
fn get_git_user() -> Result<GitUser, String> {
    let workspace_root = std::env::current_dir().unwrap_or_default();
    let name_output = Command::new("git")
        .args(["config", "user.name"])
        .current_dir(&workspace_root)
        .output();
    let email_output = Command::new("git")
        .args(["config", "user.email"])
        .current_dir(&workspace_root)
        .output();

    let name = if let Ok(out) = name_output {
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        "".to_string()
    };

    let email = if let Ok(out) = email_output {
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        "".to_string()
    };

    Ok(GitUser { name, email })
}

#[tauri::command]
fn get_ci_status() -> Result<CiStatus, String> {
    let workspace_root = std::env::current_dir().unwrap_or_default();

    // Check GitLab first
    let gitlab_ci = workspace_root.join(".gitlab-ci.yml");
    if gitlab_ci.exists()
        && let Ok(content) = fs::read_to_string(&gitlab_ci)
        && (content.contains("gantry validate")
            || content.contains("validate --ci")
            || content.contains("trygantry/gantry"))
    {
        return Ok(CiStatus {
            configured: true,
            provider: "gitlab".to_string(),
            file_path: ".gitlab-ci.yml".to_string(),
        });
    }

    // Check GitHub Workflows
    let github_workflows = workspace_root.join(".github").join("workflows");
    if github_workflows.exists()
        && github_workflows.is_dir()
        && let Ok(entries) = fs::read_dir(&github_workflows)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension()
                && (ext == "yml" || ext == "yaml")
                && let Ok(content) = fs::read_to_string(&path)
                && (content.contains("gantry validate")
                    || content.contains("validate --ci")
                    || content.contains("trygantry/gantry")
                    || content.contains("gantry-action")
                    || content.contains("gantry-validate"))
            {
                return Ok(CiStatus {
                    configured: true,
                    provider: "github".to_string(),
                    file_path: format!(
                        ".github/workflows/{}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                });
            }
        }
    }

    Ok(CiStatus {
        configured: false,
        provider: "none".to_string(),
        file_path: "".to_string(),
    })
}

#[tauri::command]
fn get_impact_summary_cmd(base_ref: Option<String>) -> Result<ImpactSummary, String> {
    get_impact_summary_ex(base_ref.as_deref())
}

#[tauri::command]
fn get_impact_graph_cmd(base_ref: Option<String>) -> Result<ImpactGraph, String> {
    get_impact_graph(base_ref.as_deref())
}

// --- Workspace persistence helpers ---

fn workspace_config_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_default()
        .join("workspace.json")
}

fn save_workspace_path(app: &tauri::AppHandle, path: &str) -> Result<(), String> {
    let config_path = workspace_config_path(app);
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::json!({ "path": path });
    fs::write(&config_path, json.to_string()).map_err(|e| e.to_string())
}

fn load_workspace_path(app: &tauri::AppHandle) -> Option<String> {
    let config_path = workspace_config_path(app);
    let content = fs::read_to_string(&config_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json["path"].as_str().map(|s| s.to_string())
}

// --- Workspace commands ---

#[tauri::command]
fn get_current_workspace() -> String {
    std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

#[tauri::command]
async fn pick_project_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let folder = app.dialog().file().blocking_pick_folder();

    match folder {
        Some(file_path) => {
            let path_buf = file_path.into_path().map_err(|e| e.to_string())?;
            if !path_buf.is_dir() {
                return Err("Selected path is not a directory".to_string());
            }
            let path_str = path_buf.to_string_lossy().to_string();
            std::env::set_current_dir(&path_buf).map_err(|e| e.to_string())?;
            save_workspace_path(&app, &path_str)?;
            Ok(Some(path_str))
        }
        None => Ok(None), // user cancelled
    }
}

#[tauri::command]
fn get_waivers_cmd() -> Result<Vec<Waiver>, String> {
    let vault_path = get_vault_path();
    let waivers_path = vault_path.join("waivers.json");
    if !waivers_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&waivers_path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_release_manifest_cmd(release: Option<String>) -> Result<ReleaseManifest, String> {
    let vault_path = get_vault_path();
    let release_version = match release {
        Some(r) => r,
        None => {
            let config_path = vault_path.join("config.json");
            if !config_path.exists() {
                return Err("config.json not found".to_string());
            }
            let content = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
            let config: serde_json::Value =
                serde_json::from_str(&content).map_err(|e| e.to_string())?;
            config
                .get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| "version not found in config.json".to_string())?
        }
    };

    let manifest_path = vault_path
        .join("releases")
        .join(format!("{}.json", release_version));
    if !manifest_path.exists() {
        return Err(format!(
            "No release manifest found for version '{}'",
            release_version
        ));
    }
    let content = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_trends_cmd() -> Result<TrendSummary, String> {
    compile_trends()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<(), String> {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // If the current working directory already has a .gantry/ vault (e.g. launched
            // from a project terminal), honour it. Also, if CWD is a plausible project
            // directory without a vault yet, honour it so the setup wizard targets the CWD.
            // Only fall back to the persisted workspace when CWD has no vault and is not a
            // plausible project folder (e.g., Dock / Finder launches).
            let cwd = std::env::current_dir().unwrap_or_default();
            if !cwd.join(".gantry").is_dir()
                && !is_plausible_project_dir(&cwd)
                && let Some(saved_path) = load_workspace_path(app.handle())
            {
                let p = std::path::Path::new(&saved_path);
                if p.is_dir() {
                    let _ = std::env::set_current_dir(p);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_vault_file,
            write_vault_file,
            list_vault_files,
            open_in_ide,
            watch_vault,
            rebuild_gantry_index,
            get_recent_changes,
            get_commit_details,
            update_blocker_field,
            toggle_checklist_item,
            create_blocker_file,
            create_qa_file,
            gantry_audit,
            init_vault_with_template_cmd,
            list_pending_tasks_cmd,
            complete_task_by_id_cmd,
            ignore_task_cmd,
            get_impact_summary_cmd,
            check_git_status,
            get_git_user,
            get_ci_status,
            get_current_workspace,
            pick_project_folder,
            detect_project_type_cmd,
            detect_mobile_project_cmd,
            run_mobile_audit_cmd,
            cancel_mobile_audit_cmd,
            pick_mobile_artifact_cmd,
            init_mobile_policy_cmd,
            check_vault_cmd,
            migrate_vault_cmd,
            write_support_bundle_cmd,
            list_release_workflows_cmd,
            ensure_release_workflow_cmd,
            schedule_release_cmd,
            release_portfolio_cmd,
            show_release_workflow_cmd,
            write_release_packet_cmd,
            diff_release_candidates_cmd,
            decide_release_cmd,
            list_release_reports_cmd,
            add_release_candidate_cmd,
            audit_release_candidate_cmd,
            add_release_evidence_cmd,
            resolve_release_finding_cmd,
            waive_release_finding_cmd,
            open_project_file,
            init_customized_vault_cmd,
            get_workspace_status,
            get_impact_graph_cmd,
            get_waivers_cmd,
            get_release_manifest_cmd,
            get_trends_cmd
        ])
        .run(tauri::generate_context!())
        .map_err(|e| format!("Failed to run the Gantry desktop application: {e}"))
}
