use crate::output::Output;
use gantry_core::compile_trends_ctx;
use std::path::Path;

pub fn handle(output: &Output, json_mode: bool, vault_path: &Path) -> Result<i32, String> {
    match compile_trends_ctx(vault_path) {
        Ok(trends) => {
            if json_mode || output.json {
                output.print_json(&trends);
            } else {
                output.print_stdout("=====================================");
                output.print_stdout("        Gantry Readiness Trends      ");
                output.print_stdout("=====================================");
                if trends.dates.is_empty() {
                    output.print_stdout("No historical snapshots to show trends.");
                } else {
                    for i in 0..trends.dates.len() {
                        output.print_stdout(&format!(
                            "Snapshot: {} | Score: {}/100 | Readiness: {} | Blockers: {} | QA: {:.1}% | Waivers: {} | Evidence: {}",
                            trends.dates[i],
                            trends.scores[i],
                            trends.readiness_states[i],
                            trends.blockers[i],
                            trends.qa_completion[i],
                            trends.waivers[i],
                            trends.evidence[i],
                        ));
                    }
                }
                output.print_stdout("=====================================");
            }
            Ok(0)
        }
        Err(e) => Err(format!("Failed to compile trends: {}", e)),
    }
}
