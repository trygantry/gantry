use crate::output::Output;
use gantry_core::{EvaluationOptions, write_launch_index_ctx};

pub fn handle(output: &Output, opts: &EvaluationOptions) -> Result<i32, String> {
    match write_launch_index_ctx(opts) {
        Ok(_) => {
            output.print_stdout("Successfully compiled and updated .gantry/index.json");
            Ok(0)
        }
        Err(e) => Err(format!("Failed to write launch index: {}", e)),
    }
}
