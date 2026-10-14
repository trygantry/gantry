use crate::MobileCommands;
use crate::output::Output;
use gantry_core::mobile::{
    MobileDecision, MobileFindingStatus, MobilePlatform, MobileReportFormat, MobileStack,
    describe_mobile_audit_cache, detect_mobile_project, init_mobile_policy, mobile_rule_catalog,
    render_mobile_report, run_mobile_audit, write_mobile_report,
};
use std::path::Path;

pub fn handle(
    output: &Output,
    command: MobileCommands,
    project_root: &Path,
) -> Result<i32, String> {
    match command {
        MobileCommands::Detect { json } => handle_detect(output, project_root, json),
        MobileCommands::Audit {
            artifact,
            format,
            output: output_path,
            write,
            ci,
            json,
        } => handle_audit(
            output,
            project_root,
            artifact.as_deref(),
            format.as_deref(),
            output_path.as_deref(),
            write,
            ci,
            json,
        ),
        MobileCommands::CacheKey { artifact, json } => {
            let descriptor = describe_mobile_audit_cache(project_root, &artifact)?;
            if json {
                output.print_json(&descriptor);
            } else if let Some(key) = descriptor.cache_key {
                output.print_stdout(&format!("Mobile audit cache key: {key}"));
                if let Some(path) = descriptor.cache_path {
                    output.print_stdout(&format!("Path: {path}"));
                }
            } else {
                output.print_stdout(
                    "Mobile audit cache is ineligible because Git identity is unavailable or source inputs are dirty.",
                );
            }
            Ok(0)
        }
        MobileCommands::Rules { json } => handle_rules(output, json),
        MobileCommands::Init { force, json } => handle_init(output, project_root, force, json),
    }
}

fn handle_detect(output: &Output, project_root: &Path, json: bool) -> Result<i32, String> {
    let project = detect_mobile_project(project_root)?;
    if json {
        output.print_json(&project);
    } else {
        output.heading("Mobile project");
        output.field("Project", &project.root);
        output.field("Stacks", join_stacks(&project.stacks));
        output.field("Platforms", join_platforms(&project.platforms));
        output.field(
            "Detected",
            format!(
                "{} iOS target{} · {} Android app{}",
                project.ios_targets.len(),
                plural(project.ios_targets.len()),
                project.android_apps.len(),
                plural(project.android_apps.len())
            ),
        );
        output.field(
            "Coverage",
            format!("{} files inspected", project.inspected_files),
        );
        if project.platforms.is_empty() {
            output.print_stdout("\nNext: run this command from the mobile project root.");
        }
    }
    Ok(if project.platforms.is_empty() { 1 } else { 0 })
}

#[allow(clippy::too_many_arguments)]
fn handle_audit(
    output: &Output,
    project_root: &Path,
    artifact: Option<&Path>,
    format: Option<&str>,
    output_path: Option<&Path>,
    write: bool,
    ci: bool,
    json: bool,
) -> Result<i32, String> {
    let report = run_mobile_audit(project_root, artifact)?;
    let report_format = if json {
        MobileReportFormat::Json
    } else if let Some(format) = format {
        MobileReportFormat::parse(format)?
    } else {
        MobileReportFormat::Json
    };

    let should_write = write || output_path.is_some();
    let mut written_path = None;
    if should_write {
        let path = write_mobile_report(&report, output_path, report_format)?;
        written_path = Some(path.clone());
        if !json {
            output.print_stdout(&format!(
                "Mobile release report written to {}",
                path.display()
            ));
            print_summary(output, &report);
        }
    } else if json {
        // JSON uses one stable envelope whether or not persistence is requested.
    } else if format.is_some() {
        output.print_stdout(&render_mobile_report(&report, report_format)?);
    } else {
        print_summary(output, &report);
        for finding in report
            .findings
            .iter()
            .filter(|f| !matches!(f.status, gantry_core::mobile::MobileFindingStatus::Pass))
        {
            output.print_stdout(&format!(
                "  [{}] {} · {}\n    Next: {}",
                finding_status(&finding.status),
                finding.id,
                finding.title,
                finding.remediation
            ));
        }
    }

    if json {
        output.print_json(&serde_json::json!({
            "schema_version": gantry_core::mobile::MOBILE_REPORT_SCHEMA_VERSION,
            "report": &report,
            "report_path": written_path,
        }));
    }

    if ci && report.decision == MobileDecision::NoGo {
        for finding in report.findings.iter().filter(|f| {
            f.status == gantry_core::mobile::MobileFindingStatus::Fail
                && f.severity == gantry_core::mobile::MobileSeverity::Blocker
        }) {
            output.print_stderr(&format!(
                "::error title={} {}::{}",
                github_command_escape(&finding.finding_instance_id),
                github_command_escape(&finding.title),
                github_command_escape(&finding.remediation)
            ));
        }
    }
    Ok(if report.decision == MobileDecision::NoGo {
        1
    } else {
        0
    })
}

