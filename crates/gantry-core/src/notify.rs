use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;

use crate::{GantrySnapshot, SnapshotDiff};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SlackConfig {
    pub enabled: bool,
    pub webhook_env: String,
    pub events: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NotificationConfig {
    pub slack: Option<SlackConfig>,
}

pub fn compile_slack_digest(
    diff: &SnapshotDiff,
    prev: &GantrySnapshot,
    current: &GantrySnapshot,
) -> serde_json::Value {
    compile_slack_digest_for_events(diff, prev, current, &[])
}

pub fn compile_slack_digest_for_events(
    diff: &SnapshotDiff,
    _prev: &GantrySnapshot,
    current: &GantrySnapshot,
    selected_events: &[String],
) -> serde_json::Value {
    let mut blocks = Vec::new();
    let filter_enabled = !selected_events.is_empty();
    let selected: HashSet<&str> = selected_events.iter().map(|s| s.as_str()).collect();

    // 1. Header Block
    blocks.push(json!({
        "type": "header",
        "text": {
            "type": "plain_text",
            "text": "🚀 Gantry Release Control Room Update",
            "emoji": true
        }
    }));

    // 2. Context / Details Section
    let transition_str = format!("`{}` ➔ `{}`", diff.base_commit, diff.head_commit);
    blocks.push(json!({
        "type": "section",
        "fields": [
            {
                "type": "mrkdwn",
                "text": format!("*Release:* `{}`", current.release)
            },
            {
                "type": "mrkdwn",
                "text": format!("*Transition:* {}", transition_str)
            }
        ]
    }));

    // 3. Status Transitions
    let status_emoji = match current.readiness.to_lowercase().as_str() {
        "go" => "🟢",
        "at risk" | "atrisk" | "risk" => "🟡",
        _ => "🔴",
    };

    let status_text = if let Some((ref prev_state, ref curr_state)) = diff.readiness_changed {
        format!("*Readiness Change:* {} ➔ *{}*", prev_state, curr_state)
    } else {
        format!("*Readiness:* *{}*", current.readiness)
    };

    let score_text = if let Some((prev_score, curr_score)) = diff.score_changed {
        format!("*Score Change:* {}/100 ➔ *{}/100*", prev_score, curr_score)
    } else {
        format!("*Score:* *{}/100*", current.readiness_score)
    };

    blocks.push(json!({
        "type": "section",
        "text": {
            "type": "mrkdwn",
            "text": format!("{} {}  |  {}", status_emoji, status_text, score_text)
        }
    }));

    // Divider
    blocks.push(json!({
        "type": "divider"
    }));

    // 4. Detailed Updates Checklist
    let mut update_bullets = Vec::new();

    // Blockers
    if !filter_enabled || selected.contains("blockerResolved") {
        for resolved in &diff.resolved_blockers {
            update_bullets.push(format!("✅ *Resolved Blocker:* `{}`", resolved));
        }
        for added in &diff.added_blockers {
            update_bullets.push(format!("⚠️ *Added Blocker:* `{}`", added));
        }
    }

    // QA Suites
    if !filter_enabled || selected.contains("qaSuiteComplete") {
        for qa in &diff.completed_qa_suites {
            update_bullets.push(format!("📋 *QA Suite Completed:* `{}`", qa));
        }
    }

    // GoNoGo Gates
    if !filter_enabled || selected.contains("gateComplete") {
        for gate in &diff.completed_gates {
            update_bullets.push(format!("🏁 *Gate Passed:* `{}`", gate));
        }
    }

    // Signoffs
    if !filter_enabled || selected.contains("signoffComplete") {
        for signoff in &diff.completed_signoffs {
            update_bullets.push(format!("✍️ *Sign-off Approved:* *{}*", signoff));
        }
    }

    if update_bullets.is_empty() {
        update_bullets.push("No structural changes detected during this transition.".to_string());
    }

    let updates_text = update_bullets.join("\n");
    blocks.push(json!({
        "type": "section",
        "text": {
            "type": "mrkdwn",
            "text": updates_text
        }
    }));

    json!({
        "blocks": blocks
    })
}
