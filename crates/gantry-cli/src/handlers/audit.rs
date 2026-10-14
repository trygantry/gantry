use crate::output::Output;
use gantry_core::{
    EvaluationOptions, export_audit_certificate, generate_markdown_report, run_launch_audit_ctx,
};
use std::path::PathBuf;

fn color_readiness(output: &Output, s: &str) -> String {
    match s {
        "Go" => output.style("Go", Some("green"), true),
        "At Risk" => output.style("At Risk", Some("yellow"), true),
        _ => output.style("No-Go", Some("red"), true),
    }
}

pub fn handle_export(
    output: &Output,
    format_arg: Option<String>,
    output_path: Option<PathBuf>,
    opts: &EvaluationOptions,
) -> Result<i32, String> {
    let format_type = format_arg.unwrap_or_else(|| "html".to_string());
    match run_launch_audit_ctx(opts, false) {
        Ok(payload) => {
            let cert = export_audit_certificate(&payload, &format_type);
            match output_path {
                Some(path) => {
                    std::fs::write(&path, &cert).map_err(|e| {
                        format!("Failed to write certificate to {}: {}", path.display(), e)
                    })?;
                    output
                        .print_stdout(&format!("Audit certificate written to {}", path.display()));
                }
                None => {
                    output.print_stdout(&cert);
                }
            }
            Ok(0)
        }
        Err(e) => Err(format!("Audit failed: {}", e)),
    }
}

pub fn handle_audit(
    output: &Output,
    dry_run: bool,
    json_mode: bool,
    report_url: Option<String>,
    format_arg: Option<String>,
    output_path: Option<PathBuf>,
    opts: &EvaluationOptions,
) -> Result<i32, String> {
    let write = !dry_run;
    gantry_core::record_telemetry_event_ctx(&opts.vault.vault_path, "audit_run");

    let final_format = if json_mode {
        "json".to_string()
    } else {
        format_arg.unwrap_or_else(|| "text".to_string())
    };

    match run_launch_audit_ctx(opts, write) {
        Ok(payload) => {
            let final_report_url = report_url.or_else(|| payload.report_url.clone());

            if final_format == "json" {
                let json = serde_json::to_string_pretty(&payload)
                    .map_err(|e| format!("Failed to serialize audit JSON: {}", e))?;
                if let Some(path) = output_path {
                    std::fs::write(&path, &json).map_err(|e| {
                        format!("Failed to write JSON report to {}: {}", path.display(), e)
                    })?;
                } else if output.json {
                    output.print_json(&payload);
                } else {
                    output.print_stdout(&json);
                }
            } else if final_format == "markdown" {
                let markdown_report = generate_markdown_report(&payload);
                if let Some(path) = output_path {
                    std::fs::write(&path, &markdown_report).map_err(|e| {
                        format!(
                            "Failed to write markdown report to {}: {}",
                            path.display(),
                            e
                        )
                    })?;
                } else {
                    output.print_stdout(&markdown_report);
                }
            } else {
                if let Some(ref url) = final_report_url {
                    output.print_stdout(&format!("[Gantry Cloud Hook] Report will be dispatched to: {} (Enterprise gate enabled)", url));
                }

                output.print_stdout("=====================================");
                output.print_stdout("        Gantry Launch Audit         ");
                output.print_stdout("=====================================");
                output.print_stdout(&format!("Vault Name:      {}", payload.vault_name));
                output.print_stdout(&format!("Release Target:  {}", payload.release));
                output.print_stdout(&format!(
                    "Readiness State: {}",
                    color_readiness(output, &payload.readiness)
                ));
                output.print_stdout(&format!("Readiness Score: {}/100", payload.readiness_score));
                output.print_stdout("-------------------------------------");
                output.print_stdout("Category Breakdown:");
                let mut cats: Vec<(&String, &u8)> = payload.category_scores.iter().collect();
                cats.sort_by(|a, b| a.0.cmp(b.0));
                for (cat, score) in cats {
                    output.print_stdout(&format!("  - {:<12} : {}/100", cat, score));
                }
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

                if !payload.readiness_reasons.is_empty() {
                    output.print_stdout("\nReadiness Reasons:");
                    for reason in &payload.readiness_reasons {
                        output.print_stdout(&format!("  - {}", reason));
                    }
                }

                if !payload.score_reasons.is_empty() {
                    output.print_stdout("\nScore Deductions & Explanations:");
                    let mut cats: Vec<(&String, &Vec<String>)> =
                        payload.score_reasons.iter().collect();
                    cats.sort_by(|a, b| a.0.cmp(b.0));
                    for (cat, reasons) in cats {
                        if cat == "Cap" {
                            continue;
                        }
                        output.print_stdout(&format!("  [Category: {}]", cat));
                        for reason in reasons {
                            output.print_stdout(&format!("    - {}", reason));
                        }
                    }
                }

                if let Some(caps) = payload.score_reasons.get("Cap") {
                    if !caps.is_empty() {
                        output.print_stdout("\n[CAPPING ALERT]");
                        for cap in caps {
                            output.print_stdout(&format!("  ⚠️  {}", cap));
                        }
                    }
                }

                output.print_stdout("=====================================");
                if !write {
                    output
                        .print_stdout("* RUNNING IN DRY-RUN PREVIEW MODE. No files were written.");
                }
            }
            Ok(0)
        }
        Err(e) => Err(format!("Launch Audit failed: {}", e)),
    }
}