fn github_command_escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace(':', "%3A")
        .replace(',', "%2C")
}

fn print_summary(output: &Output, report: &gantry_core::mobile::MobileAuditReport) {
    let decision = match report.decision {
        MobileDecision::Go => output.style("GO", Some("green"), true),
        MobileDecision::AtRisk => output.style("AT RISK", Some("yellow"), true),
        MobileDecision::NoGo => output.style("NO-GO", Some("red"), true),
    };
    output.heading("Mobile release audit");
    output.field("Decision", decision);
    if !report.decision_reasons.is_empty() {
        output.field("Why", report.decision_reasons.join("; "));
    }
    output.field("Commit", &report.git_commit);
    if let Some(artifact) = &report.artifact {
        output.field("Artifact", &artifact.sha256);
    }
    output.field(
        "Results",
        format!(
            "{} passed · {} blockers · {} warnings · {} manual · {} unavailable",
            report.summary.passed,
            report.summary.blockers,
            report.summary.warnings,
            report.summary.manual,
            report.summary.unavailable
        ),
    );
}

fn handle_rules(output: &Output, json: bool) -> Result<i32, String> {
    let catalog = mobile_rule_catalog();
    if json {
        output.print_json(&catalog);
    } else {
        output.print_stdout("Gantry mobile rule catalog");
        output.print_stdout(&format!(
            "Policy pack: {} (reviewed {})",
            catalog["policy_pack_version"].as_str().unwrap_or("unknown"),
            catalog["reviewed_at"].as_str().unwrap_or("unknown")
        ));
        if let Some(rules) = catalog["rules"].as_array() {
            for rule in rules {
                output.print_stdout(&format!(
                    "  {} [{}] {}",
                    rule["id"].as_str().unwrap_or("unknown"),
                    rule["category"].as_str().unwrap_or("unknown"),
                    rule["description"].as_str().unwrap_or("")
                ));
            }
        }
    }
    Ok(0)
}

fn handle_init(
    output: &Output,
    project_root: &Path,
    force: bool,
    json: bool,
) -> Result<i32, String> {
    let path = init_mobile_policy(project_root, force)?;
    if json {
        output.print_json(&serde_json::json!({"status":"success","path":path}));
    } else {
        output.print_stdout(&format!(
            "Mobile release policy ready at {}",
            path.display()
        ));
    }
    Ok(0)
}

fn join_stacks(values: &[MobileStack]) -> String {
    values
        .iter()
        .map(|value| match value {
            MobileStack::Flutter => "Flutter",
            MobileStack::ReactNative => "React Native",
            MobileStack::NativeIos => "Native iOS",
            MobileStack::NativeAndroid => "Native Android",
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn join_platforms(values: &[MobilePlatform]) -> String {
    values
        .iter()
        .map(|value| match value {
            MobilePlatform::Ios => "iOS",
            MobilePlatform::Android => "Android",
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn finding_status(status: &MobileFindingStatus) -> &'static str {
    match status {
        MobileFindingStatus::Pass => "PASS",
        MobileFindingStatus::Fail => "BLOCKED",
        MobileFindingStatus::Warning => "REVIEW",
        MobileFindingStatus::Manual => "HUMAN PROOF",
        MobileFindingStatus::Unavailable => "UNAVAILABLE",
    }
}
