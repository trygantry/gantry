use crate::output::Output;
use chrono::{DateTime, Utc};
use gantry_core::VaultContext;
use gantry_core::release::{
    HumanDecisionKind, MobilePlatform, NewCandidate, NewFindingResolution, NewHumanDecision,
    NewRelease, NewReleaseAudit, NewReleaseDiff, NewReleaseEvidence, NewReleaseSchedule,
    NewReleaseWaiver, ReleaseAuditDecision, ReleaseStore, ReleaseView,
};
use std::path::PathBuf;

pub enum ReleaseCommand {
    Create {
        version: String,
        name: Option<String>,
        owner: Option<String>,
        actor: Option<String>,
        json: bool,
    },
    Ensure {
        version: String,
        name: Option<String>,
        owner: Option<String>,
        actor: Option<String>,
        json: bool,
    },
    List {
        json: bool,
    },
    Portfolio {
        json: bool,
    },
    Show {
        release: String,
        json: bool,
    },
    Schedule {
        release: String,
        target_date: Option<String>,
        submission_date: Option<String>,
        rollout_date: Option<String>,
        actor: Option<String>,
        json: bool,
    },
    CandidateAdd {
        release: String,
        artifact: PathBuf,
        platform: Option<MobilePlatform>,
        actor: Option<String>,
        json: bool,
    },
    CandidateEnsure {
        release: String,
        artifact: PathBuf,
        platform: Option<MobilePlatform>,
        actor: Option<String>,
        json: bool,
    },
    Audit {
        release: String,
        platform: Option<MobilePlatform>,
        actor: Option<String>,
        reuse_unchanged: bool,
        ci: bool,
        json: bool,
    },
    EvidenceAdd {
        release: String,
        platform: Option<MobilePlatform>,
        finding: String,
        kind: String,
        title: String,
        issuer: String,
        path: Option<String>,
        url: Option<String>,
        expires_at: Option<String>,
        actor: Option<String>,
        json: bool,
    },
    FindingResolve {
        release: String,
        platform: Option<MobilePlatform>,
        finding: String,
        reason: String,
        evidence_ids: Vec<String>,
        actor: Option<String>,
        json: bool,
    },
    Decide {
        release: String,
        platform: Option<MobilePlatform>,
        decided_by: String,
        role: String,
        rationale: String,
        decision: HumanDecisionKind,
        json: bool,
    },
    Waive {
        release: String,
        platform: Option<MobilePlatform>,
        finding: String,
        reason: String,
        approved_by: String,
        expires_at: String,
        json: bool,
    },
    Packet {
        release: String,
        json: bool,
    },
    Diff {
        release: String,
        platform: Option<MobilePlatform>,
        from: Option<String>,
        to: Option<String>,
        json: bool,
    },
    Manifest {
        json: bool,
    },
    History {
        json: bool,
    },
}

