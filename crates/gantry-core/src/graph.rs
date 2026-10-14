use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

use crate::{
    BlockerEntry, EvaluationOptions, QAEntry, VaultContext, compile_launch_index_ctx,
    get_base_git_ref, get_changed_files_recent_commits, get_changed_files_since_tag,
    get_last_release_tag,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImpactSummary {
    pub changed_files: Vec<String>,
    pub impacted_blockers: Vec<String>,
    pub impacted_qa: Vec<String>,
    pub uncovered_changes: Vec<String>,
    pub suggested_tasks: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: String, // "commit" | "file" | "blocker" | "qa_suite" | "gate" | "signoff"
    pub status: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: String, // "impacts" | "covers" | "blocks" | "requires"
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImpactGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

pub fn compute_impact_summary(
    changed_files: Vec<String>,
    blockers: &[BlockerEntry],
    qa_suites: &[QAEntry],
) -> ImpactSummary {
    let mut impacted_blockers = Vec::new();
    let mut impacted_qa = Vec::new();
    let mut uncovered_changes = Vec::new();
    let mut suggested_tasks = Vec::new();

    let mut covered_paths = HashSet::new();

    // Match blockers
    for blocker in blockers {
        let mut blocker_matched = false;
        for src in &blocker.source_files {
            covered_paths.insert(src.clone());
            if let Some(parent) = Path::new(src).parent().and_then(|p| p.to_str()) {
                covered_paths.insert(parent.to_string());
            }
            if changed_files.contains(src) {
                blocker_matched = true;
            }
        }
        if blocker_matched {
            impacted_blockers.push(blocker.id.clone());
            suggested_tasks.push(format!(
                "Re-verify blocker {} after code changes",
                blocker.id
            ));
        }
    }

    // Match QA
    for qa in qa_suites {
        let mut qa_matched = false;
        for src in &qa.source_files {
            covered_paths.insert(src.clone());
            if let Some(parent) = Path::new(src).parent().and_then(|p| p.to_str()) {
                covered_paths.insert(parent.to_string());
            }
            if changed_files.contains(src) {
                qa_matched = true;
            }
        }
        if qa_matched {
            impacted_qa.push(qa.id.clone());
            suggested_tasks.push(format!("Re-verify QA plan {} after code changes", qa.id));
        }
    }

    // Identify uncovered changes
    let mut changed_dirs = std::collections::HashMap::new();
    for file in &changed_files {
        if file.starts_with(".gantry") || file.starts_with("node_modules") {
            continue;
        }
        if let Some(parent) = Path::new(file).parent().and_then(|p| p.to_str()) {
            if !parent.is_empty() {
                let count = changed_dirs.entry(parent.to_string()).or_insert(0);
                *count += 1;
            }
        }
    }

    for (dir, count) in changed_dirs {
        let dir_path = Path::new(&dir);
        let is_covered = covered_paths.contains(&dir)
            || covered_paths
                .iter()
                .any(|p| Path::new(p).starts_with(dir_path))
            || covered_paths.iter().any(|p| dir_path.starts_with(p));
        if !is_covered && count >= 2 {
            uncovered_changes.push(dir.clone());
            suggested_tasks.push(format!("Cover modified component with QA/Blocker: {}", dir));
        }
    }

    ImpactSummary {
        changed_files,
        impacted_blockers,
        impacted_qa,
        uncovered_changes,
        suggested_tasks,
    }
}

pub fn get_impact_summary_ctx(
    vault: &VaultContext,
    base_ref: Option<&str>,
) -> Result<ImpactSummary, String> {
    let base = match get_base_git_ref(&vault.project_root, base_ref) {
        Some(b) => b,
        None => return Err("Could not determine base Git reference for diff.".to_string()),
    };

    let changed_files = if base_ref.is_none() {
        let tag_opt = get_last_release_tag(&vault.project_root);
        if let Some(ref tag) = tag_opt {
            get_changed_files_since_tag(&vault.project_root, tag)
        } else {
            get_changed_files_recent_commits(&vault.project_root)
        }
    } else {
        get_changed_files_since_tag(&vault.project_root, &base)
    };

    let opts = EvaluationOptions {
        vault: vault.clone(),
        environment: None,
        policy_overlay: None,
    };
    let payload = compile_launch_index_ctx(&opts)?;

    Ok(compute_impact_summary(
        changed_files,
        &payload.blockers,
        &payload.qa_suites,
    ))
}

pub fn get_impact_summary_ex(base_ref: Option<&str>) -> Result<ImpactSummary, String> {
    let ctx = crate::get_default_context();
    get_impact_summary_ctx(&ctx, base_ref)
}

pub fn get_impact_graph_ctx(
    vault: &VaultContext,
    base_ref: Option<&str>,
) -> Result<ImpactGraph, String> {
    let summary = get_impact_summary_ctx(vault, base_ref)?;
    let opts = EvaluationOptions {
        vault: vault.clone(),
        environment: None,
        policy_overlay: None,
    };
    let payload = compile_launch_index_ctx(&opts)?;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // 1. Add File nodes & Commit node (if base_ref exists)
    let commit_label = base_ref.unwrap_or("HEAD").to_string();
    nodes.push(GraphNode {
        id: "commit_node".to_string(),
        label: commit_label,
        kind: "commit".to_string(),
        status: Some("active".to_string()),
    });

    for file in &summary.changed_files {
        let file_node_id = format!("file:{}", file);
        nodes.push(GraphNode {
            id: file_node_id.clone(),
            label: file.clone(),
            kind: "file".to_string(),
            status: None,
        });

        // Edge: Commit impacts File
        edges.push(GraphEdge {
            source: "commit_node".to_string(),
            target: file_node_id,
            kind: "impacts".to_string(),
        });
    }

    // 2. Add Blocker nodes
    for blocker in &payload.blockers {
        let is_impacted = summary.impacted_blockers.contains(&blocker.id);
        let node_status = if blocker.status.to_lowercase() == "resolved" {
            "resolved".to_string()
        } else if is_impacted {
            "impacted".to_string()
        } else {
            blocker.status.clone()
        };

        nodes.push(GraphNode {
            id: blocker.id.clone(),
            label: blocker.title.clone(),
            kind: "blocker".to_string(),
            status: Some(node_status),
        });

        for src in &blocker.source_files {
            let file_node_id = format!("file:{}", src);
            edges.push(GraphEdge {
                source: file_node_id,
                target: blocker.id.clone(),
                kind: "blocks".to_string(),
            });
        }
    }

    // 3. Add QA nodes
    for qa in &payload.qa_suites {
        let is_impacted = summary.impacted_qa.contains(&qa.id);
        let completeness = (qa.completed_tests * 100)
            .checked_div(qa.total_tests)
            .unwrap_or(0);
        let node_status = if completeness == 100 {
            "complete".to_string()
        } else if is_impacted {
            "impacted".to_string()
        } else {
            format!("{}%", completeness)
        };

        nodes.push(GraphNode {
            id: qa.id.clone(),
            label: qa.title.clone(),
            kind: "qa_suite".to_string(),
            status: Some(node_status),
        });

        for src in &qa.source_files {
            let file_node_id = format!("file:{}", src);
            edges.push(GraphEdge {
                source: file_node_id,
                target: qa.id.clone(),
                kind: "covers".to_string(),
            });
        }
    }

    // 4. Add Gates
    let mut gate_node_ids = Vec::new();
    for gate in &payload.go_no_go_gates {
        let gate_id = format!("gate:{}", gate.name);
        nodes.push(GraphNode {
            id: gate_id.clone(),
            label: format!("Gate: {}", gate.name),
            kind: "gate".to_string(),
            status: Some(gate.status.clone()),
        });
        gate_node_ids.push(gate_id);
    }

    // 5. Add Signoffs
    let mut signoff_node_ids = Vec::new();
    for signoff in &payload.signoffs {
        let signoff_id = format!("signoff:{}", signoff.role);
        nodes.push(GraphNode {
            id: signoff_id.clone(),
            label: format!("Signoff: {}", signoff.role),
            kind: "signoff".to_string(),
            status: Some(signoff.status.clone()),
        });
        signoff_node_ids.push(signoff_id);
    }

    // 6. Bridge launch dependencies
    for blocker in &payload.blockers {
        if blocker.status.to_lowercase() != "resolved" {
            for gate_id in &gate_node_ids {
                edges.push(GraphEdge {
                    source: blocker.id.clone(),
                    target: gate_id.clone(),
                    kind: "requires".to_string(),
                });
            }
        }
    }

    for qa in &payload.qa_suites {
        let complete = qa.total_tests > 0 && qa.completed_tests == qa.total_tests;
        if !complete {
            for gate_id in &gate_node_ids {
                edges.push(GraphEdge {
                    source: qa.id.clone(),
                    target: gate_id.clone(),
                    kind: "requires".to_string(),
                });
            }
        }
    }

    for gate_id in &gate_node_ids {
        for signoff_id in &signoff_node_ids {
            edges.push(GraphEdge {
                source: gate_id.clone(),
                target: signoff_id.clone(),
                kind: "requires".to_string(),
            });
        }
    }

    Ok(ImpactGraph { nodes, edges })
}

pub fn get_impact_graph(base_ref: Option<&str>) -> Result<ImpactGraph, String> {
    let ctx = crate::get_default_context();
    get_impact_graph_ctx(&ctx, base_ref)
}
