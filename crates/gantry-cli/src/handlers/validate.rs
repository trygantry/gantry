use crate::output::Output;
use gantry_core::{EvaluationOptions, compile_launch_index_ctx};

fn color_readiness(output: &Output, s: &str) -> String {
    match s {
        "Go" => output.style("Go", Some("green"), true),
        "At Risk" => output.style("At Risk", Some("yellow"), true),
        _ => output.style("No-Go", Some("red"), true),
    }
}

pub fn handle(
    output: &Output,
    ci: bool,
    report_url: Option<String>,
    policy: Option<String>,
    mut opts: EvaluationOptions,
) -> Result<i32, String> {
    gantry_core::record_telemetry_event_ctx(
        &opts.vault.vault_path,
        if ci { "ci_run" } else { "validate_run" },
    );

    if let Some(source) = policy {
        let policy_json = if source.starts_with("http://") || source.starts_with("https://") {
            let config = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(10)))
                .build();
            let agent: ureq::Agent = config.into();
            let mut resp = agent
                .get(&source)
                .call()
                .map_err(|e| format!("Failed to fetch policy from {}: {}", source, e))?;
            resp.body_mut()
                .read_to_string()
                .map_err(|e| format!("Failed to read policy response body: {}", e))?
        } else {
            std::fs::read_to_string(&source)
                .map_err(|e| format!("Failed to read policy file {}: {}", source, e))?
        };

        // Validate it parses as a scorecard rule array.
        if serde_json::from_str::<serde_json::Value>(&policy_json).is_err() {
            return Err("Policy is not valid JSON.".to_string());
        }

        // Cache to <vault_path>/.policy_cache.json
        let cache_path = opts.vault.vault_path.join(".policy_cache.json");
        std::fs::write(&cache_path, &policy_json)
            .map_err(|e| format!("Failed to cache policy: {}", e))?;
        opts.policy_overlay = Some(cache_path);
        if ci {
            output.print_stdout(&format!("Policy loaded from: {}", source));
        }
    }

    match compile_launch_index_ctx(&opts) {
        Ok(payload) => {
            let final_report_url = report_url.or_else(|| payload.report_url.clone());
            if let Some(ref url) = final_report_url {
                output.print_stdout(&format!("[Gantry Cloud Hook] Report will be dispatched to: {} (Enterprise gate enabled)", url));
            }

            if output.json {
                output.print_json(&payload);
                if payload.readiness == "Go" {
                    return Ok(0);
                } else {
                    return Ok(1);
                }
            }

            if ci {
                output.print_stdout("=====================================");
                output.print_stdout("        Gantry CI Validation         ");
                output.print_stdout("=====================================");
                output.print_stdout(&format!("Vault Name:      {}", payload.vault_name));
                output.print_stdout(&format!("Release Target:  {}", payload.release));
                output.print_stdout(&format!(
                    "Readiness State: {}",
                    color_readiness(output, &payload.readiness)
                ));
                output.print_stdout(&format!("Readiness Score: {}/100", payload.readiness_score));
                output.print_stdout("-------------------------------------");
                output.print_stdout(&format!("Open Blockers:   {}", payload.open_blockers));
                output.print_stdout(&format!(
                    "QA Completion:   {}/{} tests passed",
                    payload.qa_completed, payload.qa_total
                ));
                output.print_stdout(&format!(
                    "Validation Errors: {}",
                    payload.validation_errors.len()
                ));
                output.print_stdout(&format!(
                    "Broken Wiki Links: {}",
                    payload.broken_links.len()
                ));
                output.print_stdout(&format!(
                    "Active Waivers:     {}",
                    payload.active_waivers.len()
                ));

                if !payload.scorecard_results.is_empty() {
                    output.print_stdout("-------------------------------------");
                    output.print_stdout("Scorecard Evaluation Results:");
                    for res in &payload.scorecard_results {
                        let status = if res.waived {
                            output.style("[WAIVED]", Some("yellow"), true)
                        } else if res.passed {
                            output.style("[PASSED]", Some("green"), true)
                        } else {
                            output.style("[FAILED]", Some("red"), true)
                        };
                        output.print_stdout(&format!(
                            "  - {} {}: (Score impact weight: {})",
                            status, res.title, res.weight
                        ));
                        if !res.passed && !res.waived {
                            if let Some(ref fm) = res.failure_message {
                                output.print_stdout(&format!("    Reason: {}", fm));
                            }
                        }
                    }
                }

                // Top 5 Actionable Blockers
                output.print_stdout("-------------------------------------");
                output.print_stdout("Top Actionable Blockers:");
                let mut printed = 0;
                for blocker in &payload.blockers {
                    if blocker.status.to_lowercase() != "resolved" {
                        let waived_str = if blocker.waived {
                            format!(" {}", output.style("[WAIVED]", Some("yellow"), true))
                        } else {
                            "".to_string()
                        };
                        output.print_stdout(&format!(
                            "  - {}{} {}: {} ({})",
                            output.style("[BLOCKER]", Some("red"), true),
                            waived_str,
                            blocker.id,
                            blocker.title,
                            blocker.owner
                        ));
                        printed += 1;
                        if printed >= 5 {
                            break;
                        }
                    }
                }
                if printed < 5 {
                    for gate in &payload.go_no_go_gates {
                        if gate.status != "checked" {
                            let waived_str = if gate.waived {
                                format!(" {}", output.style("[WAIVED]", Some("yellow"), true))
                            } else {
                                "".to_string()
                            };
                            output.print_stdout(&format!(
                                "  - {}{} {}: {} ({})",
                                output.style("[GATE]", Some("yellow"), true),
                                waived_str,
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
                        if signoff.status != "Approved" {
                            let waived_str = if signoff.waived {
                                format!(" {}", output.style("[WAIVED]", Some("yellow"), true))
                            } else {
                                "".to_string()
                            };
                            output.print_stdout(&format!(
                                "  - {}{} {}: {} ({})",
                                output.style("[SIGNOFF]", Some("cyan"), true),
                                waived_str,
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
                        let waived_str = if err.waived {
                            format!(" {}", output.style("[WAIVED]", Some("yellow"), true))
                        } else {
                            "".to_string()
                        };
                        output.print_stdout(&format!(
                            "  - {}{} {}: {}",
                            output.style("[ERROR]", Some("red"), true),
                            waived_str,
                            err.file,
                            err.message
                        ));
                        printed += 1;
                        if printed >= 5 {
                            break;
                        }
                    }
                }
                if printed == 0 {
                    output.print_stdout("  All checks nominal. Clear for launch!");
                }

                output.print_stdout("-------------------------------------");
                output.print_stdout("Next Actions:");
                if payload.readiness == "Go" {
                    output.print_stdout("  No actions needed. Ready to release.");
                } else {
                    output.print_stdout("  1. Resolve open blockers listed above.");
                    output.print_stdout("  2. Coordinate with pending sign-off stakeholders.");
                    output
                        .print_stdout("  3. Run `gantry audit` to check detailed evidence graph.");
                }
                output.print_stdout("=====================================");

                if payload.readiness == "Go" {
                    Ok(0)
                } else {
                    Ok(1)
                }
            } else {
                output.print_stdout("=====================================");
                output.print_stdout("      Gantry Verification Status     ");
                output.print_stdout("=====================================");
                output.print_stdout(&format!(
                    "Readiness State: {}",
                    color_readiness(output, &payload.readiness)
                ));
                output.print_stdout(&format!("Readiness Score: {}/100", payload.readiness_score));
                output.print_stdout(&format!("Open Blockers:   {}", payload.open_blockers));
                output.print_stdout(&format!(
                    "Active Waivers:  {}",
                    payload.active_waivers.len()
                ));

                if !payload.scorecard_results.is_empty() {
                    output.print_stdout("-------------------------------------");
                    output.print_stdout("Scorecard Evaluation Results:");
                    for res in &payload.scorecard_results {
                        let status = if res.waived {
                            output.style("[WAIVED]", Some("yellow"), true)
                        } else if res.passed {
                            output.style("[PASSED]", Some("green"), true)
                        } else {
                            output.style("[FAILED]", Some("red"), true)
                        };
                        output.print_stdout(&format!(
                            "  - {} {}: (Score impact weight: {})",
                            status, res.title, res.weight
                        ));
                    }
                }

                // Top 5 Actionable Blockers
                output.print_stdout("-------------------------------------");
                output.print_stdout("Top Actionable Blockers:");
                let mut printed = 0;
                for blocker in &payload.blockers {
                    if blocker.status.to_lowercase() != "resolved" {
                        let waived_str = if blocker.waived {
                            format!(" {}", output.style("[WAIVED]", Some("yellow"), true))
                        } else {
                            "".to_string()
                        };
                        output.print_stdout(&format!(
                            "  - {}{} {}: {} ({})",
                            output.style("[BLOCKER]", Some("red"), true),
                            waived_str,
                            blocker.id,
                            blocker.title,
                            blocker.owner
                        ));
                        printed += 1;
                        if printed >= 5 {
                            break;
                        }
                    }
                }
                if printed < 5 {
                    for gate in &payload.go_no_go_gates {
                        if gate.status != "checked" {
                            let waived_str = if gate.waived {
                                format!(" {}", output.style("[WAIVED]", Some("yellow"), true))
                            } else {
                                "".to_string()
                            };
                            output.print_stdout(&format!(
                                "  - {}{} {}: {} ({})",
                                output.style("[GATE]", Some("yellow"), true),
                                waived_str,
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
                        if signoff.status != "Approved" {
                            let waived_str = if signoff.waived {
                                format!(" {}", output.style("[WAIVED]", Some("yellow"), true))
                            } else {
                                "".to_string()
                            };
                            output.print_stdout(&format!(
                                "  - {}{} {}: {} ({})",
                                output.style("[SIGNOFF]", Some("cyan"), true),
                                waived_str,
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
                        let waived_str = if err.waived {
                            format!(" {}", output.style("[WAIVED]", Some("yellow"), true))
                        } else {
                            "".to_string()
                        };
                        output.print_stdout(&format!(
                            "  - {}{} {}: {}",
                            output.style("[ERROR]", Some("red"), true),
                            waived_str,
                            err.file,
                            err.message
                        ));
                        printed += 1;
                        if printed >= 5 {
                            break;
                        }
                    }
                }
                if printed == 0 {
                    output.print_stdout("  All checks nominal. Clear for launch!");
                }

                output.print_stdout("-------------------------------------");
                output.print_stdout("Next Actions:");
                if payload.readiness == "Go" {
                    output.print_stdout("  No actions needed. Ready to release.");
                } else {
                    output.print_stdout("  1. Resolve open blockers listed above.");
                    output.print_stdout("  2. Coordinate with pending sign-off stakeholders.");
                    output
                        .print_stdout("  3. Run `gantry audit` to check detailed evidence graph.");
                }
                output.print_stdout("=====================================");

                if payload.readiness == "Go" {
                    output.print_stdout("\nLaunch checks verified successfully!");
                    Ok(0)
                } else {
                    output
                        .print_stdout("\nLaunch is blocked. Please resolve blockers or sign-offs.");
                    Ok(1)
                }
            }
        }
        Err(e) => Err(format!("Validation process failed: {}", e)),
    }
}