pub fn handle(
    output: &Output,
    command: ReleaseCommand,
    vault: &VaultContext,
) -> Result<i32, String> {
    let vault_path = &vault.vault_path;
    let store = ReleaseStore::new(vault_path, &vault.project_root);
    match command {
        ReleaseCommand::Create {
            version,
            name,
            owner,
            actor,
            json,
        } => {
            let release = store.create(NewRelease {
                version,
                name,
                owner,
                actor,
            })?;
            if json || output.json {
                output.print_json(&release);
            } else {
                output.print_stdout(&format!(
                    "Created release {} for version {}.",
                    release.id, release.version
                ));
                output.print_stdout(&format!("Stage:   {}", release.stage));
                if let Some(owner) = &release.owner {
                    output.print_stdout(&format!("Owner:   {owner}"));
                }
                output.print_stdout(&format!(
                    "Next:    gantry release candidate add {} --artifact <app.ipa|app.aab|app.apk>",
                    release.id
                ));
            }
            Ok(0)
        }
        ReleaseCommand::Ensure {
            version,
            name,
            owner,
            actor,
            json,
        } => {
            let release = store.ensure_release(NewRelease {
                version,
                name,
                owner,
                actor,
            })?;
            if json || output.json {
                output.print_json(&release);
            } else {
                output.print_stdout(&format!(
                    "Release {} is ready for version {}.",
                    release.id, release.version
                ));
                output.print_stdout(&format!("Stage: {}", release.stage));
            }
            Ok(0)
        }
        ReleaseCommand::List { json } => {
            let releases = store.list()?;
            if json || output.json {
                output.print_json(&releases);
            } else if releases.is_empty() {
                output.print_stdout("No workflow releases found.");
                output.print_stdout("Create one with: gantry release create <version>");
            } else {
                output.print_stdout("Gantry workflow releases");
                for release in releases {
                    output.print_stdout(&format!(
                        "- {}  {}  {}  {} candidate(s)",
                        release.id,
                        release.version,
                        release.stage,
                        release.candidate_ids.len()
                    ));
                    if let Some(owner) = release.owner {
                        output.print_stdout(&format!("  Owner: {owner}"));
                    }
                    output.print_stdout(&format!("  Updated: {}", release.updated_at));
                }
            }
            Ok(0)
        }
        ReleaseCommand::Portfolio { json } => {
            let portfolio = store.portfolio()?;
            if json || output.json {
                output.print_json(&portfolio);
            } else {
                output.heading("Release portfolio");
                output.field(
                    "Releases",
                    format!(
                        "{} total · {} need attention",
                        portfolio.counts.total, portfolio.counts.needs_attention
                    ),
                );
                output.field(
                    "Schedule",
                    format!(
                        "{} overdue · {} due soon",
                        portfolio.counts.overdue, portfolio.counts.due_soon
                    ),
                );
                output.field(
                    "Decisions",
                    format!(
                        "{} awaiting approval · {} blocked / rejected",
                        portfolio.counts.awaiting_approval, portfolio.counts.blocked_or_rejected
                    ),
                );
                output.field(
                    "Risk",
                    format!(
                        "{} approved with risk · {} accepted · {} evidence expiring",
                        portfolio.counts.approved_with_risk,
                        portfolio.counts.accepted_risk,
                        portfolio.counts.expiring_evidence
                    ),
                );
                if portfolio.review_queue.is_empty() {
                    output.print_stdout(&format!(
                        "\n{} Review queue clear",
                        output.style("PASS", Some("green"), true)
                    ));
                    if portfolio.counts.total == 0 {
                        output.print_stdout("Next: gantry release create <version>");
                    }
                } else {
                    output.print_stdout("");
                    output.heading("Prioritized review queue");
                    for item in portfolio.review_queue {
                        let platform = item
                            .platform
                            .map(|value| format!(" · {}", value.to_string().to_ascii_uppercase()))
                            .unwrap_or_default();
                        output.print_stdout(&format!(
                            "\n{}  {}{}\n{}\nWhy: {}",
                            output.style(
                                &item.severity.to_ascii_uppercase(),
                                Some(if item.severity == "blocker" {
                                    "red"
                                } else {
                                    "yellow"
                                }),
                                true
                            ),
                            item.release_version,
                            platform,
                            item.title,
                            item.reason
                        ));
                        output.print_stdout(&format!("Next: {}", item.next_action));
                        output.print_stdout(&format!(
                            "Owner: {}",
                            item.owner.as_deref().unwrap_or("Unassigned")
                        ));
                    }
                }
            }
            Ok(0)
        }
        ReleaseCommand::Show { release, json } => {
            let view = store.load(&release)?;
            if json || output.json {
                output.print_json(&view);
            } else {
                print_release_view(output, &view);
            }
            Ok(0)
        }
        ReleaseCommand::Schedule {
            release,
            target_date,
            submission_date,
            rollout_date,
            actor,
            json,
        } => {
            let release = store.schedule(NewReleaseSchedule {
                release,
                target_date,
                submission_date,
                rollout_date,
                actor,
            })?;
            if json || output.json {
                output.print_json(&release);
            } else {
                output.print_stdout(&format!("Updated schedule for {}.", release.version));
                if let Some(date) = release.target_date {
                    output.print_stdout(&format!("Target:     {date}"));
                }
                if let Some(date) = release.submission_date {
                    output.print_stdout(&format!("Submission: {date}"));
                }
                if let Some(date) = release.rollout_date {
                    output.print_stdout(&format!("Rollout:    {date}"));
                }
            }
            Ok(0)
        }
        ReleaseCommand::CandidateAdd {
            release,
            artifact,
            platform,
            actor,
            json,
        } => {
            let candidate = store.add_candidate(NewCandidate {
                release,
                artifact_path: artifact,
                platform,
                actor,
            })?;
            if json || output.json {
                output.print_json(&candidate);
            } else {
                output.print_stdout(&format!(
                    "Bound {} candidate {} to release {}.",
                    candidate.platform, candidate.id, candidate.release_id
                ));
                output.print_stdout(&format!("SHA-256: {}", candidate.artifact_sha256));
                output.print_stdout(&format!(
                    "Size:     {} bytes",
                    candidate.artifact_size_bytes
                ));
                output.print_stdout(&format!(
                    "Next:     gantry release audit {} --platform {}",
                    candidate.release_id,
                    platform_cli_name(candidate.platform)
                ));
            }
            Ok(0)
        }
        ReleaseCommand::CandidateEnsure {
            release,
            artifact,
            platform,
            actor,
            json,
        } => {
            let candidate = store.ensure_candidate(NewCandidate {
                release,
                artifact_path: artifact,
                platform,
                actor,
            })?;
            if json || output.json {
                output.print_json(&candidate);
            } else {
                output.print_stdout(&format!(
                    "Candidate {} is active for {}.",
                    candidate.id, candidate.platform
                ));
                output.print_stdout(&format!("SHA-256: {}", candidate.artifact_sha256));
            }
            Ok(0)
        }
        ReleaseCommand::Audit {
            release,
            platform,
            actor,
            reuse_unchanged,
            ci,
            json,
        } => {
            let input = NewReleaseAudit {
                release,
                platform,
                actor,
            };
            let result = if reuse_unchanged {
                store.audit_reusing_unchanged(input)?
            } else {
                store.audit(input)?
            };
            let exit_code = if result.audit.decision == ReleaseAuditDecision::NoGo {
                1
            } else {
                0
            };
            if json || output.json {
                output.print_json(&result);
            } else {
                output.print_stdout(&format!(
                    "Recorded {} audit {} for candidate {}.",
                    result.audit.platform, result.audit.id, result.audit.candidate_id
                ));
                output.print_stdout(&format!("Decision:    {}", result.audit.decision));
                output.print_stdout(&format!(
                    "Results:     {} passed, {} blockers, {} warnings, {} manual, {} unavailable",
                    result.audit.counts.passed,
                    result.audit.counts.blockers,
                    result.audit.counts.warnings,
                    result.audit.counts.manual,
                    result.audit.counts.unavailable
                ));
                output.print_stdout(&format!("Fingerprint: {}", result.audit.report_fingerprint));
                output.print_stdout(&format!(
                    "Report:      .gantry/{}",
                    result.audit.report_path
                ));
                if let Some(cache_key) = &result.cache_key {
                    output.print_stdout(&format!(
                        "Cache:       {} ({})",
                        if result.cache_hit { "hit" } else { "miss" },
                        &cache_key[..12]
                    ));
                }
            }
            if ci {
                print_github_annotations(output, &result.report);
            }
            Ok(exit_code)
        }
        ReleaseCommand::EvidenceAdd {
            release,
            platform,
            finding,
            kind,
            title,
            issuer,
            path,
            url,
            expires_at,
            actor,
            json,
        } => {
            let result = store.add_evidence(NewReleaseEvidence {
                release,
                platform,
                finding_instance_id: finding,
                mobile_evidence_kind: kind,
                title,
                issuer,
                path,
                url,
                expires_at,
                actor,
            })?;
            if json || output.json {
                output.print_json(&result);
            } else {
                output.print_stdout(&format!(
                    "Linked evidence {} to finding {} on candidate {}.",
                    result.evidence.id, result.link.finding_instance_id, result.link.candidate_id
                ));
                output.print_stdout(&format!(
                    "Binding:    {} ({})",
                    result.link.mobile_evidence_kind,
                    result.evidence.artifact_sha.as_deref().unwrap_or("unknown")
                ));
                output.print_stdout(&format!(
                    "Next:       gantry release audit {} --platform {}",
                    result.link.release_id,
                    platform_cli_name(result.link.platform)
                ));
                output.print_stdout(
                    "The current audit decision is unchanged until the candidate is audited again.",
                );
            }
            Ok(0)
        }
        ReleaseCommand::FindingResolve {
            release,
            platform,
            finding,
            reason,
            evidence_ids,
            actor,
            json,
        } => {
            let resolution = store.resolve_finding(NewFindingResolution {
                release,
                platform,
                finding_instance_id: finding,
                reason,
                evidence_ids,
                actor,
            })?;
            if json || output.json {
                output.print_json(&resolution);
            } else {
                output.print_stdout(&format!(
                    "Recorded resolution {} for finding {}.",
                    resolution.id, resolution.finding_instance_id
                ));
                output.print_stdout(&format!("Actor:      {}", resolution.actor));
                output.print_stdout(&format!("Reason:     {}", resolution.reason));
                output.print_stdout(
                    "Audit decision unchanged: rerun the audit to verify deterministic findings.",
                );
            }
            Ok(0)
        }
        ReleaseCommand::Decide {
            release,
            platform,
            decided_by,
            role,
            rationale,
            decision,
            json,
        } => {
            let record = store.decide(NewHumanDecision {
                release,
                platform,
                decided_by,
                role,
                rationale,
                decision,
            })?;
            if json || output.json {
                output.print_json(&record);
            } else {
                let label = match record.decision {
                    HumanDecisionKind::Approved => "Approved",
                    HumanDecisionKind::Rejected => "Rejected",
                };
                output.print_stdout(&format!(
                    "{label} {} candidate {} with decision {}.",
                    record.platform, record.candidate_id, record.id
                ));
                output.print_stdout(&format!(
                    "Binding:    candidate {} · audit {} · report {}",
                    &record.candidate_sha256[..12],
                    record.audit_id,
                    &record.report_fingerprint[..12]
                ));
                output.print_stdout(&format!(
                    "Accountable: {} ({})",
                    record.decided_by, record.role
                ));
                output.print_stdout(&format!("Rationale:  {}", record.rationale));
                let view = store.load(&record.release_id)?;
                output.print_stdout(&format!("Stage:      {}", view.release.stage));
            }
            Ok(0)
        }
        ReleaseCommand::Waive {
            release,
            platform,
            finding,
            reason,
            approved_by,
            expires_at,
            json,
        } => {
            let waiver = store.waive_finding(NewReleaseWaiver {
                release,
                platform,
                finding_instance_id: finding,
                reason,
                approved_by,
                expires_at,
            })?;
            if json || output.json {
                output.print_json(&waiver);
            } else {
                output.print_stdout(&format!(
                    "Recorded waiver {} for finding {}.",
                    waiver.id, waiver.finding_instance_id
                ));
                output.print_stdout(&format!("Approver:   {}", waiver.approved_by));
                output.print_stdout(&format!("Expires:    {}", waiver.expires_at));
                output.print_stdout(&format!(
                    "Binding:    candidate {} · audit {} · policy {}",
                    &waiver.candidate_sha256[..12],
                    waiver.audit_id,
                    &waiver.policy_digest[..12]
                ));
                output.print_stdout(&format!("Reason:     {}", waiver.reason));
                output.print_stdout(
                    "Audit decision unchanged: waivers record accepted risk but never override a deterministic No-Go.",
                );
            }
            Ok(0)
        }
        ReleaseCommand::Packet { release, json } => {
            let artifacts = store.write_packet(&release)?;
            if json || output.json {
                output.print_json(&artifacts);
            } else {
                output.print_stdout(&format!(
                    "Wrote verified release packet {} for release {}.",
                    artifacts.packet.packet_id, artifacts.packet.release.id
                ));
                output.print_stdout(&format!("Digest:   {}", artifacts.packet.packet_digest));
                output.print_stdout(&format!("JSON:     .gantry/{}", artifacts.json_path));
                output.print_stdout(&format!("Markdown: .gantry/{}", artifacts.markdown_path));
                output.print_stdout(&format!("HTML:     .gantry/{}", artifacts.html_path));
                output.print_stdout("");
                output.print_stdout(&artifacts.handoff_summary);
            }
            Ok(0)
        }
        ReleaseCommand::Diff {
            release,
            platform,
            from,
            to,
            json,
        } => {
            let diff = store.diff_candidates(NewReleaseDiff {
                release,
                platform,
                from_candidate_id: from,
                to_candidate_id: to,
            })?;
            if json || output.json {
                output.print_json(&diff);
            } else {
                output.print_stdout(&format!(
                    "{} candidate diff: {} → {}",
                    diff.platform, diff.from_candidate.id, diff.to_candidate.id
                ));
                output.print_stdout(&format!(
                    "SHA-256: {} → {}",
                    &diff.from_candidate.artifact_sha256[..12],
                    &diff.to_candidate.artifact_sha256[..12]
                ));
                output.print_stdout(&format!(
                    "Size:    {:+} bytes · Git changed: {} · Policy changed: {}",
                    diff.artifact_size_delta_bytes, diff.git_commit_changed, diff.policy_changed
                ));
                output.print_stdout(&format!(
                    "Findings: {} introduced · {} resolved · {} changed",
                    diff.introduced_findings.len(),
                    diff.resolved_findings.len(),
                    diff.changed_findings.len()
                ));
                for finding in &diff.introduced_findings {
                    output.print_stdout(&format!(
                        "  + {} — {}",
                        finding.finding_instance_id, finding.title
                    ));
                }
                for finding in &diff.resolved_findings {
                    output.print_stdout(&format!(
                        "  - {} — {}",
                        finding.finding_instance_id, finding.title
                    ));
                }
                for finding in &diff.changed_findings {
                    output.print_stdout(&format!(
                        "  ~ {} — {}",
                        finding.finding_instance_id, finding.title
                    ));
                }
                output.print_stdout(&format!(
                    "Related verified events: {}",
                    diff.related_events.len()
                ));
            }
            Ok(0)
        }
        ReleaseCommand::Manifest { json } => print_legacy_manifest(output, json, vault_path),
        ReleaseCommand::History { json } => print_legacy_history(output, json, vault_path),
    }
}

