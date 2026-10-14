use crate::output::Output;
use gantry_core::list_pending_tasks_ctx;
use std::path::Path;

pub fn handle(output: &Output, json_mode: bool, vault_path: &Path) -> Result<i32, String> {
    match list_pending_tasks_ctx(vault_path) {
        Ok(tasks) => {
            if json_mode || output.json {
                output.print_json(&tasks);
            } else {
                output.print_stdout("=====================================");
                output.print_stdout("        Gantry Pending Tasks         ");
                output.print_stdout("=====================================");
                let pending_tasks: Vec<&gantry_core::GantryTask> =
                    tasks.iter().filter(|t| !t.completed).collect();
                if pending_tasks.is_empty() {
                    output.print_stdout("No pending repair tasks found!");
                } else {
                    for task in pending_tasks {
                        output.print_stdout(&format!(
                            "- [{}] ({}): {}",
                            task.id, task.severity, task.title
                        ));
                        output.print_stdout(&format!("  Source:   {}", task.source));
                        output.print_stdout(&format!("  Evidence: {}", task.evidence));
                        if !task.status.is_empty() && task.status != "pending" {
                            output.print_stdout(&format!("  Status:   {}", task.status));
                        }
                        if let Some(ref r) = task.reason {
                            output.print_stdout(&format!("  Reason:   {}", r));
                        }
                    }
                }
                output.print_stdout("=====================================");
            }
            Ok(0)
        }
        Err(e) => Err(format!("Failed to list pending tasks: {}", e)),
    }
}
