use crate::output::Output;
use gantry_core::verify_chain_ctx;
use std::path::Path;

pub fn handle(output: &Output, json_mode: bool, vault_path: &Path) -> Result<i32, String> {
    match verify_chain_ctx(vault_path) {
        Ok(report) => {
            if json_mode || output.json {
                output.print_json(&report);
            } else {
                output.print_stdout("=====================================");
                output.print_stdout("     Gantry History Verification     ");
                output.print_stdout("=====================================");
                output.print_stdout(&format!("Snapshots checked: {}", report.snapshots_checked));
                if report.intact {
                    output.print_stdout(&format!(
                        "Status:            {}",
                        output.style("INTACT", Some("green"), true)
                    ));
                    output.print_stdout("The snapshot history has not been tampered with.");
                } else {
                    output.print_stdout(&format!(
                        "Status:            {}",
                        output.style("BROKEN", Some("red"), true)
                    ));
                    if let Some(ref at) = report.broken_at {
                        output.print_stdout(&format!("First broken at:   {}", at));
                    }
                    if let Some(ref reason) = report.broken_reason {
                        output.print_stdout(&format!("Reason:            {}", reason));
                    }
                }
                output.print_stdout("=====================================");
            }
            if report.intact { Ok(0) } else { Ok(1) }
        }
        Err(e) => Err(format!("Verification error: {}", e)),
    }
}