fn print_github_annotations(output: &Output, report: &gantry_core::mobile::MobileAuditReport) {
    for finding in report
        .findings
        .iter()
        .filter(|finding| finding.status != gantry_core::mobile::MobileFindingStatus::Pass)
    {
        let level = if finding.status == gantry_core::mobile::MobileFindingStatus::Fail
            && finding.severity == gantry_core::mobile::MobileSeverity::Blocker
        {
            "error"
        } else {
            "warning"
        };
        let location = finding
            .locations
            .first()
            .map(|path| format!("file={},", github_property_escape(path)))
            .unwrap_or_default();
        output.print_stderr(&format!(
            "::{level} {location}title={} {}::{}",
            github_property_escape(&finding.finding_instance_id),
            github_property_escape(&finding.rule_id),
            github_data_escape(&format!(
                "{} — {}",
                finding.title,
                if finding.remediation.trim().is_empty() {
                    "Review the release requirement."
                } else {
                    &finding.remediation
                }
            ))
        ));
    }
}

fn github_property_escape(value: &str) -> String {
    github_data_escape(value)
        .replace(':', "%3A")
        .replace(',', "%2C")
}

fn github_data_escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn print_release_view(output: &Output, view: &ReleaseView) {
    let release = &view.release;
    output.print_stdout(&format!("Release {}", release.id));
    output.print_stdout(&format!("Version:    {}", release.version));
    output.print_stdout(&format!("Stage:      {}", release.stage));
    if let Some(name) = &release.name {
        output.print_stdout(&format!("Name:       {name}"));
    }
    if let Some(owner) = &release.owner {
        output.print_stdout(&format!("Owner:      {owner}"));
    }
    if let Some(date) = &release.target_date {
        output.print_stdout(&format!("Target:     {date}"));
    }
    if let Some(date) = &release.submission_date {
        output.print_stdout(&format!("Submission: {date}"));
    }
    if let Some(date) = &release.rollout_date {
        output.print_stdout(&format!("Rollout:    {date}"));
    }
    output.print_stdout(&format!("Created:    {}", release.created_at));
    output.print_stdout(&format!("Updated:    {}", release.updated_at));
    output.print_stdout(&format!("Candidates: {}", view.candidates.len()));
    for candidate in &view.candidates {
        let active = release
            .active_candidates
            .get(&candidate.platform)
            .is_some_and(|id| id == &candidate.id);
        output.print_stdout(&format!(
            "  - {}  {}  {}{}",
            candidate.id,
            candidate.platform,
            &candidate.artifact_sha256[..12],
            if active { "  active" } else { "" }
        ));
        output.print_stdout(&format!("    {}", candidate.artifact_path));
    }
    output.print_stdout(&format!("Audits:     {}", view.audits.len()));
    for audit in &view.audits {
        let active = release
            .active_audits
            .get(&audit.platform)
            .is_some_and(|id| id == &audit.id);
        output.print_stdout(&format!(
            "  - {}  {}  {}{}",
            audit.id,
            audit.platform,
            audit.decision,
            if active { "  active" } else { "  stale" }
        ));
        output.print_stdout(&format!(
            "    candidate {} · fingerprint {} · .gantry/{}",
            audit.candidate_id,
            &audit.report_fingerprint[..12],
            audit.report_path
        ));
    }
    output.print_stdout(&format!(
        "Evidence:   {} link(s)",
        view.evidence_links.len()
    ));
    for link in &view.evidence_links {
        let active = release
            .active_candidates
            .get(&link.platform)
            .is_some_and(|id| id == &link.candidate_id);
        output.print_stdout(&format!(
            "  - {}  {} → {}{}",
            link.evidence_id,
            link.mobile_evidence_kind,
            link.finding_instance_id,
            if active { "  active" } else { "  stale" }
        ));
    }
    output.print_stdout(&format!(
        "Resolved:   {} finding(s)",
        view.resolutions.len()
    ));
    for resolution in &view.resolutions {
        let active = release
            .active_candidates
            .get(&resolution.platform)
            .is_some_and(|id| id == &resolution.candidate_id);
        output.print_stdout(&format!(
            "  - {}  {} by {}{}",
            resolution.id,
            resolution.finding_instance_id,
            resolution.actor,
            if active { "  active" } else { "  stale" }
        ));
        output.print_stdout(&format!("    {}", resolution.reason));
    }
    output.print_stdout(&format!("Decisions:  {} record(s)", view.decisions.len()));
    for decision in &view.decisions {
        let active = release
            .active_decisions
            .get(&decision.platform)
            .is_some_and(|id| id == &decision.id);
        let label = match decision.decision {
            HumanDecisionKind::Approved => "approved",
            HumanDecisionKind::Rejected => "rejected",
        };
        output.print_stdout(&format!(
            "  - {}  {} {} by {}{}",
            decision.id,
            decision.platform,
            label,
            decision.decided_by,
            if active { "  active" } else { "  stale" }
        ));
        output.print_stdout(&format!(
            "    audit {} · report {} · {}",
            decision.audit_id,
            &decision.report_fingerprint[..12],
            decision.rationale
        ));
    }
    output.print_stdout(&format!("Waivers:    {} record(s)", view.waivers.len()));
    for waiver in &view.waivers {
        let active = release.active_waiver_ids.contains(&waiver.id);
        let expired = DateTime::parse_from_rfc3339(&waiver.expires_at)
            .is_ok_and(|expires_at| expires_at.with_timezone(&Utc) <= Utc::now());
        let lifecycle = if expired {
            "expired"
        } else if active {
            "active"
        } else {
            "stale"
        };
        output.print_stdout(&format!(
            "  - {}  {} by {}  {}",
            waiver.id, waiver.finding_instance_id, waiver.approved_by, lifecycle
        ));
        output.print_stdout(&format!(
            "    audit {} · expires {} · {}",
            waiver.audit_id, waiver.expires_at, waiver.reason
        ));
    }
    output.print_stdout(&format!(
        "Events:     {} (hash chain verified)",
        view.events.len()
    ));
    for event in &view.events {
        output.print_stdout(&format!(
            "  {}. {:?} at {}{}",
            event.sequence,
            event.kind,
            event.occurred_at,
            event
                .actor
                .as_ref()
                .map(|actor| format!(" by {actor}"))
                .unwrap_or_default()
        ));
    }
}

