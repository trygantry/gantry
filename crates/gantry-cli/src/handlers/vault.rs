use crate::output::Output;
use gantry_core::VaultContext;
use gantry_core::release::ReleaseStore;
use std::path::PathBuf;

pub enum VaultCommand {
    Check { repair: bool, json: bool },
    Migrate { dry_run: bool, json: bool },
    SupportBundle { output: Option<PathBuf>, json: bool },
}

pub fn handle(output: &Output, command: VaultCommand, vault: &VaultContext) -> Result<i32, String> {
    let store = ReleaseStore::new(&vault.vault_path, &vault.project_root);
    match command {
        VaultCommand::Check { repair, json } => {
            let report = store.check_vault(repair)?;
            let exit_code = if report.healthy { 0 } else { 1 };
            if json || output.json {
                output.print_json(&report);
            } else {
                output.print_stdout(if report.healthy {
                    "Gantry vault is healthy."
                } else {
                    "Gantry vault needs attention."
                });
                output.print_stdout(&format!(
                    "Verified releases: {} · pending transactions: {} · repaired: {}",
                    report.verified_releases,
                    report.pending_transactions,
                    report.repaired_transactions
                ));
                for error in report.errors {
                    output.print_stdout(&format!("- {error}"));
                }
            }
            Ok(exit_code)
        }
        VaultCommand::Migrate { dry_run, json } => {
            let report = store.migrate_vault(dry_run)?;
            if json || output.json {
                output.print_json(&report);
            } else {
                output.print_stdout(if dry_run {
                    "Gantry vault migration plan"
                } else {
                    "Gantry vault migration result"
                });
                output.print_stdout(&format!(
                    "Current release schema: {} · migrations required: {} · migrated: {}",
                    report.current_release_schema,
                    report.migrations_required,
                    report.migrated_releases
                ));
                for message in report.messages {
                    output.print_stdout(&format!("- {message}"));
                }
            }
            Ok(0)
        }
        VaultCommand::SupportBundle {
            output: destination,
            json,
        } => {
            let result = gantry_core::write_redacted_support_bundle(vault, destination)?;
            if json || output.json {
                output.print_json(&result);
            } else {
                output.print_stdout(&format!(
                    "Wrote redacted Gantry support bundle to {}.",
                    result.path.display()
                ));
                output.print_stdout("Review the JSON before sharing it outside your organization.");
            }
            Ok(0)
        }
    }
}
