use crate::output::Output;
use std::path::Path;

#[cfg(feature = "telemetry")]
fn pull_telemetry_from_sentry(
    output: &Output,
    env_name: &str,
    vault_path: &Path,
) -> Result<(), String> {
    let config_path = vault_path.join("config.json");
    if !config_path.exists() {
        return Err("Configuration file config.json not found in .gantry/".to_string());
    }

    let config_content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config.json: {}", e))?;

    let gantry_config: gantry_core::GantryConfig = serde_json::from_str(&config_content)
        .map_err(|e| format!("Failed to parse config.json: {}", e))?;

    let sentry_config = gantry_config.sentry.as_ref().ok_or_else(|| {
        "Sentry organization and project are not configured in config.json under 'sentry'"
            .to_string()
    })?;

    let token = std::env::var("GANTRY_SENTRY_TOKEN")
        .map_err(|_| "GANTRY_SENTRY_TOKEN environment variable is not set.".to_string())?;

    let release_version = gantry_config.version.clone();

    // Resolve numeric project ID
    let mut project_id = sentry_config.project_id.clone();
    if project_id.is_none() {
        let project_url = format!(
            "https://api.sentry.io/0/projects/{}/{}/",
            sentry_config.organization, sentry_config.project
        );
        output.print_stdout(&format!(
            "Resolving Sentry project slug '{}' to project ID...",
            sentry_config.project
        ));
        let mut resp = ureq::get(&project_url)
            .header("Authorization", &format!("Bearer {}", token))
            .call()
            .map_err(|e| format!("Failed to resolve Sentry project ID: {}", e))?;
        let body_str = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("Failed to read Sentry project body: {}", e))?;
        let project_response: serde_json::Value = serde_json::from_str(&body_str)
            .map_err(|e| format!("Failed to parse Sentry project JSON: {}", e))?;

        if let Some(id_str) = project_response.get("id").and_then(|id| id.as_str()) {
            project_id = Some(id_str.to_string());
        } else if let Some(id_num) = project_response.get("id").and_then(|id| id.as_i64()) {
            project_id = Some(id_num.to_string());
        } else {
            return Err("Sentry project response did not contain a valid ID.".to_string());
        }
    }

    let resolved_project_id =
        project_id.ok_or_else(|| "Could not resolve Sentry project ID.".to_string())?;

    let url = format!(
        "https://api.sentry.io/0/organizations/{}/sessions/",
        sentry_config.organization
    );

    output.print_stdout(&format!(
        "Fetching Sentry session stats for release '{}' in environment '{}'...",
        release_version, env_name
    ));

    let mut resp = ureq::get(&url)
        .header("Authorization", &format!("Bearer {}", token))
        .query("project", &resolved_project_id)
        .query("field", "crash_free_rate(session)")
        .query("query", format!("release:{}", release_version))
        .query("environment", env_name)
        .query("statsPeriod", "24h")
        .call()
        .map_err(|e| format!("Sentry API request failed: {}", e))?;
    let body_str = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read Sentry session stats: {}", e))?;
    let response: serde_json::Value = serde_json::from_str(&body_str)
        .map_err(|e| format!("Failed to parse Sentry JSON response: {}", e))?;

    let mut crash_rate = None;
    if let Some(groups) = response.get("groups").and_then(|g| g.as_array()) {
        if let Some(first_group) = groups.first() {
            if let Some(totals) = first_group.get("totals") {
                if let Some(val) = totals
                    .get("crash_free_rate(session)")
                    .and_then(|v| v.as_f64())
                {
                    crash_rate = Some(val);
                }
            }
            if crash_rate.is_none() {
                if let Some(series) = first_group.get("series") {
                    if let Some(rates) = series
                        .get("crash_free_rate(session)")
                        .and_then(|r| r.as_array())
                    {
                        for item in rates.iter().rev() {
                            if let Some(val) = item.as_f64() {
                                crash_rate = Some(val);
                                break;
                            } else if let Some(arr) = item.as_array() {
                                if arr.len() == 2 {
                                    if let Some(val) = arr[1].as_f64() {
                                        crash_rate = Some(val);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let crash_free_rate = crash_rate.ok_or_else(|| {
        format!(
            "No crash-free rate statistics returned by Sentry for release '{}' in '{}'.",
            release_version, env_name
        )
    })?;

    let observed_path = vault_path.join("observed.json");
    let mut telemetry_map = gantry_core::TelemetryObsMap {
        observations: std::collections::HashMap::new(),
    };

    if observed_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&observed_path) {
            if let Ok(existing) = serde_json::from_str::<gantry_core::TelemetryObsMap>(&content) {
                telemetry_map = existing;
            }
        }
    }

    let fetched_at = chrono::Utc::now().to_rfc3339();
    let observation = gantry_core::ObservedTelemetry {
        source: "sentry".to_string(),
        environment: env_name.to_string(),
        release: release_version.clone(),
        crash_free_rate: Some(crash_free_rate),
        metrics: std::collections::HashMap::new(),
        fetched_at,
    };

    telemetry_map
        .observations
        .insert(env_name.to_string(), observation);

    let serialized = serde_json::to_string_pretty(&telemetry_map)
        .map_err(|e| format!("Failed to serialize telemetry map: {}", e))?;

    std::fs::write(&observed_path, serialized)
        .map_err(|e| format!("Failed to write observed.json: {}", e))?;

    output.print_stdout(&format!(
        "Crash-free rate successfully pulled: {}%",
        crash_free_rate
    ));
    Ok(())
}

pub fn handle(output: &Output, env_arg: Option<String>, vault_path: &Path) -> Result<i32, String> {
    #[cfg(feature = "telemetry")]
    {
        let target_env = env_arg.unwrap_or_else(|| "production".to_string());
        pull_telemetry_from_sentry(output, &target_env, vault_path)?;
        Ok(0)
    }

    #[cfg(not(feature = "telemetry"))]
    {
        let _ = env_arg;
        let _ = vault_path;
        let _ = output;
        Err(
            "Error: Telemetry feature is not enabled. Build with '--features telemetry' to enable."
                .to_string(),
        )
    }
}
