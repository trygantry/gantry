use crate::output::Output;
use std::path::Path;

fn get_sorted_snapshot_filenames(vault_path: &Path) -> Result<Vec<String>, String> {
    let snapshots_dir = vault_path.join("snapshots");
    if !snapshots_dir.exists() {
        return Ok(Vec::new());
    }

    let mut filenames = Vec::new();
    if let Ok(entries) = std::fs::read_dir(snapshots_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "json" {
                        if let Some(file_name) = path.file_name() {
                            filenames.push(file_name.to_string_lossy().into_owned());
                        }
                    }
                }
            }
        }
    }

    filenames.sort();
    Ok(filenames)
}

fn score_bucket(score: u8) -> u8 {
    if score < 50 {
        0
    } else if score < 70 {
        1
    } else if score < 90 {
        2
    } else {
        3
    }
}

fn derive_transition_events(
    diff: &gantry_core::SnapshotDiff,
    base: &gantry_core::GantrySnapshot,
    head: &gantry_core::GantrySnapshot,
) -> Vec<String> {
    let mut events = Vec::new();
    if diff.readiness_changed.is_some() {
        events.push("readinessChanged".to_string());
    }
    if score_bucket(base.readiness_score) != score_bucket(head.readiness_score) {
        events.push("scoreThresholdCrossed".to_string());
    }
    if !diff.resolved_blockers.is_empty() {
        events.push("blockerResolved".to_string());
    }
    if !diff.completed_qa_suites.is_empty() {
        events.push("qaSuiteComplete".to_string());
    }
    if !diff.completed_signoffs.is_empty() {
        events.push("signoffComplete".to_string());
    }
    if !diff.completed_gates.is_empty() {
        events.push("gateComplete".to_string());
    }
    events
}

pub fn handle(output: &Output, dry_run: bool, vault_path: &Path) -> Result<i32, String> {
    let config_path = vault_path.join("config.json");
    if !config_path.exists() {
        return Err("config.json not found. Initialize vault first.".to_string());
    }

    let config_content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config.json: {}", e))?;

    let config: gantry_core::GantryConfig = serde_json::from_str(&config_content)
        .map_err(|e| format!("Failed to parse config.json: {}", e))?;

    let slack_config = match config.notifications.as_ref().and_then(|n| n.slack.as_ref()) {
        Some(s) if s.enabled => s,
        _ => {
            output.print_stdout("Slack notifications are disabled or not configured. Exiting.");
            return Ok(0);
        }
    };

    let filenames = get_sorted_snapshot_filenames(vault_path)?;

    if filenames.len() < 2 {
        output.print_stdout("Fewer than two snapshots found; no transitions to notify.");
        return Ok(0);
    }

    let prev_file = vault_path
        .join("snapshots")
        .join(&filenames[filenames.len() - 2]);
    let head_file = vault_path
        .join("snapshots")
        .join(&filenames[filenames.len() - 1]);

    let prev_content = std::fs::read_to_string(&prev_file)
        .map_err(|e| format!("Failed to read previous snapshot: {}", e))?;
    let head_content = std::fs::read_to_string(&head_file)
        .map_err(|e| format!("Failed to read head snapshot: {}", e))?;

    let prev_snap: gantry_core::GantrySnapshot = serde_json::from_str(&prev_content)
        .map_err(|e| format!("Failed to parse previous snapshot: {}", e))?;
    let head_snap: gantry_core::GantrySnapshot = serde_json::from_str(&head_content)
        .map_err(|e| format!("Failed to parse head snapshot: {}", e))?;

    let diff = gantry_core::diff_snapshots(&prev_snap, &head_snap);
    let derived_events = derive_transition_events(&diff, &prev_snap, &head_snap);
    let selected_events: Vec<String> = if slack_config.events.is_empty() {
        derived_events.clone()
    } else {
        derived_events
            .into_iter()
            .filter(|e| slack_config.events.iter().any(|cfg| cfg == e))
            .collect()
    };

    if selected_events.is_empty() {
        output
            .print_stdout("No configured Slack events matched this snapshot transition. Skipping.");
        return Ok(0);
    }

    let event_id = format!("transition-{}-{}", diff.base_commit, diff.head_commit);
    let state_path = vault_path.join("state.json");
    let mut state: gantry_core::GantryState = if state_path.exists() {
        let content = std::fs::read_to_string(&state_path)
            .map_err(|e| format!("Failed to read state.json: {}", e))?;
        serde_json::from_str(&content).unwrap_or(gantry_core::GantryState {
            last_indexed_commit: head_snap.commit.clone(),
            last_snapshot: filenames[filenames.len() - 1].clone(),
            dispatched_events: Vec::new(),
            chain_head: String::new(),
        })
    } else {
        gantry_core::GantryState {
            last_indexed_commit: head_snap.commit.clone(),
            last_snapshot: filenames[filenames.len() - 1].clone(),
            dispatched_events: Vec::new(),
            chain_head: String::new(),
        }
    };

    if state.dispatched_events.contains(&event_id) {
        output.print_stdout(&format!(
            "Notification for transition '{}' was already dispatched. Skipping.",
            event_id
        ));
        return Ok(0);
    }

    let payload = gantry_core::compile_slack_digest_for_events(
        &diff,
        &prev_snap,
        &head_snap,
        &selected_events,
    );

    if dry_run {
        output.print_stdout("--- Dry-Run: Derived Slack Payload ---");
        output.print_stdout(&format!("Derived events: {}", selected_events.join(", ")));
        let rendered = serde_json::to_string_pretty(&payload)
            .map_err(|e| format!("Failed to serialize Slack payload: {e}"))?;
        output.print_stdout(&rendered);
        output.print_stdout("--------------------------------------");
    } else {
        let webhook_url = std::env::var(&slack_config.webhook_env)
            .map_err(|_| format!(
                "Error: Webhook environment variable '{}' is not set. Cannot dispatch Slack notification.",
                slack_config.webhook_env
            ))?;

        output.print_stdout(&format!(
            "Sending Slack notification for transition '{}'...",
            event_id
        ));

        let resp = ureq::post(&webhook_url)
            .send_json(&payload)
            .map_err(|e| format!("Failed to send Slack notification: {}", e))?;

        // In ureq 3.x, Response holds the response state and must succeed
        if resp.status().is_success() {
            state.dispatched_events.push(event_id.clone());
            let serialized = serde_json::to_string_pretty(&state)
                .map_err(|e| format!("Failed to serialize notification state: {e}"))?;
            std::fs::write(&state_path, serialized)
                .map_err(|e| format!("Failed to save state.json: {}", e))?;
            output.print_stdout(&format!(
                "Notification for transition '{}' successfully dispatched.",
                event_id
            ));
        } else {
            return Err(format!(
                "Slack notification server returned status code: {}",
                resp.status()
            ));
        }
    }

    Ok(0)
}
