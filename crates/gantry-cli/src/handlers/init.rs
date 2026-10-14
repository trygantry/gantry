use crate::output::Output;
use crate::prompter::Prompter;
use gantry_core::{VaultContext, init_vault_auto_ctx, init_vault_with_template_ctx};

#[allow(clippy::too_many_arguments)]
pub fn handle(
    output: &Output,
    prompter: &dyn Prompter,
    force: bool,
    auto: bool,
    dry_run: bool,
    json: bool,
    template: Option<String>,
    interactive: bool,
    mobile: bool,
    vault: &VaultContext,
) -> Result<i32, String> {
    let mut force = force;
    if vault.vault_path.exists() && !force && interactive && !dry_run {
        let mut has_files = false;
        if let Ok(entries) = std::fs::read_dir(&vault.vault_path) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name != "." && name != ".." {
                    has_files = true;
                    break;
                }
            }
        }
        if has_files {
            if prompter.confirm(
                "Gantry vault already initialized. Overwrite existing configuration?",
                false,
            )? {
                force = true;
            } else {
                return Err("Initialization cancelled by user.".to_string());
            }
        }
    }

    if auto {
        match init_vault_auto_ctx(&vault.vault_path, &vault.project_root, dry_run, force) {
            Ok(report) => {
                if json {
                    if report.dry_run {
                        output.print_json(&serde_json::json!({
                            "detected_project_type": report.detected_project_type,
                            "vault_path": report.vault_path,
                            "files_to_create": report.files_to_create,
                        }));
                    } else {
                        output.print_json(&serde_json::json!({
                            "status": "success",
                            "detected_project_type": report.detected_project_type,
                            "vault_path": report.vault_path,
                        }));
                    }
                } else if report.dry_run {
                    output.print_stdout(&format!(
                        "Dry-run mode: Detected project type is '{}'.",
                        report.detected_project_type
                    ));
                    output.print_stdout(&format!(
                        "The following files would be created in {}:",
                        report.vault_path
                    ));
                    output.print_stdout("  - config.json");
                    output.print_stdout("  - go_no_go.md");
                    output.print_stdout("  - tasks.md");
                    output.print_stdout("  - AGENTS.md");
                    output.print_stdout("  - qa/regression.md");
                    output.print_stdout("  - blockers/BLK-101-purchase-crash.md");
                } else {
                    output.print_stdout(&format!(
                        "Successfully initialized Gantry vault with detected project template '{}'.",
                        report.detected_project_type
                    ));
                }
                Ok(0)
            }
            Err(e) => Err(format!("Auto-initialization failed: {}", e)),
        }
    } else {
        let template_type = if let Some(t) = template {
            t
        } else if interactive {
            let choices = &[
                "mobile-app",
                "saas-app",
                "ai-app",
                "backend-api",
                "desktop-app",
                "chrome-extension",
            ];
            let idx = prompter.select("Select Gantry template type", choices, 0)?;
            choices[idx].to_string()
        } else {
            "mobile-app".to_string()
        };

        if dry_run {
            let mut files_to_create = vec![
                format!("{}/config.json", vault.vault_path.to_string_lossy()),
                format!("{}/go_no_go.md", vault.vault_path.to_string_lossy()),
                format!("{}/tasks.md", vault.vault_path.to_string_lossy()),
                format!("{}/AGENTS.md", vault.vault_path.to_string_lossy()),
                format!("{}/qa/regression.md", vault.vault_path.to_string_lossy()),
                format!(
                    "{}/blockers/BLK-101-purchase-crash.md",
                    vault.vault_path.to_string_lossy()
                ),
            ];
            if mobile {
                files_to_create.push(
                    vault
                        .vault_path
                        .join("mobile-policy.json")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            if json {
                let response = serde_json::json!({
                    "template": template_type,
                    "vault_path": vault.vault_path.to_string_lossy(),
                    "files_to_create": files_to_create
                });
                output.print_json(&response);
            } else {
                output.print_stdout(&format!(
                    "Dry-run mode: Selected template is '{}'.",
                    template_type
                ));
                output.print_stdout(&format!(
                    "The following files would be created in {}:",
                    vault.vault_path.display()
                ));
                output.print_stdout("  - config.json");
                output.print_stdout("  - go_no_go.md");
                output.print_stdout("  - tasks.md");
                output.print_stdout("  - AGENTS.md");
                output.print_stdout("  - qa/regression.md");
                output.print_stdout("  - blockers/BLK-101-purchase-crash.md");
                if mobile {
                    output.print_stdout("  - mobile-policy.json");
                }
            }
            return Ok(0);
        }

        match init_vault_with_template_ctx(&vault.vault_path, &template_type, force) {
            Ok(_) => {
                let mobile_policy_path = if mobile {
                    Some(gantry_core::mobile::init_mobile_policy(
                        &vault.project_root,
                        force,
                    )?)
                } else {
                    None
                };
                if json {
                    let response = serde_json::json!({
                        "status": "success",
                        "template": template_type,
                        "vault_path": vault.vault_path.to_string_lossy(),
                        "mobile_policy_path": mobile_policy_path
                    });
                    output.print_json(&response);
                } else {
                    output.print_stdout(&format!(
                        "Gantry initial template '{}' created successfully in .gantry/",
                        template_type
                    ));
                    if let Some(path) = mobile_policy_path {
                        output.print_stdout(&format!(
                            "Mobile release policy created at {}",
                            path.display()
                        ));
                    }
                }
                gantry_core::record_telemetry_event_ctx(&vault.vault_path, "vault_initialized");
                Ok(0)
            }
            Err(e) => Err(format!("Initialization failed: {}", e)),
        }
    }
}