fn platform_cli_name(platform: MobilePlatform) -> &'static str {
    match platform {
        MobilePlatform::Ios => "ios",
        MobilePlatform::Android => "android",
    }
}

fn print_legacy_manifest(
    output: &Output,
    json: bool,
    vault_path: &std::path::Path,
) -> Result<i32, String> {
    let config_path = vault_path.join("config.json");
    if !config_path.exists() {
        return Err("config.json not found.".to_string());
    }
    let config_content = std::fs::read_to_string(&config_path)
        .map_err(|error| format!("Failed to read config.json: {error}"))?;
    let gantry_config: serde_json::Value = serde_json::from_str(&config_content)
        .map_err(|error| format!("Failed to parse config.json: {error}"))?;
    let release_version = gantry_config
        .get("version")
        .and_then(|value| value.as_str())
        .unwrap_or("1.0.0");
    let manifest_path = vault_path
        .join("releases")
        .join(format!("{release_version}.json"));
    if !manifest_path.exists() {
        return Err(format!(
            "No generated release manifest found for version '{release_version}'. Run 'gantry index' first."
        ));
    }
    let content = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("Failed to read release manifest: {error}"))?;
    if json || output.json {
        output.print_json_raw(&content);
    } else {
        let manifest: gantry_core::ReleaseManifest = serde_json::from_str(&content)
            .map_err(|error| format!("Failed to parse release manifest JSON: {error}"))?;
        output.print_stdout("Generated readiness manifest");
        output.print_stdout(&format!("Release:         {}", manifest.release));
        output.print_stdout(&format!("Commit:          {}", manifest.commit));
        output.print_stdout(&format!("Created At:      {}", manifest.created_at));
        output.print_stdout(&format!("Readiness:       {}", manifest.readiness));
        output.print_stdout(&format!(
            "Readiness Score: {}/100",
            manifest.readiness_score
        ));
        output.print_stdout(&format!("Evidence:        {}", manifest.evidence_ids.len()));
    }
    Ok(0)
}

fn print_legacy_history(
    output: &Output,
    json: bool,
    vault_path: &std::path::Path,
) -> Result<i32, String> {
    let releases_dir = vault_path.join("releases");
    let mut manifests = Vec::new();
    if releases_dir.is_dir() {
        let entries = std::fs::read_dir(&releases_dir)
            .map_err(|error| format!("Failed to read release manifests: {error}"))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|value| value.to_str()) == Some("json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(manifest) =
                        serde_json::from_str::<gantry_core::ReleaseManifest>(&content)
                    {
                        manifests.push(manifest);
                    }
                }
            }
        }
    }
    manifests.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    if json || output.json {
        output.print_json(&manifests);
    } else if manifests.is_empty() {
        output.print_stdout("No generated release manifests found.");
    } else {
        output.print_stdout("Generated readiness manifest history");
        for manifest in manifests {
            output.print_stdout(&format!(
                "- {}  {} ({}/100)  {}",
                manifest.release, manifest.readiness, manifest.readiness_score, manifest.created_at
            ));
        }
    }
    Ok(0)
}
