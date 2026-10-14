use crate::output::Output;
use gantry_core::EvaluationOptions;

pub fn handle(_output: &Output, opts: &EvaluationOptions) -> Result<i32, String> {
    match gantry_core::run_mcp_server_ctx(opts) {
        Ok(_) => Ok(0),
        Err(e) => Err(format!("MCP Server failed: {}", e)),
    }
}
