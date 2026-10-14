use crate::VaultContext;
use crate::release::{ReleaseStore, VaultHealthReport};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactedSupportBundle {
    pub schema_version: String,
    pub generated_at: String,
    pub gantry_version: String,
    pub platform: String,
    pub architecture: String,
    pub health: VaultHealthReport,
    pub release_stages: BTreeMap<String, usize>,
    pub mobile_policy_present: bool,
    pub authority_tools: BTreeMap<String, bool>,
    pub redaction: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportBundleResult {
    pub schema_version: String,
    pub path: PathBuf,
    pub bundle: RedactedSupportBundle,
}

pub fn write_redacted_support_bundle(
    vault: &VaultContext,
    destination: Option<PathBuf>,
) -> Result<SupportBundleResult, String> {
    let store = ReleaseStore::new(&vault.vault_path, &vault.project_root);
    let health = store.check_vault(false)?;
    let mut release_stages = BTreeMap::new();
    for release in store.list()? {
        *release_stages.entry(release.stage.to_string()).or_insert(0) += 1;
    }
    let authority_tools = [
        "codesign",
        "apksigner",
        "jarsigner",
        "bundletool",
        "apkanalyzer",
    ]
    .into_iter()
    .map(|tool| {
        let available = Command::new(tool).arg("--version").output().is_ok();
        (tool.to_string(), available)
    })
    .collect();
    let bundle = RedactedSupportBundle {
        schema_version: "1.0".to_string(),
        generated_at: Utc::now().to_rfc3339(),
        gantry_version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        health,
        release_stages,
        mobile_policy_present: vault.vault_path.join("mobile-policy.json").is_file(),
        authority_tools,
        redaction: vec![
            "No source file contents or names are included.".to_string(),
            "No artifact paths, hashes, application identifiers, or signing identities are included."
                .to_string(),
            "No environment variables, credentials, evidence payloads, or actor identities are included."
                .to_string(),
        ],
    };
    let destination = destination.unwrap_or_else(|| {
        vault.vault_path.join("support").join(format!(
            "gantry-support-{}.json",
            Utc::now().format("%Y%m%dT%H%M%SZ")
        ))
    });
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create support directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let mut bytes = serde_json::to_vec_pretty(&bundle)
        .map_err(|error| format!("Failed to serialize support bundle: {error}"))?;
    bytes.push(b'\n');
    std::fs::write(&destination, bytes).map_err(|error| {
        format!(
            "Failed to write support bundle {}: {error}",
            destination.display()
        )
    })?;
    Ok(SupportBundleResult {
        schema_version: "1.0".to_string(),
        path: destination,
        bundle,
    })
}
