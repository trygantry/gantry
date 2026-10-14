use crate::output::Output;
use std::path::Path;

pub enum TaskCommand {
    Complete { id: String },
    Ignore { id: String, reason: String },
}

pub fn handle(output: &Output, command: TaskCommand, vault_path: &Path) -> Result<i32, String> {
    match command {
        TaskCommand::Complete { id } => {
            match gantry_core::complete_task_by_id_ctx(vault_path, &id) {
                Ok(_) => {
                    output
                        .print_stdout(&format!("Successfully marked task '{}' as completed.", id));
                    Ok(0)
                }
                Err(e) => Err(format!("Failed to complete task: {}", e)),
            }
        }
        TaskCommand::Ignore { id, reason } => {
            match gantry_core::ignore_task_by_id_ctx(vault_path, &id, &reason) {
                Ok(_) => {
                    output.print_stdout(&format!(
                        "Successfully ignored task '{}' with reason: {}.",
                        id, reason
                    ));
                    Ok(0)
                }
                Err(e) => Err(format!("Failed to ignore task: {}", e)),
            }
        }
    }
}
