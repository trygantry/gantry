use crate::output::Output;
use crate::prompter::Prompter;
use std::path::Path;

pub enum WaiverCommand {
    List {
        json: bool,
    },
    Add {
        target_type: Option<String>,
        target_id: Option<String>,
        reason: Option<String>,
        approved_by: Option<String>,
        expires_at: Option<String>,
        target_path: Option<String>,
        target_code: Option<String>,
        interactive: bool,
    },
    Expire {
        id: String,
    },
}

pub fn handle(
    output: &Output,
    prompter: &dyn Prompter,
    command: WaiverCommand,
    vault_path: &Path,
) -> Result<i32, String> {
    let waivers_path = vault_path.join("waivers.json");

    let mut waivers: Vec<gantry_core::Waiver> = Vec::new();
    if waivers_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&waivers_path) {
            if let Ok(w_list) = serde_json::from_str::<Vec<gantry_core::Waiver>>(&content) {
                waivers = w_list;
            }
        }
    }

    match command {
        WaiverCommand::List { json } => {
            if json || output.json {
                output.print_json(&waivers);
            } else {
                output.print_stdout("=====================================");
                output.print_stdout("        Gantry Active Waivers        ");
                output.print_stdout("=====================================");
                let active_waivers: Vec<&gantry_core::Waiver> =
                    waivers.iter().filter(|w| w.status == "active").collect();

                if active_waivers.is_empty() {
                    output.print_stdout("No active risk waivers configured.");
                } else {
                    for w in active_waivers {
                        output.print_stdout(&format!(
                            "- [{}] ({}): {}",
                            w.id, w.target_type, w.reason
                        ));
                        if let Some(ref tid) = w.target_id {
                            output.print_stdout(&format!("  Target ID:   {}", tid));
                        }
                        if let Some(ref tpath) = w.target_path {
                            output.print_stdout(&format!("  Target Path: {}", tpath));
                        }
                        if let Some(ref tcode) = w.target_code {
                            output.print_stdout(&format!("  Target Code: {}", tcode));
                        }
                        output.print_stdout(&format!(
                            "  Approved By: {} | Expires: {} | Status: {}",
                            w.approved_by, w.expires_at, w.status
                        ));
                        output.print_stdout("-------------------------------------");
                    }
                }
            }
            Ok(0)
        }
        WaiverCommand::Add {
            target_type,
            target_id,
            reason,
            approved_by,
            expires_at,
            target_path,
            target_code,
            interactive,
        } => {
            let t_type = if let Some(t) = target_type {
                t
            } else if interactive {
                let choices = &[
                    "blocker",
                    "qa-suite",
                    "policy-rule",
                    "validation-error",
                    "other",
                ];
                let idx = prompter.select("Select target type", choices, 0)?;
                if choices[idx] == "other" {
                    prompter.input("Enter custom target type", None)?
                } else {
                    choices[idx].to_string()
                }
            } else {
                return Err("Missing required argument: --target-type".to_string());
            };

            let r_reason = if let Some(r) = reason {
                r
            } else if interactive {
                prompter.input("Enter reason for waiver", None)?
            } else {
                return Err("Missing required argument: --reason".to_string());
            };

            let a_by = if let Some(a) = approved_by {
                a
            } else if interactive {
                prompter.input("Approved by (name)", None)?
            } else {
                return Err("Missing required argument: --approved-by".to_string());
            };

            let e_at = if let Some(e) = expires_at {
                if chrono::NaiveDate::parse_from_str(&e, "%Y-%m-%d").is_err() {
                    return Err(
                        "Invalid date format for --expires-at. Expected YYYY-MM-DD.".to_string()
                    );
                }
                e
            } else if interactive {
                loop {
                    let input = prompter.input("Enter expiration date (YYYY-MM-DD)", None)?;
                    if chrono::NaiveDate::parse_from_str(&input, "%Y-%m-%d").is_ok() {
                        break input;
                    }
                    output.print_stderr("Invalid date format. Please use YYYY-MM-DD.");
                }
            } else {
                return Err("Missing required argument: --expires-at".to_string());
            };

            let t_id = if target_id.is_some() {
                target_id
            } else if interactive {
                let val =
                    prompter.input("Enter target ID (optional, press Enter to skip)", Some(""))?;
                if val.trim().is_empty() {
                    None
                } else {
                    Some(val)
                }
            } else {
                None
            };

            let t_path = if target_path.is_some() {
                target_path
            } else if interactive {
                let val = prompter.input(
                    "Enter target path (optional, press Enter to skip)",
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

            let t_code = if target_code.is_some() {
                target_code
            } else if interactive {
                let val = prompter.input(
                    "Enter target code (optional, press Enter to skip)",
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

            let uuid_str = uuid::Uuid::new_v4().to_string();
            let new_id = format!("WAV-{}", &uuid_str[..8].to_uppercase());
            let new_waiver = gantry_core::Waiver {
                id: new_id.clone(),
                target_type: t_type,
                target_id: t_id,
                target_path: t_path,
                target_code: t_code,
                reason: r_reason,
                approved_by: a_by,
                created_at: chrono::Local::now().format("%Y-%m-%d").to_string(),
                expires_at: e_at,
                status: "active".to_string(),
            };

            waivers.push(new_waiver);
            let serialized = serde_json::to_string_pretty(&waivers)
                .map_err(|e| format!("Failed to serialize waivers: {e}"))?;
            std::fs::write(&waivers_path, serialized)
                .map_err(|e| format!("Failed to save waivers.json: {}", e))?;

            output.print_stdout(&format!("Successfully added active waiver '{}'.", new_id));
            Ok(0)
        }
        WaiverCommand::Expire { id } => {
            let mut found = false;
            for w in &mut waivers {
                if w.id == id {
                    w.status = "expired".to_string();
                    found = true;
                }
            }

            if !found {
                return Err(format!("Error: Waiver '{}' not found.", id));
            }

            let serialized = serde_json::to_string_pretty(&waivers)
                .map_err(|e| format!("Failed to serialize waivers: {e}"))?;
            std::fs::write(&waivers_path, serialized)
                .map_err(|e| format!("Failed to update waivers.json: {}", e))?;

            output.print_stdout(&format!("Successfully expired waiver '{}'.", id));
            Ok(0)
        }
    }
}
