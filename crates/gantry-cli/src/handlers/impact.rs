use crate::output::Output;
use gantry_core::{VaultContext, get_impact_summary_ctx};

pub fn handle(
    output: &Output,
    json_mode: bool,
    markdown_mode: bool,
    base_ref: Option<String>,
    vault: &VaultContext,
) -> Result<i32, String> {
    match get_impact_summary_ctx(vault, base_ref.as_deref()) {
        Ok(summary) => {
            if json_mode || output.json {
                output.print_json(&summary);
            } else if markdown_mode {
                let mut md = String::new();
                md.push_str("## Change Impact\n");
                md.push('\n');
                md.push_str("| | Count |\n");
                md.push_str("| :--- | ---: |\n");
                md.push_str(&format!(
                    "| Files changed | {} |\n",
                    summary.changed_files.len()
                ));
                md.push_str(&format!(
                    "| Blockers affected | {} |\n",
                    summary.impacted_blockers.len()
                ));
                md.push_str(&format!(
                    "| QA suites affected | {} |\n",
                    summary.impacted_qa.len()
                ));
                md.push_str(&format!(
                    "| Uncovered folders | {} |\n",
                    summary.uncovered_changes.len()
                ));
                if !summary.impacted_blockers.is_empty() {
                    md.push('\n');
                    md.push_str(&format!(
                        "**Affected blockers:** {}\n",
                        summary.impacted_blockers.join(", ")
                    ));
                }
                if !summary.impacted_qa.is_empty() {
                    md.push('\n');
                    md.push_str(&format!(
                        "**Affected QA suites:** {}\n",
                        summary.impacted_qa.join(", ")
                    ));
                }
                if !summary.suggested_tasks.is_empty() {
                    md.push('\n');
                    md.push_str("**Suggested tasks:**\n");
                    for t in &summary.suggested_tasks {
                        md.push_str(&format!("- {}\n", t));
                    }
                }
                output.print_stdout(md.trim_end());
            } else {
                output.print_stdout("=====================================");
                output.print_stdout("        Gantry Change Impact         ");
                output.print_stdout("=====================================");
                output.print_stdout(&format!(
                    "Changed Files:      {}",
                    summary.changed_files.len()
                ));
                for file in &summary.changed_files {
                    output.print_stdout(&format!("  - {}", file));
                }
                output.print_stdout("-------------------------------------");
                output.print_stdout(&format!(
                    "Impacted Blockers:  {}",
                    summary.impacted_blockers.len()
                ));
                for b in &summary.impacted_blockers {
                    output.print_stdout(&format!("  - {}", b));
                }
                output.print_stdout(&format!(
                    "Impacted QA Suites: {}",
                    summary.impacted_qa.len()
                ));
                for q in &summary.impacted_qa {
                    output.print_stdout(&format!("  - {}", q));
                }
                output.print_stdout(&format!(
                    "Uncovered Folders:  {}",
                    summary.uncovered_changes.len()
                ));
                for u in &summary.uncovered_changes {
                    output.print_stdout(&format!("  - {}", u));
                }
                output.print_stdout("-------------------------------------");
                output.print_stdout("Suggested Verification Tasks:");
                for t in &summary.suggested_tasks {
                    output.print_stdout(&format!("  - {}", t));
                }
                output.print_stdout("=====================================");
            }
            Ok(0)
        }
        Err(e) => Err(format!("Failed to calculate change impact: {}", e)),
    }
}
