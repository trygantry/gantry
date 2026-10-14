use crate::output::Output;
use crate::prompter::Prompter;
use gantry_core::{EvidenceRecord, VaultContext, get_current_git_commit_ctx};

pub enum EvidenceCommand {
    Add {
        kind: Option<String>,
        title: Option<String>,
        path: Option<String>,
        url: Option<String>,
        artifact_sha: Option<String>,
        mobile_evidence_kind: Option<String>,
        evidence_issuer: Option<String>,
        expires_at: Option<String>,
        env: Option<String>,
        release: Option<String>,
        interactive: bool,
    },
}

pub fn handle(
    output: &Output,
    prompter: &dyn Prompter,
    command: EvidenceCommand,
    vault: &VaultContext,
) -> Result<i32, String> {
    let vault_path = &vault.vault_path;
    let project_root = &vault.project_root;

    match command {
        EvidenceCommand::Add {
            kind,
            title,
            path,
            url,
            artifact_sha,
            mobile_evidence_kind,
            evidence_issuer,
            expires_at,
            env,
            release,
            interactive,
        } => {
            let e_kind = if let Some(k) = kind {
                k
            } else if interactive {
                let choices = &["test-run", "manual-signoff", "security-scan", "other"];
                let idx = prompter.select("Select evidence kind", choices, 0)?;
                if choices[idx] == "other" {
                    prompter.input("Enter custom evidence kind", None)?
                } else {
                    choices[idx].to_string()
                }
            } else {
                return Err("Missing required argument: --kind".to_string());
            };

            let e_title = if let Some(t) = title {
                t
            } else if interactive {
                prompter.input("Enter evidence title", None)?
            } else {
                return Err("Missing required argument: --title".to_string());
            };

            let e_path = if path.is_some() {
                path
            } else if interactive {
                let val =
                    prompter.input("Enter file path (optional, press Enter to skip)", Some(""))?;
                if val.trim().is_empty() {
                    None
                } else {
                    Some(val)
                }
            } else {
                None
            };

            let e_url = if url.is_some() {
                url
            } else if interactive {
                let val = prompter.input("Enter URL (optional, press Enter to skip)", Some(""))?;
                if val.trim().is_empty() {
                    None
                } else {
                    Some(val)
                }
            } else {
                None
            };

            let e_sha = if artifact_sha.is_some() {
                artifact_sha
            } else if interactive {
                let val = prompter.input(
                    "Enter artifact SHA (optional, press Enter to skip)",
                    Some(""),
                )?;
                if val.trim().is_empty() {
                    None
                } else {
                    Some(val)
                }
            } else {
                None
            };

            let e_expires_at = expires_at
                .map(|value| {
                    chrono::DateTime::parse_from_rfc3339(&value)
                        .map(|_| value)
                        .map_err(|_| "--expires-at must be valid RFC 3339".to_string())
                })
                .transpose()?;
            let e_mobile_kind = mobile_evidence_kind
                .map(|value| value.trim().to_ascii_lowercase().replace('-', "_"))
                .filter(|value| !value.is_empty());
            let e_issuer = evidence_issuer
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());

            let e_env = if env.is_some() {
                env
            } else if interactive {
                let val = prompter.input(
                    "Enter environment (optional, default: production)",
                    Some("production"),
                )?;
                if val.trim().is_empty() {
                    Some("production".to_string())
                } else {
                    Some(val)
                }
            } else {
                None
            };

            let mut release_ver = "1.0.0".to_string();
            let config_path = vault_path.join("config.json");
            if config_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&config_path) {
                    if let Ok(config_json) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(v) = config_json.get("version").and_then(|v| v.as_str()) {
                            release_ver = v.to_string();
                        }
                    }
                }
            }

            let e_release = if release.is_some() {
                release
            } else if interactive {
                let prompt_msg =
                    format!("Enter release version (optional, default: {})", release_ver);
                let val = prompter.input(&prompt_msg, Some(&release_ver))?;
                if val.trim().is_empty() {
                    Some(release_ver.clone())
                } else {
                    Some(val)
                }
            } else {
                None
            };

            let evidence_dir = vault_path.join("evidence");
            if !evidence_dir.exists() {
                std::fs::create_dir_all(&evidence_dir)
                    .map_err(|e| format!("Failed to create evidence directory: {}", e))?;
            }

            let uuid_str = uuid::Uuid::new_v4().to_string();
            let id = format!("EVI-{}", &uuid_str[..8].to_uppercase());

            let new_record = EvidenceRecord {
                id: id.clone(),
                kind: e_kind,
                title: e_title,
                source: "cli".to_string(),
                path: e_path,
                url: e_url,
                artifact_sha: e_sha,
                commit: Some(
                    get_current_git_commit_ctx(project_root)
                        .unwrap_or_else(|| "unknown".to_string()),
                ),
                environment: Some(e_env.unwrap_or_else(|| "production".to_string())),
                release: Some(e_release.unwrap_or(release_ver)),
                created_at: chrono::Local::now().to_rfc3339(),
                expires_at: e_expires_at,
                metadata: e_mobile_kind.map(|kind| {
                    let mut metadata = serde_json::json!({
                        "mobile_evidence_kind": kind,
                        "schema_version": "1.0"
                    });
                    if let Some(issuer) = &e_issuer {
                        metadata["issuer"] = serde_json::Value::String(issuer.clone());
                    }
                    metadata
                }),
                attachments: None,
            };

            let evidence_file_path = evidence_dir.join(format!("{}.json", id));
            let serialized = serde_json::to_string_pretty(&new_record)
                .map_err(|e| format!("Failed to serialize evidence record: {e}"))?;
            std::fs::write(&evidence_file_path, serialized)
                .map_err(|e| format!("Failed to write evidence record: {}", e))?;

            output.print_stdout(&format!("Successfully added evidence record '{}'.", id));
            Ok(0)
        }
    }
}
