use crate::output::Output;
use std::path::Path;

pub enum HooksCommand {
    Install { force: bool, stage: String },
    Uninstall { stage: String },
}

fn strip_gantry_hook_block(content: &str) -> String {
    let start_marker = "# --- gantry launch gate ---";
    let end_marker = "# --- end gantry launch gate ---";
    let mut result = String::new();
    let mut skip = false;
    for line in content.lines() {
        if line.trim_start().starts_with(start_marker) {
            skip = true;
            continue;
        }
        if line.trim_start().starts_with(end_marker) {
            skip = false;
            continue;
        }
        if !skip {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

pub fn handle(output: &Output, command: HooksCommand, project_root: &Path) -> Result<i32, String> {
    let git_dir_res = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(project_root)
        .output();

    let git_dir_path = match git_dir_res {
        Ok(out) if out.status.success() => {
            let relative_git_dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
            project_root.join(relative_git_dir)
        }
        _ => {
            return Err("Error: Not inside a git repository.".to_string());
        }
    };

    match command {
        HooksCommand::Install { force, stage } => {
            let hook_name = match stage.as_str() {
                "commit" => "pre-commit",
                _ => "pre-push",
            };

            let hook_path = git_dir_path.join("hooks").join(hook_name);

            let gantry_block = concat!(
                "\n# --- gantry launch gate ---\n",
                "if command -v gantry >/dev/null 2>&1; then\n",
                "  gantry validate --ci\n",
                "  _gantry_exit=$?\n",
                "  if [ \"$_gantry_exit\" -ne 0 ]; then\n",
                "    echo \"Gantry: launch gate blocked (exit $_gantry_exit). Run 'gantry audit' for details.\"\n",
                "    exit $_gantry_exit\n",
                "  fi\n",
                "fi\n",
                "# --- end gantry launch gate ---\n"
            ).to_string();

            let existing = if hook_path.exists() {
                std::fs::read_to_string(&hook_path)
                    .map_err(|e| format!("Failed to read existing hook: {}", e))?
            } else {
                String::new()
            };

            if existing.contains("# --- gantry launch gate ---") {
                output.print_stdout(&format!(
                    "Gantry hook already installed in {}.",
                    hook_path.display()
                ));
                return Ok(0);
            }

            let new_content = if existing.is_empty() || force {
                format!("#!/bin/sh\n{}", gantry_block)
            } else {
                format!("{}{}", existing.trim_end(), gantry_block)
            };

            std::fs::write(&hook_path, &new_content)
                .map_err(|e| format!("Failed to write hook: {}", e))?;

            // Make executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&hook_path) {
                    let mut perms = meta.permissions();
                    perms.set_mode(perms.mode() | 0o111);
                    let _ = std::fs::set_permissions(&hook_path, perms);
                }
            }

            output.print_stdout(&format!(
                "Gantry {} hook installed at {}.",
                hook_name,
                hook_path.display()
            ));
            output.print_stdout("Pushes will be blocked when readiness is No-Go.");
            output.print_stdout(
                "Use 'git push --no-verify' to bypass (will be visible in 'gantry verify').",
            );
            Ok(0)
        }
        HooksCommand::Uninstall { stage } => {
            let mut removed = 0usize;
            let stages = if stage == "all" || stage.is_empty() {
                vec!["pre-push", "pre-commit"]
            } else {
                match stage.as_str() {
                    "commit" => vec!["pre-commit"],
                    _ => vec!["pre-push"],
                }
            };

            for hook_name in stages {
                let hook_path = git_dir_path.join("hooks").join(hook_name);
                if !hook_path.exists() {
                    continue;
                }
                let content = std::fs::read_to_string(&hook_path).unwrap_or_default();
                if !content.contains("# --- gantry launch gate ---") {
                    continue;
                }
                let stripped = strip_gantry_hook_block(&content);
                if stripped.trim() == "#!/bin/sh" || stripped.trim().is_empty() {
                    let _ = std::fs::remove_file(&hook_path);
                } else {
                    let _ = std::fs::write(&hook_path, stripped);
                }
                output.print_stdout(&format!(
                    "Removed Gantry block from {}.",
                    hook_path.display()
                ));
                removed += 1;
            }

            if removed == 0 {
                output.print_stdout("No Gantry hooks found to remove.");
            }
            Ok(0)
        }
    }
}
