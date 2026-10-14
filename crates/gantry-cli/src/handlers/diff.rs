use crate::output::Output;
use gantry_core::{diff_snapshots, load_snapshot_by_commit_ctx};
use std::path::Path;

fn color_readiness(output: &Output, s: &str) -> String {
    match s {
        "Go" => output.style("Go", Some("green"), true),
        "At Risk" => output.style("At Risk", Some("yellow"), true),
        _ => output.style("No-Go", Some("red"), true),
    }
}

pub fn handle(
    output: &Output,
    spec: String,
    json_mode: bool,
    vault_path: &Path,
) -> Result<i32, String> {
    let parts: Vec<&str> = spec.split("..").collect();
    if parts.len() != 2 {
        return Err(
            "Invalid ref spec format. Expected <base>..<head> (e.g. v1.0.0..HEAD)".to_string(),
        );
    }

    let base_ref = parts[0];
    let head_ref = parts[1];

    let base_snap = load_snapshot_by_commit_ctx(vault_path, base_ref)
        .map_err(|e| format!("Failed to load base snapshot '{}': {}", base_ref, e))?;
    let head_snap = load_snapshot_by_commit_ctx(vault_path, head_ref)
        .map_err(|e| format!("Failed to load head snapshot '{}': {}", head_ref, e))?;

    let diff = diff_snapshots(&base_snap, &head_snap);

    if json_mode || output.json {
        output.print_json(&diff);
    } else {
        output.print_stdout(&format!("Base Commit:      {}", diff.base_commit));
        output.print_stdout(&format!("Head Commit:      {}", diff.head_commit));
        output.print_stdout("-------------------------------------");

        if let Some((ref b, ref h)) = diff.readiness_changed {
            output.print_stdout(&format!(
                "Readiness Changed: {} -> {}",
                color_readiness(output, b),
                color_readiness(output, h)
            ));
        }

        if let Some((b, h)) = diff.score_changed {
            output.print_stdout(&format!("Score Changed:     {} -> {}", b, h));
        }

        if !diff.added_blockers.is_empty() {
            output.print_stdout(&format!("Added Blockers ({}):", diff.added_blockers.len()));
            for id in &diff.added_blockers {
                output.print_stdout(&format!("  - {}", id));
            }
        }

        if !diff.resolved_blockers.is_empty() {
            output.print_stdout(&format!(
                "Resolved Blockers ({}):",
                diff.resolved_blockers.len()
            ));
            for id in &diff.resolved_blockers {
                output.print_stdout(&format!("  - {}", id));
            }
        }

        if !diff.completed_qa_suites.is_empty() {
            output.print_stdout(&format!(
                "Completed QA Suites ({}):",
                diff.completed_qa_suites.len()
            ));
            for id in &diff.completed_qa_suites {
                output.print_stdout(&format!("  - {}", id));
            }
        }

        if !diff.completed_gates.is_empty() {
            output.print_stdout(&format!(
                "Completed Gates ({}):",
                diff.completed_gates.len()
            ));
            for name in &diff.completed_gates {
                output.print_stdout(&format!("  - {}", name));
            }
        }

        if !diff.completed_signoffs.is_empty() {
            output.print_stdout(&format!(
                "Completed Signoffs ({}):",
                diff.completed_signoffs.len()
            ));
            for role in &diff.completed_signoffs {
                output.print_stdout(&format!("  - {}", role));
            }
        }

        if diff.added_blockers.is_empty()
            && diff.resolved_blockers.is_empty()
            && diff.completed_qa_suites.is_empty()
            && diff.completed_gates.is_empty()
            && diff.completed_signoffs.is_empty()
            && diff.readiness_changed.is_none()
            && diff.score_changed.is_none()
        {
            output.print_stdout("No structural differences detected.");
        }
    }

    Ok(0)
}
