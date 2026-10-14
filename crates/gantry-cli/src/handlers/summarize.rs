use crate::output::Output;
use gantry_core::{EvaluationOptions, compile_launch_index_ctx};

fn color_readiness(output: &Output, s: &str) -> String {
    match s {
        "Go" => output.style("Go", Some("green"), true),
        "At Risk" => output.style("At Risk", Some("yellow"), true),
        _ => output.style("No-Go", Some("red"), true),
    }
}

pub fn handle(output: &Output, opts: &EvaluationOptions) -> Result<i32, String> {
    match compile_launch_index_ctx(opts) {
        Ok(payload) => {
            if output.json {
                output.print_json(&payload);
                return Ok(0);
            }

            output.print_stdout("=====================================");
            output.print_stdout(" Gantry Launch Control Summary ");
            output.print_stdout("=====================================");
            output.print_stdout(&format!("Vault Name:      {}", payload.vault_name));
            output.print_stdout(&format!("Release Target:  {}", payload.release));
            output.print_stdout(&format!(
                "Readiness State: {}",
                color_readiness(output, &payload.readiness)
            ));
            output.print_stdout("-------------------------------------");
            output.print_stdout(&format!("Open Blockers:   {}", payload.open_blockers));
            output.print_stdout(&format!(
                "QA Completion:   {}/{} tests passed",
                payload.qa_completed, payload.qa_total
            ));

            if !payload.readiness_reasons.is_empty() {
                output.print_stdout("\nReadiness Reasons:");
                for reason in &payload.readiness_reasons {
                    output.print_stdout(&format!("- {}", reason));
                }
            }

            if !payload.validation_errors.is_empty() {
                output.print_stdout(&format!(
                    "\nValidation Issues: {}",
                    payload.validation_errors.len()
                ));
            }
            if !payload.broken_links.is_empty() {
                output.print_stdout(&format!(
                    "Broken Wiki Links: {}",
                    payload.broken_links.len()
                ));
            }
            output.print_stdout("=====================================");
            Ok(0)
        }
        Err(e) => Err(format!("Summary compilation failed: {}", e)),
    }
}
