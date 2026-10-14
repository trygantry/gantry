use crate::output::Output;
use clap_complete::{Shell, generate};
use std::io;

pub fn handle(_output: &Output, shell: String, cmd: &mut clap::Command) -> Result<i32, String> {
    let generator = match shell.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "elvish" => Shell::Elvish,
        "fish" => Shell::Fish,
        "powershell" => Shell::PowerShell,
        "zsh" => Shell::Zsh,
        other => {
            return Err(format!(
                "Unsupported shell '{}'. Choose from: bash, elvish, fish, powershell, zsh",
                other
            ));
        }
    };
    generate(generator, cmd, "gantry", &mut io::stdout());
    Ok(0)
}
