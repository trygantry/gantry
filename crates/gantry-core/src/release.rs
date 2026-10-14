//! Durable, local-first release workflow state.
//!
//! Release records intentionally live beside legacy generated release manifests without
//! replacing them. A workflow release uses `.gantry/releases/<release-id>/`, while the
//! legacy manifest remains `.gantry/releases/<version>.json`.

use chrono::{NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RELEASE_SCHEMA_VERSION: &str = "1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MobilePlatform {
    Ios,
    Android,
}

impl std::fmt::Display for MobilePlatform {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ios => write!(formatter, "iOS"),
            Self::Android => write!(formatter, "Android"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateArtifactKind {
    Ipa,
    Aab,
    Apk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseStage {
    Draft,
    Candidate,
    InReview,
    Approved,
    Rejected,
    Submitted,
    RollingOut,
    Released,
    Halted,
    Superseded,
}

impl std::fmt::Display for ReleaseStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Draft => "Draft",
            Self::Candidate => "Candidate",
            Self::InReview => "In review",
            Self::Approved => "Approved",
            Self::Rejected => "Rejected",
            Self::Submitted => "Submitted",
            Self::RollingOut => "Rolling out",
            Self::Released => "Released",
            Self::Halted => "Halted",
            Self::Superseded => "Superseded",
        };
        write!(formatter, "{value}")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MobileRelease {
    pub schema_version: String,
    pub id: String,
    pub version: String,
    pub name: Option<String>,
    pub owner: Option<String>,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default)]
    pub submission_date: Option<String>,
    #[serde(default)]
    pub rollout_date: Option<String>,
    pub stage: ReleaseStage,
    pub created_at: String,
    pub updated_at: String,
    pub candidate_ids: Vec<String>,
    pub active_candidates: BTreeMap<MobilePlatform, String>,
    #[serde(default)]
    pub audit_ids: Vec<String>,
    #[serde(default)]
    pub active_audits: BTreeMap<MobilePlatform, String>,
    #[serde(default)]
    pub evidence_link_ids: Vec<String>,
    #[serde(default)]
    pub resolution_ids: Vec<String>,
    #[serde(default)]
    pub decision_ids: Vec<String>,
    #[serde(default)]
    pub active_decisions: BTreeMap<MobilePlatform, String>,
    #[serde(default)]
    pub waiver_ids: Vec<String>,
    #[serde(default)]
    pub active_waiver_ids: Vec<String>,
    pub latest_event_sequence: u64,
    pub latest_event_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReleaseCandidate {
    pub schema_version: String,
    pub id: String,
    pub release_id: String,
    pub platform: MobilePlatform,
    pub artifact_kind: CandidateArtifactKind,
    pub artifact_path: String,
    pub artifact_path_relative: bool,
    pub artifact_sha256: String,
    pub artifact_size_bytes: u64,
    pub git_commit: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseAuditDecision {
    Go,
    AtRisk,
    NoGo,
}

impl std::fmt::Display for ReleaseAuditDecision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Go => write!(formatter, "Go"),
            Self::AtRisk => write!(formatter, "At Risk"),
            Self::NoGo => write!(formatter, "No-Go"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAuditCounts {
    pub passed: usize,
    pub blockers: usize,
    pub warnings: usize,
    pub manual: usize,
    pub unavailable: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAuditRecord {
    pub schema_version: String,
    pub id: String,
    pub release_id: String,
    pub candidate_id: String,
    pub platform: MobilePlatform,
    pub candidate_sha256: String,
    pub report_fingerprint: String,
    pub policy_digest: String,
    pub decision: ReleaseAuditDecision,
    pub counts: ReleaseAuditCounts,
    pub git_commit: String,
    pub generated_at: String,
    pub recorded_at: String,
    pub report_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseEvidenceLink {
    pub schema_version: String,
    pub id: String,
    pub evidence_id: String,
    pub evidence_digest: String,
    pub release_id: String,
    pub candidate_id: String,
    pub source_audit_id: String,
    pub platform: MobilePlatform,
    pub finding_instance_id: String,
    pub mobile_evidence_kind: String,
    pub actor: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingResolutionStatus {
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingResolution {
    pub schema_version: String,
    pub id: String,
    pub release_id: String,
    pub candidate_id: String,
    pub source_audit_id: String,
    pub platform: MobilePlatform,
    pub finding_instance_id: String,
    pub status: FindingResolutionStatus,
    pub reason: String,
    pub evidence_ids: Vec<String>,
    pub actor: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanDecisionKind {
    Approved,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanReleaseDecision {
    pub schema_version: String,
    pub id: String,
    pub release_id: String,
    pub candidate_id: String,
    pub candidate_sha256: String,
    pub audit_id: String,
    pub platform: MobilePlatform,
    pub report_fingerprint: String,
    pub policy_digest: String,
    pub decision: HumanDecisionKind,
    pub decided_by: String,
    pub role: String,
    pub rationale: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseWaiverStatus {
    Active,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseFindingWaiver {
    pub schema_version: String,
    pub id: String,
    pub release_id: String,
    pub candidate_id: String,
    pub candidate_sha256: String,
    pub audit_id: String,
    pub platform: MobilePlatform,
    pub report_fingerprint: String,
    pub policy_digest: String,
    pub finding_instance_id: String,
    pub reason: String,
    pub approved_by: String,
    pub created_at: String,
    pub expires_at: String,
    pub status: ReleaseWaiverStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseEventKind {
    ReleaseCreated,
    ReleaseScheduled,
    CandidateBound,
    CandidateAudited,
    EvidenceLinked,
    FindingResolved,
    ReleaseApproved,
    ReleaseRejected,
    FindingWaived,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReleaseEvent {
    pub schema_version: String,
    pub release_id: String,
    pub sequence: u64,
    pub occurred_at: String,
    pub kind: ReleaseEventKind,
    pub actor: Option<String>,
    pub details: Value,
    pub previous_hash: Option<String>,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReleaseView {
    pub release: MobileRelease,
    pub candidates: Vec<ReleaseCandidate>,
    pub audits: Vec<ReleaseAuditRecord>,
    pub evidence_links: Vec<ReleaseEvidenceLink>,
    pub resolutions: Vec<FindingResolution>,
    pub decisions: Vec<HumanReleaseDecision>,
    pub waivers: Vec<ReleaseFindingWaiver>,
    pub events: Vec<ReleaseEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseReviewKind {
    Overdue,
    DueSoon,
    MissingOwner,
    MissingCandidate,
    MissingAudit,
    Blocked,
    ApprovalRequired,
    Rejected,
    ApprovedWithRisk,
    AcceptedRisk,
    ExpiringEvidence,
    ExpiringWaiver,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseReviewItem {
    pub kind: ReleaseReviewKind,
    pub release_id: String,
    pub release_version: String,
    pub release_name: Option<String>,
    pub owner: Option<String>,
    pub platform: Option<MobilePlatform>,
    pub severity: String,
    pub title: String,
    pub reason: String,
    pub next_action: String,
    pub due_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasePortfolioItem {
    pub id: String,
    pub version: String,
    pub name: Option<String>,
    pub owner: Option<String>,
    pub target_date: Option<String>,
    pub submission_date: Option<String>,
    pub rollout_date: Option<String>,
    pub days_to_target: Option<i64>,
    pub stage: ReleaseStage,
    pub updated_at: String,
    pub age_days: i64,
    pub platforms: Vec<MobilePlatform>,
    pub automated_decision: Option<ReleaseAuditDecision>,
    pub counts: ReleaseAuditCounts,
    pub active_decisions: usize,
    pub active_waivers: usize,
    pub expiring_evidence: usize,
    pub expiring_waivers: usize,
    pub needs_attention: bool,
    pub next_action: String,
    pub review_kinds: Vec<ReleaseReviewKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasePortfolioCounts {
    pub total: usize,
    pub needs_attention: usize,
    pub overdue: usize,
    pub due_soon: usize,
    pub awaiting_approval: usize,
    pub blocked_or_rejected: usize,
    pub approved_with_risk: usize,
    pub accepted_risk: usize,
    pub expiring_evidence: usize,
    pub expiring_waivers: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasePortfolio {
    pub schema_version: String,
    pub generated_at: String,
    pub counts: ReleasePortfolioCounts,
    pub releases: Vec<ReleasePortfolioItem>,
    pub review_queue: Vec<ReleaseReviewItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReleasePacketPlatform {
    pub platform: MobilePlatform,
    pub candidate_id: String,
    pub candidate_sha256: String,
    pub audit_id: Option<String>,
    pub automated_decision: Option<ReleaseAuditDecision>,
    pub report_fingerprint: Option<String>,
    pub policy_digest: Option<String>,
    pub human_decision_id: Option<String>,
    pub human_decision: Option<HumanDecisionKind>,
    pub context_waiver_ids: Vec<String>,
    pub effective_waiver_ids: Vec<String>,
    pub expired_waiver_ids: Vec<String>,
    pub unresolved_finding_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleasePacket {
    pub schema_version: String,
    pub packet_id: String,
    pub packet_digest: String,
    pub assembled_at: String,
    pub release: MobileRelease,
    pub active_platforms: Vec<ReleasePacketPlatform>,
    pub candidates: Vec<ReleaseCandidate>,
    pub audits: Vec<ReleaseAuditRecord>,
    pub reports: Vec<crate::mobile::MobileAuditReport>,
    pub evidence_links: Vec<ReleaseEvidenceLink>,
    pub resolutions: Vec<FindingResolution>,
    pub decisions: Vec<HumanReleaseDecision>,
    pub waivers: Vec<ReleaseFindingWaiver>,
    pub events: Vec<ReleaseEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleasePacketArtifacts {
    pub schema_version: String,
    pub packet: ReleasePacket,
    pub json_path: String,
    pub markdown_path: String,
    pub html_path: String,
    pub handoff_summary: String,
}

#[derive(Debug, Clone)]
pub struct NewReleaseDiff {
    pub release: String,
    pub platform: Option<MobilePlatform>,
    pub from_candidate_id: Option<String>,
    pub to_candidate_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseFindingDelta {
    pub finding_instance_id: String,
    pub title: String,
    pub from_status: Option<crate::mobile::MobileFindingStatus>,
    pub to_status: Option<crate::mobile::MobileFindingStatus>,
    pub from_severity: Option<crate::mobile::MobileSeverity>,
    pub to_severity: Option<crate::mobile::MobileSeverity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReleaseCandidateDiff {
    pub schema_version: String,
    pub release_id: String,
    pub platform: MobilePlatform,
    pub from_candidate: ReleaseCandidate,
    pub to_candidate: ReleaseCandidate,
    pub artifact_size_delta_bytes: i128,
    pub git_commit_changed: bool,
    pub from_audit: Option<ReleaseAuditRecord>,
    pub to_audit: Option<ReleaseAuditRecord>,
    pub automated_decision_changed: bool,
    pub policy_changed: bool,
    pub introduced_findings: Vec<ReleaseFindingDelta>,
    pub resolved_findings: Vec<ReleaseFindingDelta>,
    pub changed_findings: Vec<ReleaseFindingDelta>,
    pub related_events: Vec<ReleaseEvent>,
}

#[derive(Debug, Clone, Default)]
pub struct NewRelease {
    pub version: String,
    pub name: Option<String>,
    pub owner: Option<String>,
    pub actor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewReleaseSchedule {
    pub release: String,
    pub target_date: Option<String>,
    pub submission_date: Option<String>,
    pub rollout_date: Option<String>,
    pub actor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewCandidate {
    pub release: String,
    pub artifact_path: PathBuf,
    pub platform: Option<MobilePlatform>,
    pub actor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewReleaseAudit {
    pub release: String,
    pub platform: Option<MobilePlatform>,
    pub actor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseAuditResult {
    pub schema_version: String,
    pub cache_hit: bool,
    pub cache_key: Option<String>,
    pub cache_path: Option<String>,
    pub audit: ReleaseAuditRecord,
    pub report: crate::mobile::MobileAuditReport,
}

#[derive(Debug, Clone)]
pub struct NewReleaseEvidence {
    pub release: String,
    pub platform: Option<MobilePlatform>,
    pub finding_instance_id: String,
    pub mobile_evidence_kind: String,
    pub title: String,
    pub issuer: String,
    pub path: Option<String>,
    pub url: Option<String>,
    pub expires_at: Option<String>,
    pub actor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseEvidenceResult {
    pub schema_version: String,
    pub link: ReleaseEvidenceLink,
    pub evidence: crate::EvidenceRecord,
}

#[derive(Debug, Clone)]
pub struct NewFindingResolution {
    pub release: String,
    pub platform: Option<MobilePlatform>,
    pub finding_instance_id: String,
    pub reason: String,
    pub evidence_ids: Vec<String>,
    pub actor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewHumanDecision {
    pub release: String,
    pub platform: Option<MobilePlatform>,
    pub decided_by: String,
    pub role: String,
    pub rationale: String,
    pub decision: HumanDecisionKind,
}

#[derive(Debug, Clone)]
pub struct NewReleaseWaiver {
    pub release: String,
    pub platform: Option<MobilePlatform>,
    pub finding_instance_id: String,
    pub reason: String,
    pub approved_by: String,
    pub expires_at: String,
}

#[derive(Debug, Clone)]
pub struct ReleaseStore {
    vault_path: PathBuf,
    project_root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultHealthReport {
    pub schema_version: String,
    pub healthy: bool,
    pub repaired_transactions: usize,
    pub pending_transactions: usize,
    pub verified_releases: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultMigrationReport {
    pub schema_version: String,
    pub dry_run: bool,
    pub current_release_schema: String,
    pub migrations_required: usize,
    pub migrated_releases: usize,
    pub backup_path: Option<String>,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReleaseTransactionManifest {
    schema_version: String,
    transaction_id: String,
    release_id: String,
    created_at: String,
    writes: Vec<ReleaseTransactionWrite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReleaseTransactionWrite {
    target: String,
    staged_file: String,
    sha256: String,
}

struct ReleaseCommitLock {
    path: PathBuf,
}

impl Drop for ReleaseCommitLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        if let Some(parent) = self.path.parent() {
            let _ = sync_directory(parent);
        }
    }
}

impl ReleaseStore {
    pub fn new(vault_path: impl Into<PathBuf>, project_root: impl Into<PathBuf>) -> Self {
        Self {
            vault_path: vault_path.into(),
            project_root: project_root.into(),
        }
    }

    /// Verify every durable release and optionally finish any transaction interrupted
    /// after its journal was made durable. Repair only rolls a fully checksummed journal
    /// forward; it never invents or discards governance state.
    pub fn check_vault(&self, repair: bool) -> Result<VaultHealthReport, String> {
        self.require_vault()?;
        let pending_before = self.pending_transaction_count()?;
        let repaired_transactions = if repair && pending_before > 0 {
            let _lock = self.acquire_commit_lock()?;
            self.recover_pending_transactions_locked()?
        } else {
            0
        };
        let pending_transactions = self.pending_transaction_count()?;
        let mut errors = Vec::new();
        let releases = self.list_without_recovery()?;
        let mut verified_releases = 0;
        for release in releases {
            match self.load_without_recovery(&release) {
                Ok(_) => verified_releases += 1,
                Err(error) => errors.push(format!("{} ({}): {error}", release.version, release.id)),
            }
        }
        if pending_transactions > 0 {
            errors.push(format!(
                "{pending_transactions} recoverable transaction(s) are pending; run `gantry vault check --repair`."
            ));
        }
        Ok(VaultHealthReport {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            healthy: errors.is_empty(),
            repaired_transactions,
            pending_transactions,
            verified_releases,
            errors,
        })
    }

    /// Plan or apply supported release-record migrations. Gantry 0.4 writes release
    /// schema 1.0, so the current implementation is deliberately a verified no-op for
    /// compatible vaults and a fail-closed error for unknown versions.
    pub fn migrate_vault(&self, dry_run: bool) -> Result<VaultMigrationReport, String> {
        let health = self.check_vault(false)?;
        if !health.healthy {
            return Err(format!(
                "Vault migration requires a healthy source vault: {}",
                health.errors.join("; ")
            ));
        }
        let releases = self.list_without_recovery()?;
        let mut unsupported = Vec::new();
        for release in &releases {
            if release.schema_version != RELEASE_SCHEMA_VERSION {
                unsupported.push(format!(
                    "{} uses unsupported schema {}",
                    release.id, release.schema_version
                ));
            }
        }
        if !unsupported.is_empty() {
            return Err(format!(
                "No safe migration is registered for this vault: {}",
                unsupported.join("; ")
            ));
        }
        Ok(VaultMigrationReport {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            dry_run,
            current_release_schema: RELEASE_SCHEMA_VERSION.to_string(),
            migrations_required: 0,
            migrated_releases: 0,
            backup_path: None,
            messages: vec![
                "All durable release records already use the current schema; no files were changed."
                    .to_string(),
                "Future registered migrations will create a timestamped vault backup before publication."
                    .to_string(),
            ],
        })
    }

    pub fn create(&self, input: NewRelease) -> Result<MobileRelease, String> {
        self.require_vault()?;
        let version = required_trimmed("version", &input.version)?;
        let name = optional_trimmed(input.name);
        let owner = optional_trimmed(input.owner);
        let actor = optional_trimmed(input.actor).or_else(current_actor);

        if self
            .list()?
            .iter()
            .any(|release| release.version == version)
        {
            return Err(format!(
                "A release already exists for version '{version}'. Use `gantry release show {version}`."
            ));
        }

        let now = timestamp();
        let id = generated_id("REL", &[&version, &now]);
        let release_dir = self.release_dir(&id);
        fs::create_dir_all(release_dir.join("candidates"))
            .map_err(|error| format!("Failed to create release directory: {error}"))?;

        let mut release = MobileRelease {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            id: id.clone(),
            version: version.clone(),
            name,
            owner,
            target_date: None,
            submission_date: None,
            rollout_date: None,
            stage: ReleaseStage::Draft,
            created_at: now.clone(),
            updated_at: now.clone(),
            candidate_ids: Vec::new(),
            active_candidates: BTreeMap::new(),
            audit_ids: Vec::new(),
            active_audits: BTreeMap::new(),
            evidence_link_ids: Vec::new(),
            resolution_ids: Vec::new(),
            decision_ids: Vec::new(),
            active_decisions: BTreeMap::new(),
            waiver_ids: Vec::new(),
            active_waiver_ids: Vec::new(),
            latest_event_sequence: 0,
            latest_event_hash: None,
        };
        let event = build_event(
            &release,
            ReleaseEventKind::ReleaseCreated,
            actor,
            json!({ "version": version, "name": release.name, "owner": release.owner }),
        )?;
        apply_event_head(&mut release, &event);
        self.commit_release_transaction(
            &release.id,
            None,
            Some(&release.version),
            vec![
                (release_dir.join("events.jsonl"), events_bytes(&[event])?),
                (release_dir.join("release.json"), json_bytes(&release)?),
            ],
        )?;
        Ok(release)
    }

    pub fn ensure_release(&self, input: NewRelease) -> Result<MobileRelease, String> {
        let version = required_trimmed("version", &input.version)?;
        if let Some(existing) = self
            .list()?
            .into_iter()
            .find(|release| release.version == version)
        {
            return self.load(&existing.id).map(|view| view.release);
        }
        self.create(NewRelease { version, ..input })
    }

    pub fn schedule(&self, input: NewReleaseSchedule) -> Result<MobileRelease, String> {
        let mut view = self.load(&input.release)?;
        let expected_event_head = view.release.latest_event_hash.clone();
        if matches!(
            view.release.stage,
            ReleaseStage::Released | ReleaseStage::Superseded
        ) {
            return Err(format!(
                "Release {} is {} and its schedule is final.",
                view.release.id, view.release.stage
            ));
        }
        if input.target_date.is_none()
            && input.submission_date.is_none()
            && input.rollout_date.is_none()
        {
            return Err("Provide at least one release schedule date.".to_string());
        }
        let target_date = validate_optional_date("target date", input.target_date)?;
        let submission_date = validate_optional_date("submission date", input.submission_date)?;
        let rollout_date = validate_optional_date("rollout date", input.rollout_date)?;
        let actor = required_actor(input.actor)?;
        if let Some(value) = target_date {
            view.release.target_date = Some(value);
        }
        if let Some(value) = submission_date {
            view.release.submission_date = Some(value);
        }
        if let Some(value) = rollout_date {
            view.release.rollout_date = Some(value);
        }
        let now = timestamp();
        view.release.updated_at = now;
        let event = build_event(
            &view.release,
            ReleaseEventKind::ReleaseScheduled,
            Some(actor),
            json!({
                "target_date": view.release.target_date,
                "submission_date": view.release.submission_date,
                "rollout_date": view.release.rollout_date,
            }),
        )?;
        apply_event_head(&mut view.release, &event);
        view.events.push(event);
        let release_dir = self.release_dir(&view.release.id);
        self.commit_release_transaction(
            &view.release.id,
            expected_event_head.as_deref(),
            None,
            vec![
                (
                    release_dir.join("events.jsonl"),
                    events_bytes(&view.events)?,
                ),
                (release_dir.join("release.json"), json_bytes(&view.release)?),
            ],
        )?;
        Ok(view.release)
    }

    pub fn list(&self) -> Result<Vec<MobileRelease>, String> {
        self.recover_if_needed()?;
        self.list_without_recovery()
    }

    fn list_without_recovery(&self) -> Result<Vec<MobileRelease>, String> {
        self.require_vault()?;
        let root = self.releases_root();
        if !root.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&root)
            .map_err(|error| format!("Failed to read {}: {error}", root.display()))?;
        let mut releases = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| format!("Failed to read release entry: {error}"))?;
            if !entry
                .file_type()
                .map_err(|error| format!("Failed to inspect release entry: {error}"))?
                .is_dir()
            {
                continue;
            }
            let path = entry.path().join("release.json");
            if path.is_file() {
                releases.push(read_json(&path)?);
            }
        }
        releases.sort_by(|left: &MobileRelease, right| right.updated_at.cmp(&left.updated_at));
        Ok(releases)
    }

    pub fn portfolio(&self) -> Result<ReleasePortfolio, String> {
        self.portfolio_at(Utc::now())
    }

    fn portfolio_at(&self, now: chrono::DateTime<Utc>) -> Result<ReleasePortfolio, String> {
        let mut releases = Vec::new();
        let mut review_queue = Vec::new();
        for release in self.list()? {
            let view = self.load(&release.id)?;
            let evidence = view
                .evidence_links
                .iter()
                .map(|link| {
                    read_json(
                        &self
                            .vault_path
                            .join("evidence")
                            .join(format!("{}.json", link.evidence_id)),
                    )
                })
                .collect::<Result<Vec<crate::EvidenceRecord>, String>>()?;
            let (item, mut queue) = release_portfolio_item(&view, &evidence, now);
            releases.push(item);
            review_queue.append(&mut queue);
        }
        releases.sort_by(|left, right| {
            right
                .needs_attention
                .cmp(&left.needs_attention)
                .then_with(|| right.updated_at.cmp(&left.updated_at))
        });
        review_queue.sort_by(|left, right| {
            review_severity_rank(&left.severity)
                .cmp(&review_severity_rank(&right.severity))
                .then_with(|| left.due_at.cmp(&right.due_at))
                .then_with(|| left.release_version.cmp(&right.release_version))
        });
        let counts = ReleasePortfolioCounts {
            total: releases.len(),
            needs_attention: releases
                .iter()
                .filter(|release| release.needs_attention)
                .count(),
            overdue: review_queue
                .iter()
                .filter(|item| item.kind == ReleaseReviewKind::Overdue)
                .count(),
            due_soon: review_queue
                .iter()
                .filter(|item| item.kind == ReleaseReviewKind::DueSoon)
                .count(),
            awaiting_approval: review_queue
                .iter()
                .filter(|item| item.kind == ReleaseReviewKind::ApprovalRequired)
                .count(),
            blocked_or_rejected: review_queue
                .iter()
                .filter(|item| {
                    matches!(
                        item.kind,
                        ReleaseReviewKind::Blocked | ReleaseReviewKind::Rejected
                    )
                })
                .count(),
            approved_with_risk: review_queue
                .iter()
                .filter(|item| item.kind == ReleaseReviewKind::ApprovedWithRisk)
                .count(),
            accepted_risk: review_queue
                .iter()
                .filter(|item| item.kind == ReleaseReviewKind::AcceptedRisk)
                .count(),
            expiring_evidence: review_queue
                .iter()
                .filter(|item| item.kind == ReleaseReviewKind::ExpiringEvidence)
                .count(),
            expiring_waivers: review_queue
                .iter()
                .filter(|item| item.kind == ReleaseReviewKind::ExpiringWaiver)
                .count(),
        };
        Ok(ReleasePortfolio {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            generated_at: now.to_rfc3339_opts(SecondsFormat::Secs, true),
            counts,
            releases,
            review_queue,
        })
    }

    pub fn load(&self, release_reference: &str) -> Result<ReleaseView, String> {
        self.recover_if_needed()?;
        let release = self.resolve_release(release_reference)?;
        self.load_without_recovery(&release)
    }

    fn load_without_recovery(&self, release: &MobileRelease) -> Result<ReleaseView, String> {
        let release_dir = self.release_dir(&release.id);
        let events = read_events(&release_dir.join("events.jsonl"))?;
        verify_event_chain(&events)?;
        if events.last().map(|event| event.hash.as_str()) != release.latest_event_hash.as_deref() {
            return Err(format!(
                "Release {} event head does not match release.json; the release record may be incomplete or modified.",
                release.id
            ));
        }
        verify_release_schedule(release, &events)?;

        let mut candidates = Vec::new();
        for candidate_id in &release.candidate_ids {
            let path = release_dir
                .join("candidates")
                .join(format!("{candidate_id}.json"));
            candidates.push(read_json(&path)?);
        }
        let mut audits = Vec::new();
        for audit_id in &release.audit_ids {
            let path = release_dir.join("audits").join(format!("{audit_id}.json"));
            audits.push(read_json(&path)?);
        }
        verify_active_audits(release, &audits)?;
        for audit in &audits {
            verify_stored_audit(&self.vault_path, release, &candidates, audit)?;
        }
        let mut evidence_links = Vec::new();
        for link_id in &release.evidence_link_ids {
            let path = release_dir.join("evidence").join(format!("{link_id}.json"));
            evidence_links.push(read_json(&path)?);
        }
        for link in &evidence_links {
            verify_evidence_link(
                &self.vault_path,
                release,
                &candidates,
                &audits,
                &events,
                link,
            )?;
        }
        let mut resolutions = Vec::new();
        for resolution_id in &release.resolution_ids {
            let path = release_dir
                .join("resolutions")
                .join(format!("{resolution_id}.json"));
            resolutions.push(read_json(&path)?);
        }
        for resolution in &resolutions {
            verify_finding_resolution(
                release,
                &candidates,
                &audits,
                &evidence_links,
                &events,
                resolution,
            )?;
        }
        let mut decisions = Vec::new();
        for decision_id in &release.decision_ids {
            let path = release_dir
                .join("decisions")
                .join(format!("{decision_id}.json"));
            decisions.push(read_json(&path)?);
        }
        for decision in &decisions {
            verify_human_decision(release, &candidates, &audits, &events, decision)?;
        }
        verify_active_decisions(release, &decisions)?;
        let mut waivers = Vec::new();
        for waiver_id in &release.waiver_ids {
            let path = release_dir
                .join("waivers")
                .join(format!("{waiver_id}.json"));
            waivers.push(read_json(&path)?);
        }
        for waiver in &waivers {
            verify_release_waiver(release, &candidates, &audits, &events, waiver)?;
        }
        verify_active_waivers(release, &waivers)?;
        Ok(ReleaseView {
            release: release.clone(),
            candidates,
            audits,
            evidence_links,
            resolutions,
            decisions,
            waivers,
            events,
        })
    }

    pub fn add_candidate(&self, input: NewCandidate) -> Result<ReleaseCandidate, String> {
        let mut view = self.load(&input.release)?;
        let expected_event_head = view.release.latest_event_hash.clone();
        if matches!(
            view.release.stage,
            ReleaseStage::Released | ReleaseStage::Superseded
        ) {
            return Err(format!(
                "Release {} is {} and cannot accept another candidate.",
                view.release.id, view.release.stage
            ));
        }

        let artifact_path = input.artifact_path.canonicalize().map_err(|error| {
            format!(
                "Failed to resolve artifact {}: {error}",
                input.artifact_path.display()
            )
        })?;
        if !artifact_path.is_file() {
            return Err(format!(
                "Artifact is not a file: {}",
                artifact_path.display()
            ));
        }
        let (inferred_platform, artifact_kind) = classify_artifact(&artifact_path)?;
        if let Some(requested) = input.platform {
            if requested != inferred_platform {
                return Err(format!(
                    "Artifact type implies {inferred_platform}, but --platform selected {requested}."
                ));
            }
        }
        let platform = input.platform.unwrap_or(inferred_platform);
        let (artifact_sha256, artifact_size_bytes) = hash_artifact(&artifact_path)?;
        let now = timestamp();
        let id = generated_id("CAN", &[&view.release.id, &artifact_sha256, &now]);
        let (artifact_path, artifact_path_relative) =
            portable_artifact_path(&artifact_path, &self.project_root);
        let candidate = ReleaseCandidate {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            id: id.clone(),
            release_id: view.release.id.clone(),
            platform,
            artifact_kind,
            artifact_path,
            artifact_path_relative,
            artifact_sha256: artifact_sha256.clone(),
            artifact_size_bytes,
            git_commit: crate::get_current_git_commit_ctx(&self.project_root),
            created_at: now.clone(),
        };

        let previous_candidate_id = view.release.active_candidates.insert(platform, id.clone());
        let invalidated_audit_id = view.release.active_audits.remove(&platform);
        let invalidated_decision_id = view.release.active_decisions.remove(&platform);
        let invalidated_waiver_ids = view
            .waivers
            .iter()
            .filter(|waiver| {
                waiver.platform == platform && view.release.active_waiver_ids.contains(&waiver.id)
            })
            .map(|waiver| waiver.id.clone())
            .collect::<Vec<_>>();
        view.release
            .active_waiver_ids
            .retain(|waiver_id| !invalidated_waiver_ids.contains(waiver_id));
        let invalidated_evidence_link_ids = previous_candidate_id
            .as_ref()
            .map(|candidate_id| {
                view.evidence_links
                    .iter()
                    .filter(|link| &link.candidate_id == candidate_id)
                    .map(|link| link.id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let invalidated_resolution_ids = previous_candidate_id
            .as_ref()
            .map(|candidate_id| {
                view.resolutions
                    .iter()
                    .filter(|resolution| &resolution.candidate_id == candidate_id)
                    .map(|resolution| resolution.id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        view.release.candidate_ids.push(id.clone());
        view.release.stage =
            stage_after_context_change(&view.release, &view.decisions, ReleaseStage::Candidate);
        view.release.updated_at = now;
        let event = build_event(
            &view.release,
            ReleaseEventKind::CandidateBound,
            optional_trimmed(input.actor).or_else(current_actor),
            json!({
                "candidate_id": id,
                "platform": platform,
                "artifact_kind": artifact_kind,
                "artifact_sha256": artifact_sha256,
                "artifact_size_bytes": artifact_size_bytes,
                "previous_candidate_id": previous_candidate_id,
                "invalidated_audit_id": invalidated_audit_id,
                "invalidated_decision_id": invalidated_decision_id,
                "invalidated_waiver_ids": invalidated_waiver_ids,
                "invalidated_evidence_link_ids": invalidated_evidence_link_ids,
                "invalidated_resolution_ids": invalidated_resolution_ids,
            }),
        )?;
        apply_event_head(&mut view.release, &event);
        view.events.push(event);

        let release_dir = self.release_dir(&view.release.id);
        self.commit_release_transaction(
            &view.release.id,
            expected_event_head.as_deref(),
            None,
            vec![
                (
                    release_dir
                        .join("candidates")
                        .join(format!("{}.json", candidate.id)),
                    json_bytes(&candidate)?,
                ),
                (
                    release_dir.join("events.jsonl"),
                    events_bytes(&view.events)?,
                ),
                (release_dir.join("release.json"), json_bytes(&view.release)?),
            ],
        )?;
        Ok(candidate)
    }

    pub fn ensure_candidate(&self, input: NewCandidate) -> Result<ReleaseCandidate, String> {
        let view = self.load(&input.release)?;
        let artifact_path = input.artifact_path.canonicalize().map_err(|error| {
            format!(
                "Failed to resolve artifact {}: {error}",
                input.artifact_path.display()
            )
        })?;
        if !artifact_path.is_file() {
            return Err(format!(
                "Artifact is not a file: {}",
                artifact_path.display()
            ));
        }
        let (inferred_platform, artifact_kind) = classify_artifact(&artifact_path)?;
        if let Some(requested) = input.platform
            && requested != inferred_platform
        {
            return Err(format!(
                "Artifact type implies {inferred_platform}, but --platform selected {requested}."
            ));
        }
        let platform = input.platform.unwrap_or(inferred_platform);
        let (artifact_sha256, artifact_size_bytes) = hash_artifact(&artifact_path)?;
        let git_commit = crate::get_current_git_commit_ctx(&self.project_root);
        if let Some(active_id) = view.release.active_candidates.get(&platform)
            && let Some(candidate) = view.candidates.iter().find(|item| &item.id == active_id)
            && candidate.artifact_kind == artifact_kind
            && candidate.artifact_sha256 == artifact_sha256
            && candidate.artifact_size_bytes == artifact_size_bytes
            && candidate.git_commit == git_commit
        {
            return Ok(candidate.clone());
        }
        self.add_candidate(input)
    }

    pub fn audit(&self, input: NewReleaseAudit) -> Result<ReleaseAuditResult, String> {
        self.audit_internal(input, false)
    }

    pub fn audit_reusing_unchanged(
        &self,
        input: NewReleaseAudit,
    ) -> Result<ReleaseAuditResult, String> {
        self.audit_internal(input, true)
    }

    fn audit_internal(
        &self,
        input: NewReleaseAudit,
        reuse_unchanged: bool,
    ) -> Result<ReleaseAuditResult, String> {
        let mut view = self.load(&input.release)?;
        let expected_event_head = view.release.latest_event_hash.clone();
        if matches!(
            view.release.stage,
            ReleaseStage::Released | ReleaseStage::Superseded
        ) {
            return Err(format!(
                "Release {} is {} and cannot be audited again.",
                view.release.id, view.release.stage
            ));
        }
        let platform = select_audit_platform(&view.release, input.platform)?;
        let candidate_id = view
            .release
            .active_candidates
            .get(&platform)
            .ok_or_else(|| {
                format!(
                    "Release {} has no active {platform} candidate.",
                    view.release.id
                )
            })?
            .clone();
        let candidate = view
            .candidates
            .iter()
            .find(|candidate| candidate.id == candidate_id)
            .ok_or_else(|| {
                format!("Active candidate {candidate_id} is missing from the release record.")
            })?;
        let artifact_path = if candidate.artifact_path_relative {
            self.project_root.join(&candidate.artifact_path)
        } else {
            PathBuf::from(&candidate.artifact_path)
        };
        let (report, cache_hit, cache_key, cache_path) = if reuse_unchanged {
            let cached =
                crate::mobile::run_mobile_audit_cached(&self.project_root, &artifact_path)?;
            (
                cached.report,
                cached.cache_hit,
                cached.cache_key,
                cached.cache_path,
            )
        } else {
            (
                crate::mobile::run_mobile_audit(&self.project_root, Some(&artifact_path))?,
                false,
                None,
                None,
            )
        };
        validate_audit_binding(candidate, &report)?;

        if reuse_unchanged
            && let Some(active_audit_id) = view.release.active_audits.get(&platform)
            && let Some(active_audit) = view
                .audits
                .iter()
                .find(|audit| &audit.id == active_audit_id)
            && active_audit.candidate_id == candidate.id
            && active_audit.report_fingerprint == report.report_fingerprint
            && active_audit.policy_digest == report.policy_digest
        {
            let stored_report = read_json(&self.vault_path.join(&active_audit.report_path))?;
            return Ok(ReleaseAuditResult {
                schema_version: RELEASE_SCHEMA_VERSION.to_string(),
                cache_hit,
                cache_key,
                cache_path,
                audit: active_audit.clone(),
                report: stored_report,
            });
        }

        let recorded_at = timestamp();
        let audit_id = generated_id(
            "AUD",
            &[
                &view.release.id,
                &candidate.id,
                &report.report_fingerprint,
                &recorded_at,
            ],
        );
        let report_relative_path = PathBuf::from("releases")
            .join(&view.release.id)
            .join("reports")
            .join(format!("{audit_id}.json"));
        let audit = ReleaseAuditRecord {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            id: audit_id.clone(),
            release_id: view.release.id.clone(),
            candidate_id: candidate.id.clone(),
            platform,
            candidate_sha256: candidate.artifact_sha256.clone(),
            report_fingerprint: report.report_fingerprint.clone(),
            policy_digest: report.policy_digest.clone(),
            decision: release_audit_decision(&report.decision),
            counts: ReleaseAuditCounts {
                passed: report.summary.passed,
                blockers: report.summary.blockers,
                warnings: report.summary.warnings,
                manual: report.summary.manual,
                unavailable: report.summary.unavailable,
            },
            git_commit: report.git_commit.clone(),
            generated_at: report.generated_at.clone(),
            recorded_at: recorded_at.clone(),
            report_path: report_relative_path
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/"),
        };

        let previous_audit_id = view
            .release
            .active_audits
            .insert(platform, audit_id.clone());
        let invalidated_decision_id = view.release.active_decisions.remove(&platform);
        let invalidated_waiver_ids = view
            .waivers
            .iter()
            .filter(|waiver| {
                waiver.platform == platform && view.release.active_waiver_ids.contains(&waiver.id)
            })
            .map(|waiver| waiver.id.clone())
            .collect::<Vec<_>>();
        view.release
            .active_waiver_ids
            .retain(|waiver_id| !invalidated_waiver_ids.contains(waiver_id));
        view.release.audit_ids.push(audit_id.clone());
        view.release.stage =
            stage_after_context_change(&view.release, &view.decisions, ReleaseStage::InReview);
        view.release.updated_at = recorded_at;
        let event = build_event(
            &view.release,
            ReleaseEventKind::CandidateAudited,
            optional_trimmed(input.actor).or_else(current_actor),
            json!({
                "audit_id": audit_id,
                "candidate_id": candidate.id,
                "platform": platform,
                "candidate_sha256": candidate.artifact_sha256,
                "report_fingerprint": report.report_fingerprint,
                "policy_digest": report.policy_digest,
                "decision": audit.decision,
                "previous_audit_id": previous_audit_id,
                "invalidated_decision_id": invalidated_decision_id,
                "invalidated_waiver_ids": invalidated_waiver_ids,
            }),
        )?;
        apply_event_head(&mut view.release, &event);
        view.events.push(event);

        let release_dir = self.release_dir(&view.release.id);
        self.commit_release_transaction(
            &view.release.id,
            expected_event_head.as_deref(),
            None,
            vec![
                (
                    self.vault_path.join(&report_relative_path),
                    json_bytes(&report)?,
                ),
                (
                    release_dir
                        .join("audits")
                        .join(format!("{}.json", audit.id)),
                    json_bytes(&audit)?,
                ),
                (
                    release_dir.join("events.jsonl"),
                    events_bytes(&view.events)?,
                ),
                (release_dir.join("release.json"), json_bytes(&view.release)?),
            ],
        )?;

        Ok(ReleaseAuditResult {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            cache_hit,
            cache_key,
            cache_path,
            audit,
            report,
        })
    }

    pub fn add_evidence(&self, input: NewReleaseEvidence) -> Result<ReleaseEvidenceResult, String> {
        let mut view = self.load(&input.release)?;
        let expected_event_head = view.release.latest_event_hash.clone();
        let (platform, candidate, source_audit, report) =
            self.active_audit_context(&view, input.platform)?;
        let finding_instance_id = required_trimmed("finding instance", &input.finding_instance_id)?;
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.finding_instance_id == finding_instance_id)
            .ok_or_else(|| {
                format!(
                    "Finding '{finding_instance_id}' is not present in active audit {}.",
                    source_audit.id
                )
            })?;
        if finding.status == crate::mobile::MobileFindingStatus::Pass {
            return Err(format!(
                "Finding '{finding_instance_id}' already passes and does not need evidence."
            ));
        }
        let mobile_evidence_kind = normalize_evidence_kind(&input.mobile_evidence_kind)?;
        let expected_rule_id = crate::mobile::mobile_evidence_rule_id(&mobile_evidence_kind);
        if finding.rule_id != expected_rule_id {
            return Err(format!(
                "Evidence kind '{mobile_evidence_kind}' satisfies {expected_rule_id}, not finding {}.",
                finding.rule_id
            ));
        }
        let title = required_trimmed("evidence title", &input.title)?;
        let issuer = required_trimmed("evidence issuer", &input.issuer)?;
        let actor = required_actor(input.actor.or_else(|| Some(issuer.clone())))?;
        let expires_at = validate_optional_rfc3339(input.expires_at)?;
        let created_at = timestamp();
        validate_evidence_times(&created_at, expires_at.as_deref())?;
        let evidence_id = generated_id(
            "EVI",
            &[
                &view.release.id,
                &candidate.id,
                &finding_instance_id,
                &mobile_evidence_kind,
                &created_at,
            ],
        );
        let evidence = crate::EvidenceRecord {
            id: evidence_id.clone(),
            kind: "mobile_attestation".to_string(),
            title,
            source: "release_workflow".to_string(),
            path: optional_trimmed(input.path),
            url: optional_trimmed(input.url),
            artifact_sha: Some(candidate.artifact_sha256.clone()),
            commit: candidate
                .git_commit
                .clone()
                .or_else(|| (report.git_commit != "unknown").then(|| report.git_commit.clone())),
            environment: Some("production".to_string()),
            release: Some(view.release.version.clone()),
            created_at: created_at.clone(),
            expires_at,
            metadata: Some(json!({
                "schema_version": "1.0",
                "mobile_evidence_kind": mobile_evidence_kind,
                "issuer": issuer,
                "release_id": view.release.id,
                "candidate_id": candidate.id,
                "source_audit_id": source_audit.id,
                "finding_instance_id": finding_instance_id,
            })),
            attachments: Some(Vec::new()),
        };
        let link_id = generated_id(
            "LNK",
            &[&view.release.id, &candidate.id, &evidence_id, &created_at],
        );
        let evidence_digest = sha256_serialized(&evidence)?;
        let link = ReleaseEvidenceLink {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            id: link_id.clone(),
            evidence_id: evidence_id.clone(),
            evidence_digest: evidence_digest.clone(),
            release_id: view.release.id.clone(),
            candidate_id: candidate.id.clone(),
            source_audit_id: source_audit.id.clone(),
            platform,
            finding_instance_id: finding_instance_id.clone(),
            mobile_evidence_kind: mobile_evidence_kind.clone(),
            actor: actor.clone(),
            created_at: created_at.clone(),
        };

        view.release.evidence_link_ids.push(link_id.clone());
        view.release.updated_at = created_at;
        let event = build_event(
            &view.release,
            ReleaseEventKind::EvidenceLinked,
            Some(actor),
            json!({
                "link_id": link_id,
                "evidence_id": evidence_id,
                "evidence_digest": evidence_digest,
                "candidate_id": candidate.id,
                "source_audit_id": source_audit.id,
                "platform": platform,
                "finding_instance_id": finding_instance_id,
                "mobile_evidence_kind": mobile_evidence_kind,
            }),
        )?;
        apply_event_head(&mut view.release, &event);
        view.events.push(event);

        let release_dir = self.release_dir(&view.release.id);
        self.commit_release_transaction(
            &view.release.id,
            expected_event_head.as_deref(),
            None,
            vec![
                (
                    self.vault_path
                        .join("evidence")
                        .join(format!("{}.json", evidence.id)),
                    json_bytes(&evidence)?,
                ),
                (
                    release_dir
                        .join("evidence")
                        .join(format!("{}.json", link.id)),
                    json_bytes(&link)?,
                ),
                (
                    release_dir.join("events.jsonl"),
                    events_bytes(&view.events)?,
                ),
                (release_dir.join("release.json"), json_bytes(&view.release)?),
            ],
        )?;

        Ok(ReleaseEvidenceResult {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            link,
            evidence,
        })
    }

    pub fn resolve_finding(
        &self,
        input: NewFindingResolution,
    ) -> Result<FindingResolution, String> {
        let mut view = self.load(&input.release)?;
        let expected_event_head = view.release.latest_event_hash.clone();
        let (platform, candidate, source_audit, report) =
            self.active_audit_context(&view, input.platform)?;
        let finding_instance_id = required_trimmed("finding instance", &input.finding_instance_id)?;
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.finding_instance_id == finding_instance_id)
            .ok_or_else(|| {
                format!(
                    "Finding '{finding_instance_id}' is not present in active audit {}.",
                    source_audit.id
                )
            })?;
        if finding.status == crate::mobile::MobileFindingStatus::Pass {
            return Err(format!(
                "Finding '{finding_instance_id}' already passes and cannot be resolved as open work."
            ));
        }
        if view.resolutions.iter().any(|resolution| {
            resolution.candidate_id == candidate.id
                && resolution.finding_instance_id == finding_instance_id
        }) {
            return Err(format!(
                "Finding '{finding_instance_id}' already has a resolution for candidate {}.",
                candidate.id
            ));
        }
        let reason = required_trimmed("resolution reason", &input.reason)?;
        let actor = required_actor(input.actor)?;
        let mut evidence_ids = input
            .evidence_ids
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        evidence_ids.sort();
        evidence_ids.dedup();
        for evidence_id in &evidence_ids {
            if !view
                .evidence_links
                .iter()
                .any(|link| link.evidence_id == *evidence_id && link.candidate_id == candidate.id)
            {
                return Err(format!(
                    "Evidence '{evidence_id}' is not linked to candidate {}.",
                    candidate.id
                ));
            }
        }
        let created_at = timestamp();
        let resolution_id = generated_id(
            "RES",
            &[
                &view.release.id,
                &candidate.id,
                &finding_instance_id,
                &created_at,
            ],
        );
        let resolution = FindingResolution {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            id: resolution_id.clone(),
            release_id: view.release.id.clone(),
            candidate_id: candidate.id.clone(),
            source_audit_id: source_audit.id.clone(),
            platform,
            finding_instance_id: finding_instance_id.clone(),
            status: FindingResolutionStatus::Resolved,
            reason: reason.clone(),
            evidence_ids: evidence_ids.clone(),
            actor: actor.clone(),
            created_at: created_at.clone(),
        };

        view.release.resolution_ids.push(resolution_id.clone());
        view.release.updated_at = created_at;
        let event = build_event(
            &view.release,
            ReleaseEventKind::FindingResolved,
            Some(actor),
            json!({
                "resolution_id": resolution_id,
                "candidate_id": candidate.id,
                "source_audit_id": source_audit.id,
                "platform": platform,
                "finding_instance_id": finding_instance_id,
                "reason": reason,
                "evidence_ids": evidence_ids,
                "decision_unchanged": true,
            }),
        )?;
        apply_event_head(&mut view.release, &event);
        view.events.push(event);

        let release_dir = self.release_dir(&view.release.id);
        self.commit_release_transaction(
            &view.release.id,
            expected_event_head.as_deref(),
            None,
            vec![
                (
                    release_dir
                        .join("resolutions")
                        .join(format!("{}.json", resolution.id)),
                    json_bytes(&resolution)?,
                ),
                (
                    release_dir.join("events.jsonl"),
                    events_bytes(&view.events)?,
                ),
                (release_dir.join("release.json"), json_bytes(&view.release)?),
            ],
        )?;
        Ok(resolution)
    }

    pub fn decide(&self, input: NewHumanDecision) -> Result<HumanReleaseDecision, String> {
        let mut view = self.load(&input.release)?;
        let expected_event_head = view.release.latest_event_hash.clone();
        let (platform, candidate, audit, _report) =
            self.active_audit_context(&view, input.platform)?;
        if input.decision == HumanDecisionKind::Approved
            && audit.decision == ReleaseAuditDecision::NoGo
        {
            return Err(format!(
                "Audit {} is No-Go and cannot be approved. Resolve or explicitly change the deterministic gate inputs, then audit again.",
                audit.id
            ));
        }
        let decided_by = required_trimmed("decision identity", &input.decided_by)?;
        let role = required_trimmed("decision role", &input.role)?;
        let rationale = required_trimmed("decision rationale", &input.rationale)?;
        let created_at = timestamp();
        let id = generated_id(
            "DEC",
            &[
                &view.release.id,
                &candidate.id,
                &audit.id,
                match input.decision {
                    HumanDecisionKind::Approved => "approved",
                    HumanDecisionKind::Rejected => "rejected",
                },
                &created_at,
            ],
        );
        let decision = HumanReleaseDecision {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            id: id.clone(),
            release_id: view.release.id.clone(),
            candidate_id: candidate.id.clone(),
            candidate_sha256: candidate.artifact_sha256.clone(),
            audit_id: audit.id.clone(),
            platform,
            report_fingerprint: audit.report_fingerprint.clone(),
            policy_digest: audit.policy_digest.clone(),
            decision: input.decision,
            decided_by: decided_by.clone(),
            role: role.clone(),
            rationale: rationale.clone(),
            created_at: created_at.clone(),
        };
        let previous_decision_id = view.release.active_decisions.insert(platform, id.clone());
        view.release.decision_ids.push(id.clone());
        view.decisions.push(decision.clone());
        view.release.stage = effective_decision_stage(&view.release, &view.decisions);
        view.release.updated_at = created_at;
        let event_kind = match input.decision {
            HumanDecisionKind::Approved => ReleaseEventKind::ReleaseApproved,
            HumanDecisionKind::Rejected => ReleaseEventKind::ReleaseRejected,
        };
        let event = build_event(
            &view.release,
            event_kind,
            Some(decided_by),
            json!({
                "decision_id": id,
                "candidate_id": candidate.id,
                "candidate_sha256": candidate.artifact_sha256,
                "audit_id": audit.id,
                "platform": platform,
                "report_fingerprint": audit.report_fingerprint,
                "policy_digest": audit.policy_digest,
                "decision": input.decision,
                "role": role,
                "rationale": rationale,
                "previous_decision_id": previous_decision_id,
            }),
        )?;
        apply_event_head(&mut view.release, &event);
        view.events.push(event);

        let release_dir = self.release_dir(&view.release.id);
        self.commit_release_transaction(
            &view.release.id,
            expected_event_head.as_deref(),
            None,
            vec![
                (
                    release_dir
                        .join("decisions")
                        .join(format!("{}.json", decision.id)),
                    json_bytes(&decision)?,
                ),
                (
                    release_dir.join("events.jsonl"),
                    events_bytes(&view.events)?,
                ),
                (release_dir.join("release.json"), json_bytes(&view.release)?),
            ],
        )?;
        Ok(decision)
    }

    pub fn waive_finding(&self, input: NewReleaseWaiver) -> Result<ReleaseFindingWaiver, String> {
        let mut view = self.load(&input.release)?;
        let expected_event_head = view.release.latest_event_hash.clone();
        let (platform, candidate, audit, report) =
            self.active_audit_context(&view, input.platform)?;
        let finding_instance_id = required_trimmed("finding instance", &input.finding_instance_id)?;
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.finding_instance_id == finding_instance_id)
            .ok_or_else(|| {
                format!(
                    "Finding '{finding_instance_id}' is not present in active audit {}.",
                    audit.id
                )
            })?;
        if finding.status == crate::mobile::MobileFindingStatus::Pass {
            return Err(format!(
                "Finding '{finding_instance_id}' already passes and cannot be waived."
            ));
        }
        if view.waivers.iter().any(|waiver| {
            waiver.audit_id == audit.id
                && waiver.finding_instance_id == finding_instance_id
                && view.release.active_waiver_ids.contains(&waiver.id)
                && !waiver_has_expired(&waiver.expires_at)
        }) {
            return Err(format!(
                "Finding '{finding_instance_id}' already has an active waiver for audit {}.",
                audit.id
            ));
        }
        let reason = required_trimmed("waiver reason", &input.reason)?;
        let approved_by = required_trimmed("waiver approver", &input.approved_by)?;
        let created_at = timestamp();
        validate_evidence_times(&created_at, Some(&input.expires_at))?;
        let id = generated_id(
            "WAV",
            &[
                &view.release.id,
                &candidate.id,
                &audit.id,
                &finding_instance_id,
                &created_at,
            ],
        );
        let waiver = ReleaseFindingWaiver {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            id: id.clone(),
            release_id: view.release.id.clone(),
            candidate_id: candidate.id.clone(),
            candidate_sha256: candidate.artifact_sha256.clone(),
            audit_id: audit.id.clone(),
            platform,
            report_fingerprint: audit.report_fingerprint.clone(),
            policy_digest: audit.policy_digest.clone(),
            finding_instance_id: finding_instance_id.clone(),
            reason: reason.clone(),
            approved_by: approved_by.clone(),
            created_at: created_at.clone(),
            expires_at: input.expires_at.clone(),
            status: ReleaseWaiverStatus::Active,
        };
        view.release.waiver_ids.push(id.clone());
        view.release.active_waiver_ids.push(id.clone());
        view.release.updated_at = created_at;
        let event = build_event(
            &view.release,
            ReleaseEventKind::FindingWaived,
            Some(approved_by),
            json!({
                "waiver_id": id,
                "candidate_id": candidate.id,
                "candidate_sha256": candidate.artifact_sha256,
                "audit_id": audit.id,
                "platform": platform,
                "report_fingerprint": audit.report_fingerprint,
                "policy_digest": audit.policy_digest,
                "finding_instance_id": finding_instance_id,
                "reason": reason,
                "expires_at": input.expires_at,
                "decision_unchanged": true,
            }),
        )?;
        apply_event_head(&mut view.release, &event);
        view.events.push(event);

        let release_dir = self.release_dir(&view.release.id);
        self.commit_release_transaction(
            &view.release.id,
            expected_event_head.as_deref(),
            None,
            vec![
                (
                    release_dir
                        .join("waivers")
                        .join(format!("{}.json", waiver.id)),
                    json_bytes(&waiver)?,
                ),
                (
                    release_dir.join("events.jsonl"),
                    events_bytes(&view.events)?,
                ),
                (release_dir.join("release.json"), json_bytes(&view.release)?),
            ],
        )?;
        Ok(waiver)
    }

    pub fn build_packet(&self, release_reference: &str) -> Result<ReleasePacket, String> {
        let view = self.load(release_reference)?;
        let mut reports: Vec<crate::mobile::MobileAuditReport> =
            Vec::with_capacity(view.audits.len());
        for audit in &view.audits {
            reports.push(read_json(&self.vault_path.join(&audit.report_path))?);
        }

        let mut active_platforms = Vec::with_capacity(view.release.active_candidates.len());
        for (platform, candidate_id) in &view.release.active_candidates {
            let candidate = view
                .candidates
                .iter()
                .find(|candidate| &candidate.id == candidate_id)
                .ok_or_else(|| format!("Active candidate {candidate_id} is missing."))?;
            let audit = view
                .release
                .active_audits
                .get(platform)
                .and_then(|audit_id| view.audits.iter().find(|audit| &audit.id == audit_id));
            let report = audit.and_then(|audit| {
                view.audits
                    .iter()
                    .position(|candidate_audit| candidate_audit.id == audit.id)
                    .and_then(|index| reports.get(index))
            });
            let decision = view
                .release
                .active_decisions
                .get(platform)
                .and_then(|decision_id| {
                    view.decisions
                        .iter()
                        .find(|decision| &decision.id == decision_id)
                });
            let context_waivers = view
                .waivers
                .iter()
                .filter(|waiver| {
                    waiver.platform == *platform
                        && view.release.active_waiver_ids.contains(&waiver.id)
                })
                .collect::<Vec<_>>();
            let effective_waiver_ids = context_waivers
                .iter()
                .filter(|waiver| {
                    !time_is_at_or_before(&waiver.expires_at, &view.release.updated_at)
                })
                .map(|waiver| waiver.id.clone())
                .collect();
            let expired_waiver_ids = context_waivers
                .iter()
                .filter(|waiver| time_is_at_or_before(&waiver.expires_at, &view.release.updated_at))
                .map(|waiver| waiver.id.clone())
                .collect();
            active_platforms.push(ReleasePacketPlatform {
                platform: *platform,
                candidate_id: candidate.id.clone(),
                candidate_sha256: candidate.artifact_sha256.clone(),
                audit_id: audit.map(|audit| audit.id.clone()),
                automated_decision: audit.map(|audit| audit.decision),
                report_fingerprint: audit.map(|audit| audit.report_fingerprint.clone()),
                policy_digest: audit.map(|audit| audit.policy_digest.clone()),
                human_decision_id: decision.map(|decision| decision.id.clone()),
                human_decision: decision.map(|decision| decision.decision),
                context_waiver_ids: context_waivers
                    .iter()
                    .map(|waiver| waiver.id.clone())
                    .collect(),
                effective_waiver_ids,
                expired_waiver_ids,
                unresolved_finding_ids: report
                    .map(|report| {
                        report
                            .findings
                            .iter()
                            .filter(|finding| {
                                finding.status != crate::mobile::MobileFindingStatus::Pass
                            })
                            .map(|finding| finding.finding_instance_id.clone())
                            .collect()
                    })
                    .unwrap_or_default(),
            });
        }

        let mut packet = ReleasePacket {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            packet_id: String::new(),
            packet_digest: String::new(),
            assembled_at: view.release.updated_at.clone(),
            release: view.release,
            active_platforms,
            candidates: view.candidates,
            audits: view.audits,
            reports,
            evidence_links: view.evidence_links,
            resolutions: view.resolutions,
            decisions: view.decisions,
            waivers: view.waivers,
            events: view.events,
        };
        let digest = calculate_release_packet_digest(&packet)?;
        packet.packet_id = format!("PKT-{}", &digest[..10].to_ascii_uppercase());
        packet.packet_digest = digest;
        verify_release_packet(&packet)?;
        Ok(packet)
    }

    pub fn write_packet(&self, release_reference: &str) -> Result<ReleasePacketArtifacts, String> {
        let packet = self.build_packet(release_reference)?;
        let base = PathBuf::from("releases")
            .join(&packet.release.id)
            .join("packets")
            .join(&packet.packet_id);
        let json_path = base.with_extension("json");
        let markdown_path = base.with_extension("md");
        let html_path = base.with_extension("html");
        let handoff_summary = render_release_handoff_summary(&packet);
        self.commit_release_transaction(
            &packet.release.id,
            packet.release.latest_event_hash.as_deref(),
            None,
            vec![
                (self.vault_path.join(&json_path), json_bytes(&packet)?),
                (
                    self.vault_path.join(&markdown_path),
                    render_release_packet_markdown(&packet).into_bytes(),
                ),
                (
                    self.vault_path.join(&html_path),
                    render_release_packet_html(&packet).into_bytes(),
                ),
            ],
        )?;
        Ok(ReleasePacketArtifacts {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            packet,
            json_path: portable_path(&json_path),
            markdown_path: portable_path(&markdown_path),
            html_path: portable_path(&html_path),
            handoff_summary,
        })
    }

    pub fn diff_candidates(&self, input: NewReleaseDiff) -> Result<ReleaseCandidateDiff, String> {
        let view = self.load(&input.release)?;
        let platform = select_audit_platform(&view.release, input.platform)?;
        let to_candidate_id = input
            .to_candidate_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .or_else(|| view.release.active_candidates.get(&platform).cloned())
            .ok_or_else(|| {
                format!(
                    "Release {} has no active {platform} candidate.",
                    view.release.id
                )
            })?;
        let to_position = view
            .candidates
            .iter()
            .position(|candidate| candidate.id == to_candidate_id)
            .ok_or_else(|| {
                format!(
                    "Candidate '{to_candidate_id}' was not found in release {}.",
                    view.release.id
                )
            })?;
        let to_candidate = view.candidates[to_position].clone();
        if to_candidate.platform != platform {
            return Err(format!(
                "Candidate {} is not a {platform} candidate.",
                to_candidate.id
            ));
        }
        let from_candidate = if let Some(from_candidate_id) = input
            .from_candidate_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            view.candidates
                .iter()
                .find(|candidate| candidate.id == from_candidate_id)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Candidate '{from_candidate_id}' was not found in release {}.",
                        view.release.id
                    )
                })?
        } else {
            view.candidates[..to_position]
                .iter()
                .rev()
                .find(|candidate| candidate.platform == platform)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Release {} has no earlier {platform} candidate to compare.",
                        view.release.id
                    )
                })?
        };
        if from_candidate.platform != platform || from_candidate.id == to_candidate.id {
            return Err(
                "Release diff requires two distinct candidates on the same platform.".to_string(),
            );
        }

        let from_audit = view
            .audits
            .iter()
            .rev()
            .find(|audit| audit.candidate_id == from_candidate.id)
            .cloned();
        let to_audit = view
            .audits
            .iter()
            .rev()
            .find(|audit| audit.candidate_id == to_candidate.id)
            .cloned();
        let from_report: Option<crate::mobile::MobileAuditReport> = from_audit
            .as_ref()
            .map(|audit| read_json(&self.vault_path.join(&audit.report_path)))
            .transpose()?;
        let to_report: Option<crate::mobile::MobileAuditReport> = to_audit
            .as_ref()
            .map(|audit| read_json(&self.vault_path.join(&audit.report_path)))
            .transpose()?;
        let from_findings = finding_map(from_report.as_ref());
        let to_findings = finding_map(to_report.as_ref());
        let finding_ids = if from_report.is_some() && to_report.is_some() {
            from_findings
                .keys()
                .chain(to_findings.keys())
                .copied()
                .collect::<BTreeSet<_>>()
        } else {
            BTreeSet::new()
        };
        let mut introduced_findings = Vec::new();
        let mut resolved_findings = Vec::new();
        let mut changed_findings = Vec::new();
        for finding_id in finding_ids {
            let from = from_findings.get(finding_id).copied();
            let to = to_findings.get(finding_id).copied();
            let was_open = from.is_some_and(|finding| finding_is_open(&finding.status));
            let is_open = to.is_some_and(|finding| finding_is_open(&finding.status));
            let delta = finding_delta(finding_id, from, to);
            if !was_open && is_open {
                introduced_findings.push(delta);
            } else if was_open && !is_open {
                resolved_findings.push(delta);
            } else if was_open
                && is_open
                && from.zip(to).is_some_and(|(from, to)| {
                    from.status != to.status
                        || from.severity != to.severity
                        || from.title != to.title
                        || from.observed != to.observed
                        || from.expected != to.expected
                })
            {
                changed_findings.push(delta);
            }
        }
        let related_events = view
            .events
            .iter()
            .filter(|event| {
                event.details.get("candidate_id").and_then(Value::as_str)
                    == Some(from_candidate.id.as_str())
                    || event.details.get("candidate_id").and_then(Value::as_str)
                        == Some(to_candidate.id.as_str())
            })
            .cloned()
            .collect();
        Ok(ReleaseCandidateDiff {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            release_id: view.release.id,
            platform,
            artifact_size_delta_bytes: i128::from(to_candidate.artifact_size_bytes)
                - i128::from(from_candidate.artifact_size_bytes),
            git_commit_changed: from_candidate.git_commit != to_candidate.git_commit,
            automated_decision_changed: from_audit.as_ref().map(|audit| audit.decision)
                != to_audit.as_ref().map(|audit| audit.decision),
            policy_changed: from_audit.as_ref().map(|audit| &audit.policy_digest)
                != to_audit.as_ref().map(|audit| &audit.policy_digest),
            from_candidate,
            to_candidate,
            from_audit,
            to_audit,
            introduced_findings,
            resolved_findings,
            changed_findings,
            related_events,
        })
    }

    fn active_audit_context(
        &self,
        view: &ReleaseView,
        requested: Option<MobilePlatform>,
    ) -> Result<
        (
            MobilePlatform,
            ReleaseCandidate,
            ReleaseAuditRecord,
            crate::mobile::MobileAuditReport,
        ),
        String,
    > {
        let platform = select_audit_platform(&view.release, requested)?;
        let candidate_id = view
            .release
            .active_candidates
            .get(&platform)
            .ok_or_else(|| {
                format!(
                    "Release {} has no active {platform} candidate.",
                    view.release.id
                )
            })?;
        let candidate = view
            .candidates
            .iter()
            .find(|candidate| &candidate.id == candidate_id)
            .cloned()
            .ok_or_else(|| format!("Active candidate {candidate_id} is missing."))?;
        let audit_id = view.release.active_audits.get(&platform).ok_or_else(|| {
            format!(
                "Release {} has no active {platform} audit. Run `gantry release audit` first.",
                view.release.id
            )
        })?;
        let audit = view
            .audits
            .iter()
            .find(|audit| &audit.id == audit_id)
            .cloned()
            .ok_or_else(|| format!("Active audit {audit_id} is missing."))?;
        let report = read_json(&self.vault_path.join(&audit.report_path))?;
        Ok((platform, candidate, audit, report))
    }

    fn require_vault(&self) -> Result<(), String> {
        if self.vault_path.is_dir() {
            Ok(())
        } else {
            Err(format!(
                "Gantry vault not found at {}",
                self.vault_path.display()
            ))
        }
    }

    fn resolve_release(&self, release_reference: &str) -> Result<MobileRelease, String> {
        let release_reference = required_trimmed("release", release_reference)?;
        let matches: Vec<_> = self
            .list()?
            .into_iter()
            .filter(|release| {
                release.id == release_reference || release.version == release_reference
            })
            .collect();
        match matches.as_slice() {
            [] => Err(format!("Release '{release_reference}' was not found.")),
            [release] => Ok(release.clone()),
            _ => Err(format!(
                "Release reference '{release_reference}' is ambiguous; use the REL- identifier."
            )),
        }
    }

    fn transaction_root(&self) -> PathBuf {
        self.vault_path.join("transactions")
    }

    fn commit_lock_path(&self) -> PathBuf {
        self.vault_path.join(".release-write.lock")
    }

    fn acquire_commit_lock(&self) -> Result<ReleaseCommitLock, String> {
        self.require_vault()?;
        let path = self.commit_lock_path();
        for attempt in 0..200 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    writeln!(
                        file,
                        "pid={} acquired_at={}",
                        std::process::id(),
                        timestamp()
                    )
                    .map_err(|error| format!("Failed to initialize {}: {error}", path.display()))?;
                    file.sync_all()
                        .map_err(|error| format!("Failed to sync {}: {error}", path.display()))?;
                    sync_directory(&self.vault_path)?;
                    return Ok(ReleaseCommitLock { path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = fs::metadata(&path)
                        .and_then(|metadata| metadata.modified())
                        .ok()
                        .and_then(|modified| modified.elapsed().ok())
                        .is_some_and(|age| age > Duration::from_secs(30 * 60));
                    if stale {
                        fs::remove_file(&path).map_err(|remove_error| {
                            format!(
                                "Release commit lock {} is stale but could not be removed: {remove_error}",
                                path.display()
                            )
                        })?;
                        continue;
                    }
                    if attempt == 199 {
                        return Err(format!(
                            "Another Gantry process is committing release state (lock: {}). Retry after it finishes.",
                            path.display()
                        ));
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(error) => {
                    return Err(format!(
                        "Failed to acquire release commit lock {}: {error}",
                        path.display()
                    ));
                }
            }
        }
        Err("Failed to acquire release commit lock.".to_string())
    }

    fn pending_transaction_count(&self) -> Result<usize, String> {
        let root = self.transaction_root();
        if !root.exists() {
            return Ok(0);
        }
        let mut count = 0;
        for entry in fs::read_dir(&root)
            .map_err(|error| format!("Failed to read {}: {error}", root.display()))?
        {
            let entry =
                entry.map_err(|error| format!("Failed to read transaction entry: {error}"))?;
            if entry
                .file_type()
                .map_err(|error| format!("Failed to inspect transaction entry: {error}"))?
                .is_dir()
            {
                count += 1;
            }
        }
        Ok(count)
    }

    fn recover_if_needed(&self) -> Result<(), String> {
        if self.pending_transaction_count()? == 0 {
            return Ok(());
        }
        let _lock = self.acquire_commit_lock()?;
        self.recover_pending_transactions_locked()?;
        Ok(())
    }

    fn recover_pending_transactions_locked(&self) -> Result<usize, String> {
        let root = self.transaction_root();
        if !root.exists() {
            return Ok(0);
        }
        let mut directories = fs::read_dir(&root)
            .map_err(|error| format!("Failed to read {}: {error}", root.display()))?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry
                    .file_type()
                    .ok()
                    .filter(|kind| kind.is_dir())
                    .map(|_| entry.path())
            })
            .collect::<Vec<_>>();
        directories.sort();
        let mut recovered = 0;
        for directory in directories {
            let manifest_path = directory.join("manifest.json");
            if !manifest_path.is_file() {
                fs::remove_dir_all(&directory).map_err(|error| {
                    format!(
                        "Failed to remove unpublished transaction {}: {error}",
                        directory.display()
                    )
                })?;
                continue;
            }
            self.apply_transaction_directory(&directory)?;
            recovered += 1;
        }
        sync_directory(&root)?;
        Ok(recovered)
    }

    fn apply_transaction_directory(&self, directory: &Path) -> Result<(), String> {
        let manifest: ReleaseTransactionManifest = read_json(&directory.join("manifest.json"))?;
        if manifest.schema_version != RELEASE_SCHEMA_VERSION {
            return Err(format!(
                "Transaction {} uses unsupported schema {}.",
                manifest.transaction_id, manifest.schema_version
            ));
        }
        for write in &manifest.writes {
            let target = safe_relative_transaction_path(&write.target)?;
            let staged = safe_relative_transaction_path(&write.staged_file)?;
            let bytes = fs::read(directory.join(staged)).map_err(|error| {
                format!(
                    "Failed to read staged data for transaction {}: {error}",
                    manifest.transaction_id
                )
            })?;
            let actual = hex::encode(Sha256::digest(&bytes));
            if actual != write.sha256 {
                return Err(format!(
                    "Transaction {} staged digest mismatch for {}.",
                    manifest.transaction_id, write.target
                ));
            }
            atomic_write(&self.vault_path.join(target), &bytes)?;
        }
        atomic_write(&directory.join("committed"), b"committed\n")?;
        fs::remove_dir_all(directory).map_err(|error| {
            format!(
                "Transaction committed but cleanup failed for {}: {error}",
                directory.display()
            )
        })?;
        Ok(())
    }

    fn commit_release_transaction(
        &self,
        release_id: &str,
        expected_event_head: Option<&str>,
        unique_version: Option<&str>,
        writes: Vec<(PathBuf, Vec<u8>)>,
    ) -> Result<(), String> {
        let _lock = self.acquire_commit_lock()?;
        self.recover_pending_transactions_locked()?;

        let release_path = self.release_dir(release_id).join("release.json");
        if let Some(version) = unique_version {
            if release_path.exists() {
                return Err(format!("Release {release_id} already exists."));
            }
            if self
                .list_without_recovery()?
                .iter()
                .any(|release| release.version == version)
            {
                return Err(format!(
                    "A release already exists for version '{version}'. Use `gantry release show {version}`."
                ));
            }
        } else {
            let current: MobileRelease = read_json(&release_path)?;
            if current.latest_event_hash.as_deref() != expected_event_head {
                return Err(format!(
                    "Release {} changed while this operation was running. Reload it and retry; no files were written.",
                    current.id
                ));
            }
        }

        let transaction_id = generated_id("TXN", &[release_id, &timestamp()]);
        let directory = self.transaction_root().join(&transaction_id);
        fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "Failed to create transaction directory {}: {error}",
                directory.display()
            )
        })?;
        let mut manifest_writes = Vec::with_capacity(writes.len());
        for (index, (target, bytes)) in writes.into_iter().enumerate() {
            let relative = target.strip_prefix(&self.vault_path).map_err(|_| {
                format!(
                    "Transaction target {} is outside the Gantry vault.",
                    target.display()
                )
            })?;
            let relative = safe_relative_transaction_path(&portable_path(relative))?;
            let staged_file = format!("{index:03}.data");
            atomic_write(&directory.join(&staged_file), &bytes)?;
            manifest_writes.push(ReleaseTransactionWrite {
                target: portable_path(&relative),
                staged_file,
                sha256: hex::encode(Sha256::digest(&bytes)),
            });
        }
        let manifest = ReleaseTransactionManifest {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            transaction_id,
            release_id: release_id.to_string(),
            created_at: timestamp(),
            writes: manifest_writes,
        };
        atomic_write_json(&directory.join("manifest.json"), &manifest)?;
        sync_directory(&directory)?;
        self.apply_transaction_directory(&directory)
    }

    fn releases_root(&self) -> PathBuf {
        self.vault_path.join("releases")
    }

    fn release_dir(&self, id: &str) -> PathBuf {
        self.releases_root().join(id)
    }
}

fn select_audit_platform(
    release: &MobileRelease,
    requested: Option<MobilePlatform>,
) -> Result<MobilePlatform, String> {
    if let Some(platform) = requested {
        return Ok(platform);
    }
    match release
        .active_candidates
        .keys()
        .copied()
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => Err(format!(
            "Release {} has no active candidate. Bind one with `gantry release candidate add`.",
            release.id
        )),
        [platform] => Ok(*platform),
        _ => Err(format!(
            "Release {} has both iOS and Android candidates. Select one with --platform.",
            release.id
        )),
    }
}

fn release_audit_decision(decision: &crate::mobile::MobileDecision) -> ReleaseAuditDecision {
    match decision {
        crate::mobile::MobileDecision::Go => ReleaseAuditDecision::Go,
        crate::mobile::MobileDecision::AtRisk => ReleaseAuditDecision::AtRisk,
        crate::mobile::MobileDecision::NoGo => ReleaseAuditDecision::NoGo,
    }
}

fn validate_audit_binding(
    candidate: &ReleaseCandidate,
    report: &crate::mobile::MobileAuditReport,
) -> Result<(), String> {
    let artifact = report
        .artifact
        .as_ref()
        .ok_or_else(|| "Persistent release audits require an inspected artifact.".to_string())?;
    if artifact.sha256 != candidate.artifact_sha256 {
        return Err(format!(
            "Candidate {} changed after it was bound: expected SHA-256 {}, audited {}. Bind the rebuilt candidate before auditing.",
            candidate.id, candidate.artifact_sha256, artifact.sha256
        ));
    }
    if artifact.size_bytes != candidate.artifact_size_bytes {
        return Err(format!(
            "Candidate {} size changed after it was bound.",
            candidate.id
        ));
    }
    let report_platform = match artifact.platform {
        crate::mobile::MobilePlatform::Ios => MobilePlatform::Ios,
        crate::mobile::MobilePlatform::Android => MobilePlatform::Android,
    };
    if report_platform != candidate.platform {
        return Err(format!(
            "Candidate {} platform does not match the audited artifact.",
            candidate.id
        ));
    }
    if let Some(candidate_commit) = &candidate.git_commit {
        if &report.git_commit != candidate_commit {
            return Err(format!(
                "Current source commit {} does not match candidate {} binding commit {}. Rebuild and bind a candidate from the current commit.",
                report.git_commit, candidate.id, candidate_commit
            ));
        }
    }
    Ok(())
}

fn verify_release_schedule(release: &MobileRelease, events: &[ReleaseEvent]) -> Result<(), String> {
    let latest = events
        .iter()
        .rev()
        .find(|event| event.kind == ReleaseEventKind::ReleaseScheduled);
    if latest.is_none()
        && (release.target_date.is_some()
            || release.submission_date.is_some()
            || release.rollout_date.is_some())
    {
        return Err(format!(
            "Release {} schedule has no matching release event.",
            release.id
        ));
    }
    if let Some(event) = latest {
        let matches = event.details.get("target_date").and_then(Value::as_str)
            == release.target_date.as_deref()
            && event.details.get("submission_date").and_then(Value::as_str)
                == release.submission_date.as_deref()
            && event.details.get("rollout_date").and_then(Value::as_str)
                == release.rollout_date.as_deref();
        if !matches {
            return Err(format!(
                "Release {} schedule does not match its latest release event.",
                release.id
            ));
        }
    }
    Ok(())
}

fn verify_active_audits(
    release: &MobileRelease,
    audits: &[ReleaseAuditRecord],
) -> Result<(), String> {
    for (platform, audit_id) in &release.active_audits {
        let audit = audits
            .iter()
            .find(|audit| &audit.id == audit_id)
            .ok_or_else(|| {
                format!(
                    "Active audit {audit_id} is missing from release {}.",
                    release.id
                )
            })?;
        if &audit.platform != platform {
            return Err(format!("Active audit {audit_id} has the wrong platform."));
        }
        if release.active_candidates.get(platform) != Some(&audit.candidate_id) {
            return Err(format!(
                "Active audit {audit_id} is not bound to the active {platform} candidate."
            ));
        }
    }
    Ok(())
}

fn verify_stored_audit(
    vault_path: &Path,
    release: &MobileRelease,
    candidates: &[ReleaseCandidate],
    audit: &ReleaseAuditRecord,
) -> Result<(), String> {
    if audit.release_id != release.id {
        return Err(format!(
            "Audit {} belongs to a different release.",
            audit.id
        ));
    }
    let expected_path = PathBuf::from("releases")
        .join(&release.id)
        .join("reports")
        .join(format!("{}.json", audit.id));
    if Path::new(&audit.report_path) != expected_path {
        return Err(format!("Audit {} has an invalid report path.", audit.id));
    }
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.id == audit.candidate_id)
        .ok_or_else(|| format!("Audit {} references a missing candidate.", audit.id))?;
    if candidate.platform != audit.platform || candidate.artifact_sha256 != audit.candidate_sha256 {
        return Err(format!(
            "Audit {} candidate binding is inconsistent.",
            audit.id
        ));
    }
    let report: crate::mobile::MobileAuditReport = read_json(&vault_path.join(&expected_path))?;
    validate_audit_binding(candidate, &report)?;
    let calculated = crate::mobile::calculate_mobile_report_fingerprint(&report)?;
    if calculated != report.report_fingerprint || calculated != audit.report_fingerprint {
        return Err(format!(
            "Audit {} report fingerprint does not verify; the report may have been modified.",
            audit.id
        ));
    }
    if report.policy_digest != audit.policy_digest
        || release_audit_decision(&report.decision) != audit.decision
        || audit.counts
            != (ReleaseAuditCounts {
                passed: report.summary.passed,
                blockers: report.summary.blockers,
                warnings: report.summary.warnings,
                manual: report.summary.manual,
                unavailable: report.summary.unavailable,
            })
        || audit.git_commit != report.git_commit
        || audit.generated_at != report.generated_at
    {
        return Err(format!(
            "Audit {} summary does not match its report.",
            audit.id
        ));
    }
    Ok(())
}

fn verify_evidence_link(
    vault_path: &Path,
    release: &MobileRelease,
    candidates: &[ReleaseCandidate],
    audits: &[ReleaseAuditRecord],
    events: &[ReleaseEvent],
    link: &ReleaseEvidenceLink,
) -> Result<(), String> {
    if link.release_id != release.id {
        return Err(format!(
            "Evidence link {} belongs to another release.",
            link.id
        ));
    }
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.id == link.candidate_id)
        .ok_or_else(|| format!("Evidence link {} references a missing candidate.", link.id))?;
    let audit = audits
        .iter()
        .find(|audit| audit.id == link.source_audit_id)
        .ok_or_else(|| format!("Evidence link {} references a missing audit.", link.id))?;
    if candidate.platform != link.platform
        || audit.platform != link.platform
        || audit.candidate_id != candidate.id
    {
        return Err(format!(
            "Evidence link {} binding is inconsistent.",
            link.id
        ));
    }
    let report: crate::mobile::MobileAuditReport = read_json(&vault_path.join(&audit.report_path))?;
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.finding_instance_id == link.finding_instance_id)
        .ok_or_else(|| format!("Evidence link {} references a missing finding.", link.id))?;
    if finding.rule_id != crate::mobile::mobile_evidence_rule_id(&link.mobile_evidence_kind) {
        return Err(format!(
            "Evidence link {} kind does not match its finding.",
            link.id
        ));
    }
    let evidence: crate::EvidenceRecord = read_json(
        &vault_path
            .join("evidence")
            .join(format!("{}.json", link.evidence_id)),
    )?;
    let metadata = evidence
        .metadata
        .as_ref()
        .ok_or_else(|| format!("Evidence {} has no workflow metadata.", evidence.id))?;
    let metadata_matches = metadata.get("schema_version").and_then(Value::as_str) == Some("1.0")
        && metadata.get("mobile_evidence_kind").and_then(Value::as_str)
            == Some(link.mobile_evidence_kind.as_str())
        && metadata.get("issuer").and_then(Value::as_str).is_some()
        && metadata.get("release_id").and_then(Value::as_str) == Some(release.id.as_str())
        && metadata.get("candidate_id").and_then(Value::as_str) == Some(candidate.id.as_str())
        && metadata.get("source_audit_id").and_then(Value::as_str) == Some(audit.id.as_str())
        && metadata.get("finding_instance_id").and_then(Value::as_str)
            == Some(link.finding_instance_id.as_str());
    if evidence.id != link.evidence_id
        || sha256_serialized(&evidence)? != link.evidence_digest
        || evidence.artifact_sha.as_deref() != Some(candidate.artifact_sha256.as_str())
        || evidence.release.as_deref() != Some(release.version.as_str())
        || evidence.source != "release_workflow"
        || !metadata_matches
    {
        return Err(format!("Evidence link {} record is inconsistent.", link.id));
    }
    validate_evidence_times(&evidence.created_at, evidence.expires_at.as_deref())?;
    let event_matches = events.iter().any(|event| {
        event.kind == ReleaseEventKind::EvidenceLinked
            && event.details.get("link_id").and_then(Value::as_str) == Some(link.id.as_str())
            && event.details.get("evidence_id").and_then(Value::as_str)
                == Some(link.evidence_id.as_str())
            && event.details.get("evidence_digest").and_then(Value::as_str)
                == Some(link.evidence_digest.as_str())
            && event.details.get("candidate_id").and_then(Value::as_str)
                == Some(link.candidate_id.as_str())
            && event.details.get("source_audit_id").and_then(Value::as_str)
                == Some(link.source_audit_id.as_str())
            && event.details.get("platform").and_then(Value::as_str)
                == Some(platform_storage_name(link.platform))
            && event
                .details
                .get("mobile_evidence_kind")
                .and_then(Value::as_str)
                == Some(link.mobile_evidence_kind.as_str())
            && event
                .details
                .get("finding_instance_id")
                .and_then(Value::as_str)
                == Some(link.finding_instance_id.as_str())
    });
    if !event_matches {
        return Err(format!(
            "Evidence link {} has no matching release event.",
            link.id
        ));
    }
    Ok(())
}

fn verify_finding_resolution(
    release: &MobileRelease,
    candidates: &[ReleaseCandidate],
    audits: &[ReleaseAuditRecord],
    evidence_links: &[ReleaseEvidenceLink],
    events: &[ReleaseEvent],
    resolution: &FindingResolution,
) -> Result<(), String> {
    if resolution.release_id != release.id
        || resolution.reason.trim().is_empty()
        || resolution.actor.trim().is_empty()
    {
        return Err(format!("Resolution {} is incomplete.", resolution.id));
    }
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.id == resolution.candidate_id)
        .ok_or_else(|| {
            format!(
                "Resolution {} references a missing candidate.",
                resolution.id
            )
        })?;
    let audit = audits
        .iter()
        .find(|audit| audit.id == resolution.source_audit_id)
        .ok_or_else(|| format!("Resolution {} references a missing audit.", resolution.id))?;
    if candidate.platform != resolution.platform
        || audit.platform != resolution.platform
        || audit.candidate_id != candidate.id
    {
        return Err(format!(
            "Resolution {} binding is inconsistent.",
            resolution.id
        ));
    }
    for evidence_id in &resolution.evidence_ids {
        if !evidence_links
            .iter()
            .any(|link| link.evidence_id == *evidence_id && link.candidate_id == candidate.id)
        {
            return Err(format!(
                "Resolution {} references unlinked evidence {evidence_id}.",
                resolution.id
            ));
        }
    }
    let expected_evidence_ids = serde_json::to_value(&resolution.evidence_ids)
        .map_err(|error| format!("Failed to verify resolution evidence IDs: {error}"))?;
    let event_matches = events.iter().any(|event| {
        event.kind == ReleaseEventKind::FindingResolved
            && event.details.get("resolution_id").and_then(Value::as_str)
                == Some(resolution.id.as_str())
            && event
                .details
                .get("finding_instance_id")
                .and_then(Value::as_str)
                == Some(resolution.finding_instance_id.as_str())
            && event.details.get("reason").and_then(Value::as_str)
                == Some(resolution.reason.as_str())
            && event.details.get("candidate_id").and_then(Value::as_str)
                == Some(resolution.candidate_id.as_str())
            && event.details.get("source_audit_id").and_then(Value::as_str)
                == Some(resolution.source_audit_id.as_str())
            && event.details.get("platform").and_then(Value::as_str)
                == Some(platform_storage_name(resolution.platform))
            && event.details.get("evidence_ids") == Some(&expected_evidence_ids)
    });
    if !event_matches {
        return Err(format!(
            "Resolution {} has no matching release event.",
            resolution.id
        ));
    }
    Ok(())
}

fn verify_human_decision(
    release: &MobileRelease,
    candidates: &[ReleaseCandidate],
    audits: &[ReleaseAuditRecord],
    events: &[ReleaseEvent],
    decision: &HumanReleaseDecision,
) -> Result<(), String> {
    if decision.release_id != release.id
        || decision.decided_by.trim().is_empty()
        || decision.role.trim().is_empty()
        || decision.rationale.trim().is_empty()
    {
        return Err(format!("Decision {} is incomplete.", decision.id));
    }
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.id == decision.candidate_id)
        .ok_or_else(|| format!("Decision {} references a missing candidate.", decision.id))?;
    let audit = audits
        .iter()
        .find(|audit| audit.id == decision.audit_id)
        .ok_or_else(|| format!("Decision {} references a missing audit.", decision.id))?;
    if candidate.platform != decision.platform
        || candidate.artifact_sha256 != decision.candidate_sha256
        || audit.platform != decision.platform
        || audit.candidate_id != candidate.id
        || audit.candidate_sha256 != decision.candidate_sha256
        || audit.report_fingerprint != decision.report_fingerprint
        || audit.policy_digest != decision.policy_digest
        || (decision.decision == HumanDecisionKind::Approved
            && audit.decision == ReleaseAuditDecision::NoGo)
    {
        return Err(format!("Decision {} binding is inconsistent.", decision.id));
    }
    let expected_kind = match decision.decision {
        HumanDecisionKind::Approved => ReleaseEventKind::ReleaseApproved,
        HumanDecisionKind::Rejected => ReleaseEventKind::ReleaseRejected,
    };
    let expected_decision = match decision.decision {
        HumanDecisionKind::Approved => "approved",
        HumanDecisionKind::Rejected => "rejected",
    };
    let event_matches = events.iter().any(|event| {
        event.kind == expected_kind
            && event.actor.as_deref() == Some(decision.decided_by.as_str())
            && event.occurred_at == decision.created_at
            && event.details.get("decision_id").and_then(Value::as_str)
                == Some(decision.id.as_str())
            && event.details.get("candidate_id").and_then(Value::as_str)
                == Some(decision.candidate_id.as_str())
            && event
                .details
                .get("candidate_sha256")
                .and_then(Value::as_str)
                == Some(decision.candidate_sha256.as_str())
            && event.details.get("audit_id").and_then(Value::as_str)
                == Some(decision.audit_id.as_str())
            && event
                .details
                .get("report_fingerprint")
                .and_then(Value::as_str)
                == Some(decision.report_fingerprint.as_str())
            && event.details.get("policy_digest").and_then(Value::as_str)
                == Some(decision.policy_digest.as_str())
            && event.details.get("platform").and_then(Value::as_str)
                == Some(platform_storage_name(decision.platform))
            && event.details.get("decision").and_then(Value::as_str) == Some(expected_decision)
            && event.details.get("role").and_then(Value::as_str) == Some(decision.role.as_str())
            && event.details.get("rationale").and_then(Value::as_str)
                == Some(decision.rationale.as_str())
    });
    if !event_matches {
        return Err(format!(
            "Decision {} has no matching release event.",
            decision.id
        ));
    }
    Ok(())
}

fn verify_active_decisions(
    release: &MobileRelease,
    decisions: &[HumanReleaseDecision],
) -> Result<(), String> {
    for (platform, decision_id) in &release.active_decisions {
        let decision = decisions
            .iter()
            .find(|decision| &decision.id == decision_id)
            .ok_or_else(|| format!("Active decision {decision_id} is missing."))?;
        if &decision.platform != platform
            || release.active_candidates.get(platform) != Some(&decision.candidate_id)
            || release.active_audits.get(platform) != Some(&decision.audit_id)
        {
            return Err(format!(
                "Active decision {decision_id} is stale or inconsistent."
            ));
        }
    }
    if !release.active_decisions.is_empty()
        && release.stage != effective_decision_stage(release, decisions)
    {
        return Err(format!(
            "Release stage {} is inconsistent with its active decisions.",
            release.stage
        ));
    }
    Ok(())
}

fn verify_release_waiver(
    release: &MobileRelease,
    candidates: &[ReleaseCandidate],
    audits: &[ReleaseAuditRecord],
    events: &[ReleaseEvent],
    waiver: &ReleaseFindingWaiver,
) -> Result<(), String> {
    if waiver.release_id != release.id
        || waiver.reason.trim().is_empty()
        || waiver.approved_by.trim().is_empty()
    {
        return Err(format!("Waiver {} is incomplete.", waiver.id));
    }
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.id == waiver.candidate_id)
        .ok_or_else(|| format!("Waiver {} references a missing candidate.", waiver.id))?;
    let audit = audits
        .iter()
        .find(|audit| audit.id == waiver.audit_id)
        .ok_or_else(|| format!("Waiver {} references a missing audit.", waiver.id))?;
    if candidate.platform != waiver.platform
        || candidate.artifact_sha256 != waiver.candidate_sha256
        || audit.platform != waiver.platform
        || audit.candidate_id != candidate.id
        || audit.candidate_sha256 != waiver.candidate_sha256
        || audit.report_fingerprint != waiver.report_fingerprint
        || audit.policy_digest != waiver.policy_digest
    {
        return Err(format!("Waiver {} binding is inconsistent.", waiver.id));
    }
    validate_evidence_times(&waiver.created_at, Some(&waiver.expires_at))?;
    let event_matches = events.iter().any(|event| {
        event.kind == ReleaseEventKind::FindingWaived
            && event.actor.as_deref() == Some(waiver.approved_by.as_str())
            && event.occurred_at == waiver.created_at
            && event.details.get("waiver_id").and_then(Value::as_str) == Some(waiver.id.as_str())
            && event.details.get("candidate_id").and_then(Value::as_str)
                == Some(waiver.candidate_id.as_str())
            && event
                .details
                .get("candidate_sha256")
                .and_then(Value::as_str)
                == Some(waiver.candidate_sha256.as_str())
            && event.details.get("audit_id").and_then(Value::as_str)
                == Some(waiver.audit_id.as_str())
            && event.details.get("platform").and_then(Value::as_str)
                == Some(platform_storage_name(waiver.platform))
            && event
                .details
                .get("report_fingerprint")
                .and_then(Value::as_str)
                == Some(waiver.report_fingerprint.as_str())
            && event.details.get("policy_digest").and_then(Value::as_str)
                == Some(waiver.policy_digest.as_str())
            && event
                .details
                .get("finding_instance_id")
                .and_then(Value::as_str)
                == Some(waiver.finding_instance_id.as_str())
            && event.details.get("reason").and_then(Value::as_str) == Some(waiver.reason.as_str())
            && event.details.get("expires_at").and_then(Value::as_str)
                == Some(waiver.expires_at.as_str())
            && event
                .details
                .get("decision_unchanged")
                .and_then(Value::as_bool)
                == Some(true)
    });
    if !event_matches {
        return Err(format!(
            "Waiver {} has no matching release event.",
            waiver.id
        ));
    }
    Ok(())
}

fn verify_active_waivers(
    release: &MobileRelease,
    waivers: &[ReleaseFindingWaiver],
) -> Result<(), String> {
    for waiver_id in &release.active_waiver_ids {
        let waiver = waivers
            .iter()
            .find(|waiver| &waiver.id == waiver_id)
            .ok_or_else(|| format!("Active waiver {waiver_id} is missing."))?;
        if release.active_candidates.get(&waiver.platform) != Some(&waiver.candidate_id)
            || release.active_audits.get(&waiver.platform) != Some(&waiver.audit_id)
        {
            return Err(format!(
                "Active waiver {waiver_id} is stale or inconsistent."
            ));
        }
    }
    Ok(())
}

fn all_active_candidates_approved(
    release: &MobileRelease,
    decisions: &[HumanReleaseDecision],
) -> bool {
    !release.active_candidates.is_empty()
        && release
            .active_candidates
            .iter()
            .all(|(platform, candidate_id)| {
                release
                    .active_decisions
                    .get(platform)
                    .and_then(|decision_id| {
                        decisions
                            .iter()
                            .find(|decision| &decision.id == decision_id)
                    })
                    .is_some_and(|decision| {
                        decision.decision == HumanDecisionKind::Approved
                            && &decision.candidate_id == candidate_id
                            && release.active_audits.get(platform) == Some(&decision.audit_id)
                    })
            })
}

fn effective_decision_stage(
    release: &MobileRelease,
    decisions: &[HumanReleaseDecision],
) -> ReleaseStage {
    let has_active_rejection = release.active_decisions.values().any(|decision_id| {
        decisions.iter().any(|decision| {
            &decision.id == decision_id && decision.decision == HumanDecisionKind::Rejected
        })
    });
    if has_active_rejection {
        ReleaseStage::Rejected
    } else if all_active_candidates_approved(release, decisions) {
        ReleaseStage::Approved
    } else {
        ReleaseStage::InReview
    }
}

fn release_portfolio_item(
    view: &ReleaseView,
    evidence: &[crate::EvidenceRecord],
    now: chrono::DateTime<Utc>,
) -> (ReleasePortfolioItem, Vec<ReleaseReviewItem>) {
    let release = &view.release;
    let terminal = matches!(
        release.stage,
        ReleaseStage::Released | ReleaseStage::Superseded
    );
    let days_to_target = release.target_date.as_deref().and_then(|value| {
        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .ok()
            .map(|date| (date - now.date_naive()).num_days())
    });
    let mut queue = Vec::new();
    let mut counts = ReleaseAuditCounts {
        passed: 0,
        blockers: 0,
        warnings: 0,
        manual: 0,
        unavailable: 0,
    };
    let mut audit_decisions = Vec::new();

    let review_item =
        |kind, platform, severity: &str, title: &str, reason: String, next_action: &str, due_at| {
            ReleaseReviewItem {
                kind,
                release_id: release.id.clone(),
                release_version: release.version.clone(),
                release_name: release.name.clone(),
                owner: release.owner.clone(),
                platform,
                severity: severity.to_string(),
                title: title.to_string(),
                reason,
                next_action: next_action.to_string(),
                due_at,
            }
        };

    if !terminal {
        if let (Some(target_date), Some(days)) = (&release.target_date, days_to_target) {
            let due_at = Some(format!("{target_date}T00:00:00Z"));
            if days < 0 {
                queue.push(review_item(
                    ReleaseReviewKind::Overdue,
                    None,
                    "warning",
                    "Release target is overdue",
                    format!("The target date passed {} day(s) ago.", days.unsigned_abs()),
                    "Confirm the new target date or complete the remaining release work now.",
                    due_at,
                ));
            } else if days <= 7 {
                let reason = match days {
                    0 => "The release target is today.".to_string(),
                    1 => "The release target is tomorrow.".to_string(),
                    _ => format!("The release target is in {days} days."),
                };
                queue.push(review_item(
                    ReleaseReviewKind::DueSoon,
                    None,
                    "review",
                    "Release target is approaching",
                    reason,
                    "Confirm the submission, approval, and rollout owners before the target date.",
                    due_at,
                ));
            }
        }
    }

    if !terminal && release.owner.as_deref().is_none_or(str::is_empty) {
        queue.push(review_item(
            ReleaseReviewKind::MissingOwner,
            None,
            "review",
            "Release owner required",
            "No accountable release owner is recorded.".to_string(),
            "Assign the person responsible for coordinating this release.",
            None,
        ));
    }
    if !terminal && release.active_candidates.is_empty() {
        queue.push(review_item(
            ReleaseReviewKind::MissingCandidate,
            None,
            "review",
            "Candidate not bound",
            "No active IPA, AAB, or APK is attached to this release.".to_string(),
            "Bind the exact store-upload candidate and record its SHA-256 identity.",
            None,
        ));
    }

    for (platform, candidate_id) in &release.active_candidates {
        let Some(audit_id) = release.active_audits.get(platform) else {
            if !terminal {
                queue.push(review_item(
                    ReleaseReviewKind::MissingAudit,
                    Some(*platform),
                    "review",
                    "Candidate audit required",
                    format!("Candidate {candidate_id} has no active verified audit."),
                    "Run the active candidate audit before requesting approval.",
                    None,
                ));
            }
            continue;
        };
        let Some(audit) = view.audits.iter().find(|audit| &audit.id == audit_id) else {
            continue;
        };
        counts.passed += audit.counts.passed;
        counts.blockers += audit.counts.blockers;
        counts.warnings += audit.counts.warnings;
        counts.manual += audit.counts.manual;
        counts.unavailable += audit.counts.unavailable;
        audit_decisions.push(audit.decision);

        if !terminal && (audit.decision == ReleaseAuditDecision::NoGo || audit.counts.blockers > 0)
        {
            queue.push(review_item(
                ReleaseReviewKind::Blocked,
                Some(*platform),
                "blocker",
                "Automated release blockers",
                format!(
                    "{} audit {} is {} with {} blocker(s).",
                    platform, audit.id, audit.decision, audit.counts.blockers
                ),
                "Resolve the blocking findings, rebuild if needed, and record a new audit.",
                None,
            ));
        }

        let decision = release
            .active_decisions
            .get(platform)
            .and_then(|decision_id| {
                view.decisions
                    .iter()
                    .find(|decision| &decision.id == decision_id)
            });
        if !terminal && decision.is_some_and(|value| value.decision == HumanDecisionKind::Rejected)
        {
            queue.push(review_item(
                ReleaseReviewKind::Rejected,
                Some(*platform),
                "blocker",
                "Candidate rejected",
                format!(
                    "The active {} candidate was rejected by {}.",
                    platform,
                    decision
                        .map(|value| value.decided_by.as_str())
                        .unwrap_or("a reviewer")
                ),
                "Address the rejection rationale and bind or audit the corrected candidate.",
                None,
            ));
        } else if !terminal
            && decision.is_some_and(|value| value.decision == HumanDecisionKind::Approved)
            && audit.decision == ReleaseAuditDecision::AtRisk
        {
            queue.push(review_item(
                ReleaseReviewKind::ApprovedWithRisk,
                Some(*platform),
                "risk",
                "Approved candidate still carries open risk",
                format!(
                    "The active {} candidate is approved, but audit {} remains At Risk with {} warning(s), {} human-evidence item(s), and {} unavailable check(s).",
                    platform,
                    audit.id,
                    audit.counts.warnings,
                    audit.counts.manual,
                    audit.counts.unavailable,
                ),
                "Resolve or explicitly waive the remaining work, rerun the audit, and refresh the handoff packet.",
                None,
            ));
        } else if !terminal && decision.is_none() && audit.decision != ReleaseAuditDecision::NoGo {
            queue.push(review_item(
                ReleaseReviewKind::ApprovalRequired,
                Some(*platform),
                "review",
                "Human approval required",
                format!(
                    "The active {} audit is {}, but no candidate-bound decision is recorded.",
                    platform, audit.decision
                ),
                "Review the evidence and record an accountable approval or rejection.",
                None,
            ));
        }
    }

    let seven_days_from_now = now + chrono::Duration::days(7);
    if !terminal {
        for link in view.evidence_links.iter().filter(|link| {
            release.active_candidates.get(&link.platform) == Some(&link.candidate_id)
                && release.active_audits.get(&link.platform) == Some(&link.source_audit_id)
        }) {
            let Some(record) = evidence.iter().find(|record| record.id == link.evidence_id) else {
                continue;
            };
            let Some(expires_at) = record.expires_at.as_deref() else {
                continue;
            };
            let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(expires_at)
                .map(|value| value.with_timezone(&Utc))
            else {
                continue;
            };
            if expiry <= seven_days_from_now {
                let expired = expiry <= now;
                queue.push(review_item(
                    ReleaseReviewKind::ExpiringEvidence,
                    Some(link.platform),
                    if expired { "blocker" } else { "warning" },
                    if expired { "Release evidence expired" } else { "Release evidence expires soon" },
                    if expired {
                        format!("Evidence {} ({}) expired at {expires_at}.", record.id, record.title)
                    } else {
                        format!("Evidence {} ({}) expires at {expires_at}.", record.id, record.title)
                    },
                    "Replace or renew the evidence, then rerun the candidate audit before approval.",
                    Some(expires_at.to_string()),
                ));
            }
        }
        for waiver in view.waivers.iter().filter(|waiver| {
            release.active_waiver_ids.contains(&waiver.id)
                && !waiver_has_expired(&waiver.expires_at)
        }) {
            queue.push(review_item(
                ReleaseReviewKind::AcceptedRisk,
                Some(waiver.platform),
                "risk",
                "Accepted risk remains active",
                format!(
                    "{} accepted {} risk for {}.",
                    waiver.approved_by, waiver.platform, waiver.finding_instance_id
                ),
                "Confirm the waiver is still necessary and visible in the final handoff.",
                Some(waiver.expires_at.clone()),
            ));
            let expires_at = chrono::DateTime::parse_from_rfc3339(&waiver.expires_at)
                .map(|value| value.with_timezone(&Utc));
            if expires_at.is_ok_and(|value| value <= seven_days_from_now) {
                queue.push(review_item(
                    ReleaseReviewKind::ExpiringWaiver,
                    Some(waiver.platform),
                    "warning",
                    "Risk acceptance expires soon",
                    format!("Waiver {} expires at {}.", waiver.id, waiver.expires_at),
                    "Renew with current evidence or resolve the underlying finding before expiry.",
                    Some(waiver.expires_at.clone()),
                ));
            }
        }
    }

    queue.sort_by(|left, right| {
        review_kind_rank(left.kind)
            .cmp(&review_kind_rank(right.kind))
            .then_with(|| left.platform.cmp(&right.platform))
    });
    let review_kinds = queue
        .iter()
        .map(|item| item.kind)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let automated_decision =
        if audit_decisions.is_empty() || audit_decisions.len() != release.active_candidates.len() {
            None
        } else if audit_decisions.contains(&ReleaseAuditDecision::NoGo) {
            Some(ReleaseAuditDecision::NoGo)
        } else if audit_decisions.contains(&ReleaseAuditDecision::AtRisk) {
            Some(ReleaseAuditDecision::AtRisk)
        } else {
            Some(ReleaseAuditDecision::Go)
        };
    let age_days = chrono::DateTime::parse_from_rfc3339(&release.updated_at)
        .map(|updated| (now - updated.with_timezone(&Utc)).num_days().max(0))
        .unwrap_or_default();
    let next_action = queue
        .first()
        .map(|item| item.next_action.clone())
        .unwrap_or_else(|| {
            if terminal {
                "Retain the verified packet and history as the final release record.".to_string()
            } else if release.stage == ReleaseStage::Approved {
                "Prepare the approved candidate for submission and preserve the handoff packet."
                    .to_string()
            } else {
                "Continue monitoring the release record for candidate or policy changes."
                    .to_string()
            }
        });
    let active_waivers = view
        .waivers
        .iter()
        .filter(|waiver| {
            release.active_waiver_ids.contains(&waiver.id)
                && !waiver_has_expired(&waiver.expires_at)
        })
        .count();
    let expiring_waivers = view
        .waivers
        .iter()
        .filter(|waiver| {
            release.active_waiver_ids.contains(&waiver.id)
                && chrono::DateTime::parse_from_rfc3339(&waiver.expires_at)
                    .map(|value| {
                        let value = value.with_timezone(&Utc);
                        value > now && value <= seven_days_from_now
                    })
                    .unwrap_or(false)
        })
        .count();
    let expiring_evidence = queue
        .iter()
        .filter(|item| item.kind == ReleaseReviewKind::ExpiringEvidence)
        .count();
    let mut platforms = release
        .active_candidates
        .keys()
        .copied()
        .collect::<Vec<_>>();
    platforms.sort();
    (
        ReleasePortfolioItem {
            id: release.id.clone(),
            version: release.version.clone(),
            name: release.name.clone(),
            owner: release.owner.clone(),
            target_date: release.target_date.clone(),
            submission_date: release.submission_date.clone(),
            rollout_date: release.rollout_date.clone(),
            days_to_target,
            stage: release.stage,
            updated_at: release.updated_at.clone(),
            age_days,
            platforms,
            automated_decision,
            counts,
            active_decisions: release.active_decisions.len(),
            active_waivers,
            expiring_evidence,
            expiring_waivers,
            needs_attention: !queue.is_empty(),
            next_action,
            review_kinds,
        },
        queue,
    )
}

fn review_kind_rank(kind: ReleaseReviewKind) -> u8 {
    match kind {
        ReleaseReviewKind::Blocked | ReleaseReviewKind::Rejected => 0,
        ReleaseReviewKind::Overdue
        | ReleaseReviewKind::MissingCandidate
        | ReleaseReviewKind::MissingAudit => 1,
        ReleaseReviewKind::ExpiringEvidence | ReleaseReviewKind::ExpiringWaiver => 2,
        ReleaseReviewKind::DueSoon
        | ReleaseReviewKind::ApprovalRequired
        | ReleaseReviewKind::MissingOwner => 3,
        ReleaseReviewKind::ApprovedWithRisk | ReleaseReviewKind::AcceptedRisk => 4,
    }
}

fn review_severity_rank(severity: &str) -> u8 {
    match severity {
        "blocker" => 0,
        "warning" => 1,
        "review" => 2,
        "risk" => 3,
        _ => 4,
    }
}

fn stage_after_context_change(
    release: &MobileRelease,
    decisions: &[HumanReleaseDecision],
    fallback: ReleaseStage,
) -> ReleaseStage {
    if release.active_decisions.is_empty() {
        fallback
    } else {
        effective_decision_stage(release, decisions)
    }
}

fn waiver_has_expired(expires_at: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(expires_at)
        .is_ok_and(|expires_at| expires_at.with_timezone(&Utc) <= Utc::now())
}

fn time_is_at_or_before(value: &str, reference: &str) -> bool {
    let Ok(value) = chrono::DateTime::parse_from_rfc3339(value) else {
        return false;
    };
    let Ok(reference) = chrono::DateTime::parse_from_rfc3339(reference) else {
        return false;
    };
    value <= reference
}

fn calculate_release_packet_digest(packet: &ReleasePacket) -> Result<String, String> {
    let mut unsigned = packet.clone();
    unsigned.packet_id.clear();
    unsigned.packet_digest.clear();
    sha256_serialized(&unsigned)
}

pub fn verify_release_packet(packet: &ReleasePacket) -> Result<(), String> {
    if packet.schema_version != RELEASE_SCHEMA_VERSION
        || packet.assembled_at != packet.release.updated_at
        || packet.packet_id.len() != 14
    {
        return Err("Release packet identity is inconsistent.".to_string());
    }
    let digest = calculate_release_packet_digest(packet)?;
    let expected_id = format!("PKT-{}", &digest[..10].to_ascii_uppercase());
    if packet.packet_digest != digest || packet.packet_id != expected_id {
        return Err(format!(
            "Release packet {} digest does not verify.",
            packet.packet_id
        ));
    }
    verify_event_chain(&packet.events)?;
    let event_head_matches = packet.events.last().is_some_and(|event| {
        event.sequence == packet.release.latest_event_sequence
            && packet.release.latest_event_hash.as_ref() == Some(&event.hash)
    });
    if !event_head_matches || packet.audits.len() != packet.reports.len() {
        return Err(format!(
            "Release packet {} history is incomplete.",
            packet.packet_id
        ));
    }
    for (audit, report) in packet.audits.iter().zip(&packet.reports) {
        let fingerprint = crate::mobile::calculate_mobile_report_fingerprint(report)?;
        if audit.report_fingerprint != fingerprint
            || report.report_fingerprint != fingerprint
            || audit.policy_digest != report.policy_digest
            || release_audit_decision(&report.decision) != audit.decision
        {
            return Err(format!(
                "Release packet {} contains an inconsistent audit {}.",
                packet.packet_id, audit.id
            ));
        }
    }
    Ok(())
}

pub fn render_release_packet_json(packet: &ReleasePacket) -> Result<String, String> {
    verify_release_packet(packet)?;
    serde_json::to_string_pretty(packet)
        .map(|json| format!("{json}\n"))
        .map_err(|error| format!("Failed to serialize release packet: {error}"))
}

pub fn render_release_handoff_summary(packet: &ReleasePacket) -> String {
    let blocked = packet
        .active_platforms
        .iter()
        .any(|platform| platform.automated_decision == Some(ReleaseAuditDecision::NoGo));
    let rejected = packet
        .active_platforms
        .iter()
        .any(|platform| platform.human_decision == Some(HumanDecisionKind::Rejected));
    let pending = packet
        .active_platforms
        .iter()
        .any(|platform| platform.human_decision.is_none());
    let approved_with_risk = packet.active_platforms.iter().any(|platform| {
        platform.human_decision == Some(HumanDecisionKind::Approved)
            && platform.automated_decision == Some(ReleaseAuditDecision::AtRisk)
    });
    let status = if blocked {
        "BLOCKED — deterministic audit requirements are not satisfied"
    } else if rejected {
        "REJECTED — a candidate-bound human rejection is active"
    } else if packet.active_platforms.is_empty() {
        "DRAFT — no active store candidate is bound"
    } else if pending {
        "AWAITING APPROVAL — review is not complete for every active platform"
    } else if approved_with_risk {
        "APPROVED WITH RISK — accountable approval exists while automated findings remain open"
    } else {
        "APPROVED HANDOFF — every active platform has an accountable decision"
    };
    let next_action = if blocked {
        "Resolve blockers and rerun the audit against the corrected candidate."
    } else if rejected {
        "Address the rejection rationale, then bind or audit the corrected candidate."
    } else if packet.active_platforms.is_empty() {
        "Bind the exact IPA, AAB, or APK intended for store upload."
    } else if pending {
        "Review the verified evidence and record the missing approval or rejection."
    } else if approved_with_risk {
        "Resolve or explicitly waive remaining findings, rerun the audit, and refresh this handoff."
    } else {
        "Transfer the exact approved artifacts with this verified packet and preserve its digest."
    };
    let accepted_risk = packet
        .active_platforms
        .iter()
        .map(|platform| platform.effective_waiver_ids.len())
        .sum::<usize>();
    let unresolved = packet
        .active_platforms
        .iter()
        .map(|platform| platform.unresolved_finding_ids.len())
        .sum::<usize>();
    let mut output = format!(
        "## Handoff summary\n\n- Status: **{status}**\n- Owner: {}\n- Target date: {}\n- Submission date: {}\n- Rollout date: {}\n- Exact candidates: {}\n- Unresolved findings: {}\n- Accepted risks: {}\n- Next action: {}\n\n",
        packet.release.owner.as_deref().unwrap_or("Unassigned"),
        packet
            .release
            .target_date
            .as_deref()
            .unwrap_or("Unscheduled"),
        packet
            .release
            .submission_date
            .as_deref()
            .unwrap_or("Unscheduled"),
        packet
            .release
            .rollout_date
            .as_deref()
            .unwrap_or("Unscheduled"),
        packet.active_platforms.len(),
        unresolved,
        accepted_risk,
        next_action,
    );
    for platform in &packet.active_platforms {
        output.push_str(&format!(
            "- **{}**: candidate `{}` / SHA `{}` / audit **{}** / decision **{}** / {} open finding(s) / {} active waiver(s)\n",
            platform.platform,
            platform.candidate_id,
            &platform.candidate_sha256[..12],
            platform
                .automated_decision
                .map(|decision| decision.to_string())
                .unwrap_or_else(|| "Not audited".to_string()),
            platform
                .human_decision
                .map(human_decision_name)
                .unwrap_or("Pending"),
            platform.unresolved_finding_ids.len(),
            platform.effective_waiver_ids.len(),
        ));
    }
    output
}

pub fn render_release_packet_markdown(packet: &ReleasePacket) -> String {
    let mut output = format!(
        "# Gantry Release Packet\n\n- Release: `{}` (`{}`)\n- Stage: **{}**\n- Packet: `{}`\n- Digest: `{}`\n- State as of: `{}`\n- Event chain: {} verified event(s), head `{}`\n\n",
        packet.release.version,
        packet.release.id,
        packet.release.stage,
        packet.packet_id,
        packet.packet_digest,
        packet.assembled_at,
        packet.events.len(),
        packet
            .release
            .latest_event_hash
            .as_deref()
            .unwrap_or("none")
    );
    output.push_str(&render_release_handoff_summary(packet));
    output.push_str("\n## Active platforms\n\n");
    for platform in &packet.active_platforms {
        output.push_str(&format!(
            "### {}\n\n- Candidate: `{}` (`{}`)\n- Audit: `{}`\n- Automated decision: **{}**\n- Human decision: **{}**\n- Open findings: {}\n- Effective waivers: {}\n- Expired waivers: {}\n\n",
            platform.platform,
            platform.candidate_id,
            platform.candidate_sha256,
            platform.audit_id.as_deref().unwrap_or("not audited"),
            platform
                .automated_decision
                .map(|decision| decision.to_string())
                .unwrap_or_else(|| "Not audited".to_string()),
            platform
                .human_decision
                .map(human_decision_name)
                .unwrap_or("Pending"),
            platform.unresolved_finding_ids.len(),
            packet_list(&platform.effective_waiver_ids),
            packet_list(&platform.expired_waiver_ids),
        ));
    }
    output.push_str("## Accountable decisions\n\n");
    if packet.decisions.is_empty() {
        output.push_str("No decisions recorded.\n\n");
    }
    for decision in &packet.decisions {
        output.push_str(&format!(
            "- `{}`: **{}** {} candidate `{}` by {} ({}) — {}\n",
            decision.id,
            human_decision_name(decision.decision),
            decision.platform,
            decision.candidate_id,
            markdown_inline(&decision.decided_by),
            markdown_inline(&decision.role),
            markdown_inline(&decision.rationale),
        ));
    }
    output.push_str("\n## Accepted risk\n\n");
    if packet.waivers.is_empty() {
        output.push_str("No waivers recorded.\n\n");
    }
    for waiver in &packet.waivers {
        output.push_str(&format!(
            "- `{}`: finding `{}` on {} approved by {} until `{}` — {}\n",
            waiver.id,
            waiver.finding_instance_id,
            waiver.platform,
            markdown_inline(&waiver.approved_by),
            waiver.expires_at,
            markdown_inline(&waiver.reason),
        ));
    }
    output.push_str("\n## Unresolved findings\n\n");
    let mut unresolved = 0;
    for platform in &packet.active_platforms {
        for finding_id in &platform.unresolved_finding_ids {
            if let Some(finding) = packet
                .reports
                .iter()
                .flat_map(|report| &report.findings)
                .find(|finding| &finding.finding_instance_id == finding_id)
            {
                unresolved += 1;
                output.push_str(&format!(
                    "- **{:?} / {:?}** `{}` {} — {}\n",
                    finding.status,
                    finding.severity,
                    finding.finding_instance_id,
                    markdown_inline(&finding.title),
                    markdown_inline(&finding.remediation),
                ));
            }
        }
    }
    if unresolved == 0 {
        output.push_str("No unresolved findings in active audits.\n");
    }
    output.push_str("\n## Verified timeline\n\n");
    for event in &packet.events {
        output.push_str(&format!(
            "{}. `{:?}` at `{}`{}\n",
            event.sequence,
            event.kind,
            event.occurred_at,
            event
                .actor
                .as_ref()
                .map(|actor| format!(" by {}", markdown_inline(actor)))
                .unwrap_or_default(),
        ));
    }
    output
}

pub fn render_release_packet_html(packet: &ReleasePacket) -> String {
    let markdown = render_release_packet_markdown(packet);
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Gantry release {} packet</title><style>body{{font:15px/1.55 system-ui,sans-serif;max-width:1080px;margin:40px auto;padding:0 24px;color:#182029}}header{{border-bottom:1px solid #dce2e8;margin-bottom:24px}}pre{{white-space:pre-wrap;overflow-wrap:anywhere;background:#f7f8fa;border:1px solid #dce2e8;border-radius:8px;padding:24px}}code{{font-family:ui-monospace,monospace}}</style></head><body><header><h1>Gantry verified release packet</h1><p>Release <code>{}</code> · packet <code>{}</code></p></header><pre>{}</pre></body></html>\n",
        html_escape(&packet.release.version),
        html_escape(&packet.release.id),
        html_escape(&packet.packet_id),
        html_escape(&markdown),
    )
}

fn human_decision_name(decision: HumanDecisionKind) -> &'static str {
    match decision {
        HumanDecisionKind::Approved => "Approved",
        HumanDecisionKind::Rejected => "Rejected",
    }
}

fn packet_list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values
            .iter()
            .map(|value| format!("`{value}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn markdown_inline(value: &str) -> String {
    value.replace('\n', " ").replace('|', "\\|")
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn portable_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn finding_map(
    report: Option<&crate::mobile::MobileAuditReport>,
) -> BTreeMap<&str, &crate::mobile::MobileFinding> {
    report
        .map(|report| {
            report
                .findings
                .iter()
                .map(|finding| (finding.finding_instance_id.as_str(), finding))
                .collect()
        })
        .unwrap_or_default()
}

fn finding_is_open(status: &crate::mobile::MobileFindingStatus) -> bool {
    status != &crate::mobile::MobileFindingStatus::Pass
}

fn finding_delta(
    finding_instance_id: &str,
    from: Option<&crate::mobile::MobileFinding>,
    to: Option<&crate::mobile::MobileFinding>,
) -> ReleaseFindingDelta {
    ReleaseFindingDelta {
        finding_instance_id: finding_instance_id.to_string(),
        title: to
            .or(from)
            .map(|finding| finding.title.clone())
            .unwrap_or_default(),
        from_status: from.map(|finding| finding.status.clone()),
        to_status: to.map(|finding| finding.status.clone()),
        from_severity: from.map(|finding| finding.severity.clone()),
        to_severity: to.map(|finding| finding.severity.clone()),
    }
}

fn platform_storage_name(platform: MobilePlatform) -> &'static str {
    match platform {
        MobilePlatform::Ios => "ios",
        MobilePlatform::Android => "android",
    }
}

fn normalize_evidence_kind(value: &str) -> Result<String, String> {
    let normalized = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if normalized.is_empty() {
        Err("Release mobile evidence kind cannot be empty.".to_string())
    } else {
        Ok(normalized)
    }
}

fn required_actor(actor: Option<String>) -> Result<String, String> {
    optional_trimmed(actor)
        .or_else(current_actor)
        .ok_or_else(|| "An actor is required. Pass --actor or set GANTRY_ACTOR.".to_string())
}

fn validate_optional_rfc3339(value: Option<String>) -> Result<Option<String>, String> {
    value
        .map(|value| {
            chrono::DateTime::parse_from_rfc3339(&value)
                .map(|_| value)
                .map_err(|_| "Evidence expiry must be valid RFC 3339.".to_string())
        })
        .transpose()
}

fn validate_optional_date(label: &str, value: Option<String>) -> Result<Option<String>, String> {
    value
        .map(|value| {
            let value = required_trimmed(label, &value)?;
            NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                .map(|_| value)
                .map_err(|_| format!("{label} must use YYYY-MM-DD."))
        })
        .transpose()
}

fn validate_evidence_times(created_at: &str, expires_at: Option<&str>) -> Result<(), String> {
    let created = chrono::DateTime::parse_from_rfc3339(created_at)
        .map_err(|_| "Evidence creation time must be valid RFC 3339.".to_string())?;
    if let Some(expires_at) = expires_at {
        let expires = chrono::DateTime::parse_from_rfc3339(expires_at)
            .map_err(|_| "Evidence expiry must be valid RFC 3339.".to_string())?;
        if expires <= created {
            return Err("Evidence expiry must be later than its creation time.".to_string());
        }
    }
    Ok(())
}

fn classify_artifact(path: &Path) -> Result<(MobilePlatform, CandidateArtifactKind), String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "ipa" => Ok((MobilePlatform::Ios, CandidateArtifactKind::Ipa)),
        "aab" => Ok((MobilePlatform::Android, CandidateArtifactKind::Aab)),
        "apk" => Ok((MobilePlatform::Android, CandidateArtifactKind::Apk)),
        _ => Err("Release candidates must be an .ipa, .aab, or .apk file.".to_string()),
    }
}

fn hash_artifact(path: &Path) -> Result<(String, u64), String> {
    let mut file = File::open(path)
        .map_err(|error| format!("Failed to open artifact {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Failed to read artifact {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((hex::encode(hasher.finalize()), size))
}

fn portable_artifact_path(path: &Path, project_root: &Path) -> (String, bool) {
    let canonical_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    if let Ok(relative) = path.strip_prefix(&canonical_root) {
        (relative.to_string_lossy().to_string(), true)
    } else {
        (path.to_string_lossy().to_string(), false)
    }
}

#[derive(Serialize)]
struct UnsignedEvent<'a> {
    schema_version: &'a str,
    release_id: &'a str,
    sequence: u64,
    occurred_at: &'a str,
    kind: ReleaseEventKind,
    actor: &'a Option<String>,
    details: &'a Value,
    previous_hash: &'a Option<String>,
}

fn build_event(
    release: &MobileRelease,
    kind: ReleaseEventKind,
    actor: Option<String>,
    details: Value,
) -> Result<ReleaseEvent, String> {
    let sequence = release.latest_event_sequence + 1;
    let occurred_at = timestamp();
    let previous_hash = release.latest_event_hash.clone();
    let unsigned = UnsignedEvent {
        schema_version: RELEASE_SCHEMA_VERSION,
        release_id: &release.id,
        sequence,
        occurred_at: &occurred_at,
        kind,
        actor: &actor,
        details: &details,
        previous_hash: &previous_hash,
    };
    let bytes = serde_json::to_vec(&unsigned)
        .map_err(|error| format!("Failed to serialize release event: {error}"))?;
    let hash = hex::encode(Sha256::digest(bytes));
    Ok(ReleaseEvent {
        schema_version: RELEASE_SCHEMA_VERSION.to_string(),
        release_id: release.id.clone(),
        sequence,
        occurred_at,
        kind,
        actor,
        details,
        previous_hash,
        hash,
    })
}

fn apply_event_head(release: &mut MobileRelease, event: &ReleaseEvent) {
    release.latest_event_sequence = event.sequence;
    release.latest_event_hash = Some(event.hash.clone());
}

pub fn verify_event_chain(events: &[ReleaseEvent]) -> Result<(), String> {
    let mut previous_hash: Option<String> = None;
    for (index, event) in events.iter().enumerate() {
        let expected_sequence = index as u64 + 1;
        if event.sequence != expected_sequence {
            return Err(format!(
                "Release event sequence is broken at event {}.",
                event.sequence
            ));
        }
        if event.previous_hash != previous_hash {
            return Err(format!(
                "Release event previous hash is broken at sequence {}.",
                event.sequence
            ));
        }
        let unsigned = UnsignedEvent {
            schema_version: &event.schema_version,
            release_id: &event.release_id,
            sequence: event.sequence,
            occurred_at: &event.occurred_at,
            kind: event.kind,
            actor: &event.actor,
            details: &event.details,
            previous_hash: &event.previous_hash,
        };
        let bytes = serde_json::to_vec(&unsigned)
            .map_err(|error| format!("Failed to serialize release event: {error}"))?;
        let expected_hash = hex::encode(Sha256::digest(bytes));
        if event.hash != expected_hash {
            return Err(format!(
                "Release event hash is broken at sequence {}.",
                event.sequence
            ));
        }
        previous_hash = Some(event.hash.clone());
    }
    Ok(())
}

fn read_events(path: &Path) -> Result<Vec<ReleaseEvent>, String> {
    let file =
        File::open(path).map_err(|error| format!("Failed to open {}: {error}", path.display()))?;
    let mut events = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        events.push(serde_json::from_str(&line).map_err(|error| {
            format!(
                "Failed to parse {} line {}: {error}",
                path.display(),
                index + 1
            )
        })?);
    }
    Ok(events)
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    atomic_write(path, &json_bytes(value)?)
}

fn events_bytes(events: &[ReleaseEvent]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    for event in events {
        serde_json::to_writer(&mut bytes, event)
            .map_err(|error| format!("Failed to serialize release event: {error}"))?;
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("Failed to serialize release record: {error}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Cannot determine parent directory for {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
    let temp_path = parent.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("gantry"),
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut file = File::create(&temp_path)
        .map_err(|error| format!("Failed to create {}: {error}", temp_path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("Failed to write {}: {error}", temp_path.display()))?;
    file.sync_all()
        .map_err(|error| format!("Failed to sync {}: {error}", temp_path.display()))?;
    fs::rename(&temp_path, path)
        .map_err(|error| format!("Failed to publish {}: {error}", path.display()))?;
    sync_directory(parent)?;
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), String> {
    let directory = File::open(path).map_err(|error| {
        format!(
            "Failed to open directory {} for sync: {error}",
            path.display()
        )
    })?;
    directory
        .sync_all()
        .map_err(|error| format!("Failed to sync directory {}: {error}", path.display()))
}

fn safe_relative_transaction_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!("Unsafe transaction path '{value}'."));
    }
    Ok(path)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))
}

fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn generated_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_le_bytes(),
    );
    let hash = hex::encode_upper(hasher.finalize());
    format!("{prefix}-{}", &hash[..10])
}

fn sha256_serialized<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("Failed to calculate record digest: {error}"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn required_trimmed(label: &str, value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("Release {label} cannot be empty."))
    } else {
        Ok(trimmed.to_string())
    }
}

fn optional_trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn current_actor() -> Option<String> {
    std::env::var("GANTRY_ACTOR")
        .ok()
        .or_else(|| std::env::var("USER").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    fn fixture(name: &str) -> (PathBuf, ReleaseStore) {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("gantry_release_{name}_{unique}"));
        let vault = root.join(".gantry");
        fs::create_dir_all(&vault).expect("vault should be created");
        let store = ReleaseStore::new(&vault, &root);
        (root, store)
    }

    fn write_android_fixture(root: &Path, file_name: &str, package: &str) -> PathBuf {
        let source_manifest = root.join("android/app/src/main/AndroidManifest.xml");
        fs::create_dir_all(source_manifest.parent().expect("manifest parent"))
            .expect("Android source directory should be created");
        let manifest = format!(
            r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="{package}"><uses-sdk android:minSdkVersion="24" android:targetSdkVersion="35"/><application android:debuggable="false" android:usesCleartextTraffic="false"/></manifest>"#
        );
        fs::write(&source_manifest, &manifest).expect("source manifest should be written");
        fs::write(
            root.join("android/app/build.gradle.kts"),
            format!(
                r#"plugins {{ id("com.android.application") }}
android {{ namespace = "{package}"; compileSdk = 35
defaultConfig {{ applicationId = "{package}"; minSdk = 24; targetSdk = 35; versionCode = 1; versionName = "1.0" }}
buildTypes {{ getByName("release") {{ isDebuggable = false }} }} }}"#
            ),
        )
        .expect("Gradle fixture should be written");

        let artifact = root.join("build").join(file_name);
        fs::create_dir_all(artifact.parent().expect("artifact parent"))
            .expect("artifact directory should be created");
        let file = File::create(&artifact).expect("APK fixture should be created");
        let mut archive = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        archive
            .start_file("AndroidManifest.xml", options)
            .expect("manifest entry should start");
        archive
            .write_all(manifest.as_bytes())
            .expect("manifest entry should be written");
        archive
            .start_file("META-INF/CERT.RSA", options)
            .expect("signature entry should start");
        archive
            .write_all(b"synthetic signature fixture")
            .expect("signature fixture should be written");
        archive.finish().expect("APK fixture should finish");
        artifact
    }

    #[test]
    fn creates_and_loads_release_with_verified_event() {
        let (root, store) = fixture("create");
        let release = store
            .create(NewRelease {
                version: "2.4.0".to_string(),
                name: Some("Summer release".to_string()),
                owner: Some("release@example.com".to_string()),
                actor: Some("test-runner".to_string()),
            })
            .expect("release should be created");

        let view = store.load(&release.id).expect("release should load");
        assert_eq!(view.release.version, "2.4.0");
        assert_eq!(view.release.stage, ReleaseStage::Draft);
        assert_eq!(view.events.len(), 1);
        assert_eq!(view.events[0].kind, ReleaseEventKind::ReleaseCreated);
        verify_event_chain(&view.events).expect("event chain should verify");
        let ensured = store
            .ensure_release(NewRelease {
                version: "2.4.0".to_string(),
                name: Some("Ignored retry metadata".to_string()),
                ..NewRelease::default()
            })
            .expect("an existing verified release should be reused");
        assert_eq!(ensured.id, release.id);
        assert_eq!(
            store
                .load(&release.id)
                .expect("release should reload")
                .events
                .len(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn schedules_release_and_projects_verified_urgency() {
        let (root, store) = fixture("schedule");
        let release = store
            .create(NewRelease {
                version: "2.5.0".to_string(),
                owner: Some("release@example.com".to_string()),
                ..NewRelease::default()
            })
            .expect("release should be created");
        let target = (Utc::now().date_naive() + chrono::Duration::days(5))
            .format("%Y-%m-%d")
            .to_string();
        let invalid = store
            .schedule(NewReleaseSchedule {
                release: release.id.clone(),
                target_date: Some("07/20/2026".to_string()),
                submission_date: None,
                rollout_date: None,
                actor: Some("release@example.com".to_string()),
            })
            .expect_err("ambiguous dates should fail closed");
        assert!(invalid.contains("YYYY-MM-DD"));
        let scheduled = store
            .schedule(NewReleaseSchedule {
                release: release.id.clone(),
                target_date: Some(target.clone()),
                submission_date: None,
                rollout_date: None,
                actor: Some("release@example.com".to_string()),
            })
            .expect("schedule should be recorded");
        assert_eq!(scheduled.target_date.as_deref(), Some(target.as_str()));
        let view = store.load(&release.id).expect("schedule should verify");
        assert_eq!(view.events.len(), 2);
        assert_eq!(view.events[1].kind, ReleaseEventKind::ReleaseScheduled);
        let portfolio = store.portfolio().expect("portfolio should project urgency");
        assert_eq!(portfolio.counts.due_soon, 1);
        assert_eq!(portfolio.releases[0].days_to_target, Some(5));
        assert!(
            portfolio
                .review_queue
                .iter()
                .any(|item| item.kind == ReleaseReviewKind::DueSoon)
        );
        let schemas = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        assert_schema_accepts(
            &schemas.join("mobile-release.schema.json"),
            &serde_json::to_value(&scheduled).expect("release should serialize"),
        );
        assert_schema_accepts(
            &schemas.join("release-event.schema.json"),
            &serde_json::to_value(&view.events[1]).expect("event should serialize"),
        );
        assert_schema_accepts(
            &schemas.join("release-portfolio.schema.json"),
            &serde_json::to_value(&portfolio).expect("portfolio should serialize"),
        );

        let release_path = root
            .join(".gantry/releases")
            .join(&release.id)
            .join("release.json");
        let mut changed: Value =
            serde_json::from_slice(&fs::read(&release_path).expect("release should be readable"))
                .expect("release should parse");
        changed["target_date"] = Value::String("2099-01-01".to_string());
        fs::write(
            &release_path,
            serde_json::to_vec_pretty(&changed).expect("release should serialize"),
        )
        .expect("release should be modified");
        assert!(
            store
                .load(&release.id)
                .expect_err("schedule tampering should fail")
                .contains("schedule does not match")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn portfolio_projects_verified_release_work_into_review_queues() {
        let (root, store) = fixture("portfolio");
        let artifact = write_android_fixture(&root, "app-release.apk", "dev.trygantry.portfolio");
        let release = store
            .create(NewRelease {
                version: "8.0.0".to_string(),
                name: Some("Portfolio release".to_string()),
                owner: Some("release@example.com".to_string()),
                actor: Some("test-runner".to_string()),
            })
            .expect("release should be created");

        let draft = store.portfolio().expect("draft portfolio should load");
        assert_eq!(draft.counts.total, 1);
        assert_eq!(draft.counts.needs_attention, 1);
        assert!(draft.review_queue.iter().any(|item| {
            item.release_id == release.id && item.kind == ReleaseReviewKind::MissingCandidate
        }));

        store
            .add_candidate(NewCandidate {
                release: release.id.clone(),
                artifact_path: artifact,
                platform: Some(MobilePlatform::Android),
                actor: Some("engineer@example.com".to_string()),
            })
            .expect("candidate should bind");
        let candidate = store.portfolio().expect("candidate portfolio should load");
        assert!(
            candidate
                .review_queue
                .iter()
                .any(|item| item.kind == ReleaseReviewKind::MissingAudit)
        );

        let audit = store
            .audit(NewReleaseAudit {
                release: release.id.clone(),
                platform: Some(MobilePlatform::Android),
                actor: Some("engineer@example.com".to_string()),
            })
            .expect("candidate should audit");
        let finding = audit
            .report
            .findings
            .iter()
            .find(|finding| finding.status != crate::mobile::MobileFindingStatus::Pass)
            .expect("audit should contain review work")
            .finding_instance_id
            .clone();
        let expires_at =
            (Utc::now() + chrono::Duration::days(2)).to_rfc3339_opts(SecondsFormat::Secs, true);
        let evidence_finding = audit
            .report
            .findings
            .iter()
            .find(|finding| {
                finding.rule_id == crate::mobile::mobile_evidence_rule_id("release_owner_approval")
            })
            .expect("release-owner evidence work should exist")
            .finding_instance_id
            .clone();
        store
            .add_evidence(NewReleaseEvidence {
                release: release.id.clone(),
                platform: Some(MobilePlatform::Android),
                finding_instance_id: evidence_finding,
                mobile_evidence_kind: "release_owner_approval".to_string(),
                title: "Time-bounded release approval".to_string(),
                issuer: "release@example.com".to_string(),
                path: None,
                url: None,
                expires_at: Some(expires_at.clone()),
                actor: Some("release@example.com".to_string()),
            })
            .expect("expiring evidence should be linked");
        store
            .waive_finding(NewReleaseWaiver {
                release: release.id.clone(),
                platform: Some(MobilePlatform::Android),
                finding_instance_id: finding,
                reason: "Time-bounded contract test risk".to_string(),
                approved_by: "release@example.com".to_string(),
                expires_at,
            })
            .expect("finding should be waived");
        store
            .decide(NewHumanDecision {
                release: release.id,
                platform: Some(MobilePlatform::Android),
                decided_by: "release@example.com".to_string(),
                role: "Release manager".to_string(),
                rationale: "Blocked candidate requires correction".to_string(),
                decision: HumanDecisionKind::Rejected,
            })
            .expect("candidate should be rejected");

        let portfolio = store.portfolio().expect("review portfolio should load");
        assert_eq!(portfolio.releases[0].stage, ReleaseStage::Rejected);
        assert!(portfolio.releases[0].needs_attention);
        assert!(portfolio.releases[0].counts.blockers > 0);
        assert!(
            portfolio
                .review_queue
                .iter()
                .any(|item| item.kind == ReleaseReviewKind::Blocked)
        );
        assert!(
            portfolio
                .review_queue
                .iter()
                .any(|item| item.kind == ReleaseReviewKind::Rejected)
        );
        assert!(
            portfolio
                .review_queue
                .iter()
                .any(|item| item.kind == ReleaseReviewKind::AcceptedRisk)
        );
        assert!(
            portfolio
                .review_queue
                .iter()
                .any(|item| item.kind == ReleaseReviewKind::ExpiringWaiver)
        );
        assert_eq!(portfolio.counts.expiring_waivers, 1);
        assert_eq!(portfolio.counts.expiring_evidence, 1);
        assert!(
            portfolio
                .review_queue
                .iter()
                .any(|item| item.kind == ReleaseReviewKind::ExpiringEvidence)
        );
        assert_schema_accepts(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../schemas/release-portfolio.schema.json"),
            &serde_json::to_value(&portfolio).expect("portfolio should serialize"),
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn binds_candidate_by_artifact_hash_and_platform() {
        let (root, store) = fixture("candidate");
        let artifact = root.join("build/app-release.aab");
        fs::create_dir_all(artifact.parent().expect("artifact parent"))
            .expect("build directory should exist");
        fs::write(&artifact, b"candidate bytes").expect("artifact should be written");
        let release = store
            .create(NewRelease {
                version: "3.0.0".to_string(),
                ..NewRelease::default()
            })
            .expect("release should be created");

        let candidate = store
            .add_candidate(NewCandidate {
                release: release.id.clone(),
                artifact_path: artifact,
                platform: None,
                actor: Some("ci".to_string()),
            })
            .expect("candidate should be added");
        let view = store.load("3.0.0").expect("version lookup should work");
        assert_eq!(candidate.platform, MobilePlatform::Android);
        assert_eq!(candidate.artifact_size_bytes, 15);
        assert!(candidate.artifact_path_relative);
        assert_eq!(view.release.stage, ReleaseStage::Candidate);
        assert_eq!(view.events.len(), 2);
        assert_eq!(
            view.release.active_candidates.get(&MobilePlatform::Android),
            Some(&candidate.id)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_duplicate_release_version() {
        let (root, store) = fixture("duplicate");
        let input = NewRelease {
            version: "1.0.0".to_string(),
            ..NewRelease::default()
        };
        store
            .create(input.clone())
            .expect("first release should work");
        let error = store.create(input).expect_err("duplicate should fail");
        assert!(error.contains("already exists"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detects_modified_event_history() {
        let (root, store) = fixture("tamper");
        let release = store
            .create(NewRelease {
                version: "4.0.0".to_string(),
                ..NewRelease::default()
            })
            .expect("release should be created");
        let path = store.release_dir(&release.id).join("events.jsonl");
        let content = fs::read_to_string(&path).expect("events should be readable");
        fs::write(&path, content.replace("4.0.0", "4.0.1")).expect("events should be modified");

        let error = store.load(&release.id).expect_err("tampering should fail");
        assert!(error.contains("hash is broken"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mixed_platform_decisions_keep_rejection_until_every_candidate_is_approved() {
        let release = MobileRelease {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            id: "REL-0000000001".to_string(),
            version: "1.0.0".to_string(),
            name: None,
            owner: None,
            target_date: None,
            submission_date: None,
            rollout_date: None,
            stage: ReleaseStage::InReview,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            candidate_ids: vec!["CAN-IOS".to_string(), "CAN-ANDROID".to_string()],
            active_candidates: BTreeMap::from([
                (MobilePlatform::Ios, "CAN-IOS".to_string()),
                (MobilePlatform::Android, "CAN-ANDROID".to_string()),
            ]),
            audit_ids: vec!["AUD-IOS".to_string(), "AUD-ANDROID".to_string()],
            active_audits: BTreeMap::from([
                (MobilePlatform::Ios, "AUD-IOS".to_string()),
                (MobilePlatform::Android, "AUD-ANDROID".to_string()),
            ]),
            evidence_link_ids: Vec::new(),
            resolution_ids: Vec::new(),
            decision_ids: vec!["DEC-IOS".to_string(), "DEC-ANDROID".to_string()],
            active_decisions: BTreeMap::from([
                (MobilePlatform::Ios, "DEC-IOS".to_string()),
                (MobilePlatform::Android, "DEC-ANDROID".to_string()),
            ]),
            waiver_ids: Vec::new(),
            active_waiver_ids: Vec::new(),
            latest_event_sequence: 1,
            latest_event_hash: Some("0".repeat(64)),
        };
        let decision = |id: &str,
                        candidate_id: &str,
                        audit_id: &str,
                        platform: MobilePlatform,
                        kind: HumanDecisionKind| HumanReleaseDecision {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            id: id.to_string(),
            release_id: release.id.clone(),
            candidate_id: candidate_id.to_string(),
            candidate_sha256: "3".repeat(64),
            audit_id: audit_id.to_string(),
            platform,
            report_fingerprint: "1".repeat(64),
            policy_digest: "2".repeat(64),
            decision: kind,
            decided_by: "manager@example.com".to_string(),
            role: "release_manager".to_string(),
            rationale: "Test decision".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let ios_approval = decision(
            "DEC-IOS",
            "CAN-IOS",
            "AUD-IOS",
            MobilePlatform::Ios,
            HumanDecisionKind::Approved,
        );
        let android_rejection = decision(
            "DEC-ANDROID",
            "CAN-ANDROID",
            "AUD-ANDROID",
            MobilePlatform::Android,
            HumanDecisionKind::Rejected,
        );
        assert_eq!(
            effective_decision_stage(&release, &[ios_approval.clone(), android_rejection.clone()]),
            ReleaseStage::Rejected
        );
        assert_eq!(
            stage_after_context_change(
                &release,
                &[ios_approval.clone(), android_rejection],
                ReleaseStage::Candidate,
            ),
            ReleaseStage::Rejected
        );

        let android_approval = decision(
            "DEC-ANDROID",
            "CAN-ANDROID",
            "AUD-ANDROID",
            MobilePlatform::Android,
            HumanDecisionKind::Approved,
        );
        assert_eq!(
            effective_decision_stage(&release, &[ios_approval, android_approval]),
            ReleaseStage::Approved
        );

        let mut undecided_release = release;
        undecided_release.active_decisions.clear();
        assert_eq!(
            stage_after_context_change(&undecided_release, &[], ReleaseStage::Candidate),
            ReleaseStage::Candidate
        );
    }

    #[test]
    fn approved_at_risk_candidate_remains_in_the_review_queue() {
        let now = Utc::now();
        let release = MobileRelease {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            id: "REL-RISK000001".to_string(),
            version: "9.0.0".to_string(),
            name: Some("Risk-visible release".to_string()),
            owner: Some("release@example.com".to_string()),
            target_date: None,
            submission_date: None,
            rollout_date: None,
            stage: ReleaseStage::Approved,
            created_at: now.to_rfc3339_opts(SecondsFormat::Secs, true),
            updated_at: now.to_rfc3339_opts(SecondsFormat::Secs, true),
            candidate_ids: vec!["CAN-RISK".to_string()],
            active_candidates: BTreeMap::from([(MobilePlatform::Android, "CAN-RISK".to_string())]),
            audit_ids: vec!["AUD-RISK".to_string()],
            active_audits: BTreeMap::from([(MobilePlatform::Android, "AUD-RISK".to_string())]),
            evidence_link_ids: Vec::new(),
            resolution_ids: Vec::new(),
            decision_ids: vec!["DEC-RISK".to_string()],
            active_decisions: BTreeMap::from([(MobilePlatform::Android, "DEC-RISK".to_string())]),
            waiver_ids: Vec::new(),
            active_waiver_ids: Vec::new(),
            latest_event_sequence: 4,
            latest_event_hash: Some("0".repeat(64)),
        };
        let view = ReleaseView {
            release,
            candidates: Vec::new(),
            audits: vec![ReleaseAuditRecord {
                schema_version: RELEASE_SCHEMA_VERSION.to_string(),
                id: "AUD-RISK".to_string(),
                release_id: "REL-RISK000001".to_string(),
                candidate_id: "CAN-RISK".to_string(),
                platform: MobilePlatform::Android,
                candidate_sha256: "1".repeat(64),
                report_fingerprint: "2".repeat(64),
                policy_digest: "3".repeat(64),
                decision: ReleaseAuditDecision::AtRisk,
                counts: ReleaseAuditCounts {
                    passed: 12,
                    blockers: 0,
                    warnings: 2,
                    manual: 1,
                    unavailable: 1,
                },
                git_commit: "4".repeat(40),
                generated_at: now.to_rfc3339_opts(SecondsFormat::Secs, true),
                recorded_at: now.to_rfc3339_opts(SecondsFormat::Secs, true),
                report_path: "mobile/audits/risk.json".to_string(),
            }],
            evidence_links: Vec::new(),
            resolutions: Vec::new(),
            decisions: vec![HumanReleaseDecision {
                schema_version: RELEASE_SCHEMA_VERSION.to_string(),
                id: "DEC-RISK".to_string(),
                release_id: "REL-RISK000001".to_string(),
                candidate_id: "CAN-RISK".to_string(),
                candidate_sha256: "1".repeat(64),
                audit_id: "AUD-RISK".to_string(),
                platform: MobilePlatform::Android,
                report_fingerprint: "2".repeat(64),
                policy_digest: "3".repeat(64),
                decision: HumanDecisionKind::Approved,
                decided_by: "manager@example.com".to_string(),
                role: "release_manager".to_string(),
                rationale: "Risk is understood and time-bounded.".to_string(),
                created_at: now.to_rfc3339_opts(SecondsFormat::Secs, true),
            }],
            waivers: Vec::new(),
            events: Vec::new(),
        };

        let (portfolio_item, queue) = release_portfolio_item(&view, &[], now);
        assert!(portfolio_item.needs_attention);
        assert_eq!(
            portfolio_item.review_kinds,
            vec![ReleaseReviewKind::ApprovedWithRisk]
        );
        assert!(queue.iter().any(|item| {
            item.kind == ReleaseReviewKind::ApprovedWithRisk
                && item.next_action.contains("Resolve or explicitly waive")
        }));
    }

    #[test]
    fn persists_candidate_audit_and_invalidates_it_for_a_new_candidate() {
        let (root, store) = fixture("audit");
        let artifact = write_android_fixture(&root, "app-release.apk", "com.example.first");
        let release = store
            .create(NewRelease {
                version: "4.1.0".to_string(),
                ..NewRelease::default()
            })
            .expect("release should be created");
        let candidate = store
            .add_candidate(NewCandidate {
                release: release.id.clone(),
                artifact_path: artifact.clone(),
                platform: None,
                actor: Some("builder".to_string()),
            })
            .expect("candidate should be bound");
        let ensured_candidate = store
            .ensure_candidate(NewCandidate {
                release: release.id.clone(),
                artifact_path: artifact,
                platform: None,
                actor: Some("retry".to_string()),
            })
            .expect("the unchanged candidate should be reused");
        assert_eq!(ensured_candidate.id, candidate.id);
        assert_eq!(
            store
                .load(&release.id)
                .expect("release should reload")
                .events
                .len(),
            2
        );
        let result = store
            .audit(NewReleaseAudit {
                release: release.id.clone(),
                platform: None,
                actor: Some("release-engineer".to_string()),
            })
            .expect("candidate should be audited");
        let reused_audit = store
            .audit_reusing_unchanged(NewReleaseAudit {
                release: release.id.clone(),
                platform: None,
                actor: Some("retry".to_string()),
            })
            .expect("the unchanged audit should be reused");
        assert_eq!(reused_audit.audit.id, result.audit.id);

        assert_eq!(result.audit.candidate_id, candidate.id);
        assert_eq!(result.audit.platform, MobilePlatform::Android);
        assert_eq!(
            result
                .report
                .artifact
                .as_ref()
                .map(|artifact| &artifact.sha256),
            Some(&candidate.artifact_sha256)
        );
        let view = store
            .load(&release.id)
            .expect("audited release should load");
        assert_eq!(view.release.stage, ReleaseStage::InReview);
        assert_eq!(view.audits.len(), 1);
        assert_eq!(view.events.len(), 3);
        assert_eq!(view.events[2].kind, ReleaseEventKind::CandidateAudited);
        assert_eq!(
            view.release.active_audits.get(&MobilePlatform::Android),
            Some(&result.audit.id)
        );
        assert!(
            root.join(".gantry")
                .join(&result.audit.report_path)
                .is_file()
        );

        let schemas = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        assert_schema_accepts(
            &schemas.join("release-audit.schema.json"),
            &serde_json::to_value(&result.audit).expect("audit should serialize"),
        );
        assert_schema_accepts(
            &schemas.join("mobile-release.schema.json"),
            &serde_json::to_value(&view.release).expect("release should serialize"),
        );
        assert_schema_accepts(
            &schemas.join("release-event.schema.json"),
            &serde_json::to_value(&view.events[2]).expect("audit event should serialize"),
        );

        assert_eq!(result.audit.decision, ReleaseAuditDecision::NoGo);
        let approval_error = store
            .decide(NewHumanDecision {
                release: release.id.clone(),
                platform: Some(MobilePlatform::Android),
                decided_by: "manager@example.com".to_string(),
                role: "release_manager".to_string(),
                rationale: "Ship the candidate.".to_string(),
                decision: HumanDecisionKind::Approved,
            })
            .expect_err("a No-Go audit must fail closed");
        assert!(approval_error.contains("No-Go"));

        let evidence_finding = result
            .report
            .findings
            .iter()
            .find(|finding| {
                finding.rule_id == crate::mobile::mobile_evidence_rule_id("release_owner_approval")
            })
            .expect("release owner evidence finding should exist")
            .finding_instance_id
            .clone();
        let waived_finding = result
            .report
            .findings
            .iter()
            .find(|finding| {
                finding.finding_instance_id != evidence_finding
                    && finding.status != crate::mobile::MobileFindingStatus::Pass
            })
            .expect("a non-pass finding should exist for waiver coverage")
            .finding_instance_id
            .clone();
        let wrong_kind = store
            .add_evidence(NewReleaseEvidence {
                release: release.id.clone(),
                platform: Some(MobilePlatform::Android),
                finding_instance_id: evidence_finding.clone(),
                mobile_evidence_kind: "rollback_plan".to_string(),
                title: "Wrong evidence".to_string(),
                issuer: "captain@example.com".to_string(),
                path: None,
                url: None,
                expires_at: None,
                actor: Some("captain@example.com".to_string()),
            })
            .expect_err("a mismatched evidence kind should be rejected");
        assert!(wrong_kind.contains("not finding"));
        let evidence = store
            .add_evidence(NewReleaseEvidence {
                release: release.id.clone(),
                platform: Some(MobilePlatform::Android),
                finding_instance_id: evidence_finding.clone(),
                mobile_evidence_kind: "release_owner_approval".to_string(),
                title: "Release owner approval".to_string(),
                issuer: "captain@example.com".to_string(),
                path: None,
                url: None,
                expires_at: Some("2099-01-01T00:00:00Z".to_string()),
                actor: Some("captain@example.com".to_string()),
            })
            .expect("candidate evidence should be linked");
        let resolution = store
            .resolve_finding(NewFindingResolution {
                release: release.id.clone(),
                platform: Some(MobilePlatform::Android),
                finding_instance_id: evidence_finding.clone(),
                reason: "Release captain reviewed the production candidate.".to_string(),
                evidence_ids: vec![evidence.evidence.id.clone()],
                actor: Some("captain@example.com".to_string()),
            })
            .expect("finding resolution should be recorded");
        let evidence_view = store
            .load(&release.id)
            .expect("evidence release should load");
        assert_eq!(evidence_view.evidence_links.len(), 1);
        assert_eq!(evidence_view.resolutions.len(), 1);
        assert_eq!(evidence_view.events.len(), 5);
        assert_schema_accepts(
            &schemas.join("release-evidence-link.schema.json"),
            &serde_json::to_value(&evidence.link).expect("evidence link should serialize"),
        );
        assert_schema_accepts(
            &schemas.join("finding-resolution.schema.json"),
            &serde_json::to_value(&resolution).expect("resolution should serialize"),
        );
        assert_schema_accepts(
            &schemas.join("release-event.schema.json"),
            &serde_json::to_value(&evidence_view.events[3])
                .expect("evidence event should serialize"),
        );
        assert_schema_accepts(
            &schemas.join("release-event.schema.json"),
            &serde_json::to_value(&evidence_view.events[4])
                .expect("resolution event should serialize"),
        );

        let waiver = store
            .waive_finding(NewReleaseWaiver {
                release: release.id.clone(),
                platform: Some(MobilePlatform::Android),
                finding_instance_id: waived_finding,
                reason: "Accepted for this candidate while the vendor rotates credentials."
                    .to_string(),
                approved_by: "security@example.com".to_string(),
                expires_at: "2099-01-01T00:00:00Z".to_string(),
            })
            .expect("finding waiver should be recorded");
        let rejection = store
            .decide(NewHumanDecision {
                release: release.id.clone(),
                platform: Some(MobilePlatform::Android),
                decided_by: "manager@example.com".to_string(),
                role: "release_manager".to_string(),
                rationale: "Blocking findings remain.".to_string(),
                decision: HumanDecisionKind::Rejected,
            })
            .expect("No-Go candidate should be rejectable");
        let decision_view = store.load(&release.id).expect("decisions should verify");
        assert_eq!(decision_view.release.stage, ReleaseStage::Rejected);
        assert_eq!(decision_view.events.len(), 7);
        assert_eq!(
            decision_view.events[5].kind,
            ReleaseEventKind::FindingWaived
        );
        assert_eq!(
            decision_view.events[6].kind,
            ReleaseEventKind::ReleaseRejected
        );
        assert_schema_accepts(
            &schemas.join("release-finding-waiver.schema.json"),
            &serde_json::to_value(&waiver).expect("waiver should serialize"),
        );
        assert_schema_accepts(
            &schemas.join("human-release-decision.schema.json"),
            &serde_json::to_value(&rejection).expect("decision should serialize"),
        );
        assert_schema_accepts(
            &schemas.join("release-event.schema.json"),
            &serde_json::to_value(&decision_view.events[5]).expect("waiver event should serialize"),
        );
        assert_schema_accepts(
            &schemas.join("release-event.schema.json"),
            &serde_json::to_value(&decision_view.events[6])
                .expect("decision event should serialize"),
        );

        let decision_path = root
            .join(".gantry/releases")
            .join(&release.id)
            .join("decisions")
            .join(format!("{}.json", rejection.id));
        let original_decision = fs::read(&decision_path).expect("decision should be readable");
        let mut changed_decision: Value =
            serde_json::from_slice(&original_decision).expect("decision should parse");
        changed_decision["role"] = Value::String("unaccountable_role".to_string());
        fs::write(
            &decision_path,
            serde_json::to_vec_pretty(&changed_decision)
                .expect("changed decision should serialize"),
        )
        .expect("decision should be modified");
        assert!(
            store
                .load(&release.id)
                .expect_err("decision edits should fail verification")
                .contains("matching release event")
        );
        fs::write(&decision_path, original_decision).expect("decision should be restored");

        let release_path = root
            .join(".gantry/releases")
            .join(&release.id)
            .join("release.json");
        let original_release = fs::read(&release_path).expect("release should be readable");
        let mut changed_release: Value =
            serde_json::from_slice(&original_release).expect("release should parse");
        changed_release["stage"] = Value::String("approved".to_string());
        fs::write(
            &release_path,
            serde_json::to_vec_pretty(&changed_release).expect("changed release should serialize"),
        )
        .expect("release should be modified");
        assert!(
            store
                .load(&release.id)
                .expect_err("release stage edits should fail verification")
                .contains("inconsistent with its active decisions")
        );
        fs::write(&release_path, original_release).expect("release should be restored");

        let packet = store
            .write_packet(&release.id)
            .expect("verified packet should be written");
        let repeated_packet = store
            .write_packet(&release.id)
            .expect("unchanged packet should be reproducible");
        assert_eq!(packet.packet.packet_id, repeated_packet.packet.packet_id);
        assert_eq!(
            packet.packet.packet_digest,
            repeated_packet.packet.packet_digest
        );
        assert_eq!(
            fs::read(root.join(".gantry").join(&packet.json_path))
                .expect("packet JSON should be readable"),
            fs::read(root.join(".gantry").join(&repeated_packet.json_path))
                .expect("repeated packet JSON should be readable")
        );
        assert!(root.join(".gantry").join(&packet.markdown_path).is_file());
        assert!(root.join(".gantry").join(&packet.html_path).is_file());
        assert!(packet.handoff_summary.contains("## Handoff summary"));
        assert!(packet.handoff_summary.contains("Exact candidates: 1"));
        verify_release_packet(&packet.packet).expect("packet digest should verify");
        assert_schema_accepts(
            &schemas.join("release-packet.schema.json"),
            &serde_json::to_value(&packet.packet).expect("packet should serialize"),
        );
        let mut changed_packet = packet.packet.clone();
        changed_packet.release.version = "tampered".to_string();
        assert!(
            verify_release_packet(&changed_packet)
                .expect_err("modified packet should fail verification")
                .contains("digest does not verify")
        );

        let refreshed = store
            .audit(NewReleaseAudit {
                release: release.id.clone(),
                platform: Some(MobilePlatform::Android),
                actor: Some("release-engineer".to_string()),
            })
            .expect("candidate should be re-audited with evidence");
        assert!(refreshed.report.findings.iter().any(|finding| {
            finding.finding_instance_id == evidence_finding
                && finding.status == crate::mobile::MobileFindingStatus::Pass
        }));
        let refreshed_view = store
            .load(&release.id)
            .expect("refreshed release should load");
        assert_eq!(refreshed_view.release.stage, ReleaseStage::InReview);
        assert!(refreshed_view.release.active_decisions.is_empty());
        assert!(refreshed_view.release.active_waiver_ids.is_empty());
        assert_eq!(
            refreshed_view.events[7].details["invalidated_decision_id"],
            rejection.id
        );
        assert_eq!(
            refreshed_view.events[7].details["invalidated_waiver_ids"][0],
            waiver.id
        );
        let refreshed_packet = store
            .build_packet(&release.id)
            .expect("refreshed packet should build");
        assert_ne!(refreshed_packet.packet_id, packet.packet.packet_id);
        assert_eq!(refreshed_packet.release.stage, ReleaseStage::InReview);

        let evidence_path = root
            .join(".gantry/evidence")
            .join(format!("{}.json", evidence.evidence.id));
        let original_evidence = fs::read(&evidence_path).expect("evidence should be readable");
        let mut modified_evidence: Value =
            serde_json::from_slice(&original_evidence).expect("evidence should parse");
        modified_evidence["title"] = Value::String("Modified approval".to_string());
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&modified_evidence)
                .expect("modified evidence should serialize"),
        )
        .expect("evidence should be modified");
        let error = store
            .load(&release.id)
            .expect_err("modified evidence should fail verification");
        assert!(error.contains("record is inconsistent"));
        fs::write(&evidence_path, original_evidence).expect("evidence should be restored");

        let replacement =
            write_android_fixture(&root, "app-release-2.apk", "com.example.replacement");
        let replacement_candidate = store
            .add_candidate(NewCandidate {
                release: release.id.clone(),
                artifact_path: replacement,
                platform: None,
                actor: Some("builder".to_string()),
            })
            .expect("replacement candidate should be bound");
        let stale_view = store
            .load(&release.id)
            .expect("release with stale audit should load");
        assert_eq!(stale_view.release.stage, ReleaseStage::Candidate);
        assert!(stale_view.release.active_audits.is_empty());
        assert_eq!(stale_view.audits.len(), 2);
        assert_eq!(stale_view.evidence_links.len(), 1);
        assert_eq!(stale_view.resolutions.len(), 1);
        assert_eq!(stale_view.decisions.len(), 1);
        assert_eq!(stale_view.waivers.len(), 1);
        assert_eq!(stale_view.events.len(), 9);
        assert_eq!(
            stale_view.events[8].details["invalidated_audit_id"],
            refreshed.audit.id
        );
        assert_eq!(
            stale_view.events[8].details["invalidated_evidence_link_ids"][0],
            evidence.link.id
        );
        assert_eq!(
            stale_view.events[8].details["invalidated_resolution_ids"][0],
            resolution.id
        );

        let replacement_audit = store
            .audit(NewReleaseAudit {
                release: release.id.clone(),
                platform: Some(MobilePlatform::Android),
                actor: Some("release-engineer".to_string()),
            })
            .expect("replacement candidate should be audited");
        let diff = store
            .diff_candidates(NewReleaseDiff {
                release: release.id.clone(),
                platform: Some(MobilePlatform::Android),
                from_candidate_id: None,
                to_candidate_id: None,
            })
            .expect("candidate diff should be generated");
        assert_eq!(diff.from_candidate.id, candidate.id);
        assert_eq!(diff.to_candidate.id, replacement_candidate.id);
        assert_eq!(
            diff.to_audit.as_ref().map(|audit| &audit.id),
            Some(&replacement_audit.audit.id)
        );
        assert!(!diff.related_events.is_empty());
        assert_schema_accepts(
            &schemas.join("release-candidate-diff.schema.json"),
            &serde_json::to_value(&diff).expect("candidate diff should serialize"),
        );

        let report_path = root.join(".gantry").join(&result.audit.report_path);
        let mut report: Value = serde_json::from_slice(
            &fs::read(&report_path).expect("stored report should be readable"),
        )
        .expect("stored report should parse");
        report["decision"] = Value::String("go".to_string());
        fs::write(
            &report_path,
            serde_json::to_vec_pretty(&report).expect("modified report should serialize"),
        )
        .expect("stored report should be modified");
        let error = store
            .load(&release.id)
            .expect_err("modified report should fail verification");
        assert!(error.contains("fingerprint does not verify"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn published_schemas_accept_release_candidate_and_events() {
        let (root, store) = fixture("schemas");
        let artifact = root.join("App.ipa");
        fs::write(&artifact, b"ios candidate").expect("artifact should be written");
        let release = store
            .create(NewRelease {
                version: "5.0.0".to_string(),
                ..NewRelease::default()
            })
            .expect("release should be created");
        store
            .add_candidate(NewCandidate {
                release: release.id.clone(),
                artifact_path: artifact,
                platform: None,
                actor: None,
            })
            .expect("candidate should be added");
        let view = store.load(&release.id).expect("release should load");
        let schemas = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas");

        assert_schema_accepts(
            &schemas.join("mobile-release.schema.json"),
            &serde_json::to_value(&view.release).expect("release should serialize"),
        );
        assert_schema_accepts(
            &schemas.join("release-candidate.schema.json"),
            &serde_json::to_value(&view.candidates[0]).expect("candidate should serialize"),
        );
        for event in &view.events {
            assert_schema_accepts(
                &schemas.join("release-event.schema.json"),
                &serde_json::to_value(event).expect("event should serialize"),
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_transaction_is_detected_and_rolled_forward() {
        let (root, store) = fixture("transaction_recovery");
        let release = store
            .create(NewRelease {
                version: "6.0.0".to_string(),
                ..NewRelease::default()
            })
            .expect("release should be created");
        let mut view = store.load(&release.id).expect("release should load");
        view.release.target_date = Some("2026-08-01".to_string());
        view.release.updated_at = timestamp();
        let event = build_event(
            &view.release,
            ReleaseEventKind::ReleaseScheduled,
            Some("release-manager".to_string()),
            json!({
                "target_date": view.release.target_date,
                "submission_date": Value::Null,
                "rollout_date": Value::Null,
            }),
        )
        .expect("event should build");
        apply_event_head(&mut view.release, &event);
        view.events.push(event);

        let transaction_id = "TXN-INTERRUPTED";
        let directory = store.transaction_root().join(transaction_id);
        fs::create_dir_all(&directory).expect("transaction directory should be created");
        let event_data = events_bytes(&view.events).expect("events should serialize");
        let release_data = json_bytes(&view.release).expect("release should serialize");
        atomic_write(&directory.join("000.data"), &event_data).expect("events should be staged");
        atomic_write(&directory.join("001.data"), &release_data).expect("release should be staged");
        let manifest = ReleaseTransactionManifest {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            transaction_id: transaction_id.to_string(),
            release_id: release.id.clone(),
            created_at: timestamp(),
            writes: vec![
                ReleaseTransactionWrite {
                    target: format!("releases/{}/events.jsonl", release.id),
                    staged_file: "000.data".to_string(),
                    sha256: hex::encode(Sha256::digest(&event_data)),
                },
                ReleaseTransactionWrite {
                    target: format!("releases/{}/release.json", release.id),
                    staged_file: "001.data".to_string(),
                    sha256: hex::encode(Sha256::digest(&release_data)),
                },
            ],
        };
        atomic_write_json(&directory.join("manifest.json"), &manifest)
            .expect("manifest should be staged");

        let before = store
            .check_vault(false)
            .expect("health check should complete");
        assert!(!before.healthy);
        assert_eq!(before.pending_transactions, 1);

        let recovered = store
            .load(&release.id)
            .expect("load should roll the transaction forward");
        assert_eq!(recovered.release.target_date.as_deref(), Some("2026-08-01"));
        assert_eq!(store.pending_transaction_count().unwrap(), 0);
        assert!(store.check_vault(false).unwrap().healthy);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_event_head_cannot_overwrite_a_newer_release() {
        let (root, store) = fixture("optimistic_concurrency");
        let release = store
            .create(NewRelease {
                version: "6.1.0".to_string(),
                ..NewRelease::default()
            })
            .expect("release should be created");
        let stale_head = release.latest_event_hash.clone();
        store
            .schedule(NewReleaseSchedule {
                release: release.id.clone(),
                target_date: Some("2026-08-02".to_string()),
                submission_date: None,
                rollout_date: None,
                actor: Some("release-manager".to_string()),
            })
            .expect("schedule should commit");
        let error = store
            .commit_release_transaction(&release.id, stale_head.as_deref(), None, Vec::new())
            .expect_err("stale writer should be refused");
        assert!(error.contains("changed while this operation was running"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn vault_migration_is_a_verified_no_op_for_current_records() {
        let (root, store) = fixture("migration_noop");
        store
            .create(NewRelease {
                version: "6.2.0".to_string(),
                ..NewRelease::default()
            })
            .expect("release should be created");
        let report = store
            .migrate_vault(true)
            .expect("current vault should produce a migration plan");
        assert!(report.dry_run);
        assert_eq!(report.migrations_required, 0);
        assert_eq!(report.migrated_releases, 0);
        assert!(report.backup_path.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn transaction_targets_cannot_escape_the_vault() {
        let (root, store) = fixture("transaction_traversal");
        let directory = store.transaction_root().join("TXN-TRAVERSAL");
        fs::create_dir_all(&directory).expect("transaction directory should be created");
        let staged = b"must not escape";
        atomic_write(&directory.join("000.data"), staged).expect("staged data should write");
        let manifest = ReleaseTransactionManifest {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            transaction_id: "TXN-TRAVERSAL".to_string(),
            release_id: "REL-TRAVERSAL".to_string(),
            created_at: timestamp(),
            writes: vec![ReleaseTransactionWrite {
                target: "../escaped.json".to_string(),
                staged_file: "000.data".to_string(),
                sha256: hex::encode(Sha256::digest(staged)),
            }],
        };
        atomic_write_json(&directory.join("manifest.json"), &manifest)
            .expect("manifest should write");

        let error = store
            .apply_transaction_directory(&directory)
            .expect_err("path traversal must fail closed");
        assert!(error.contains("Unsafe transaction path"));
        assert!(!root.join("escaped.json").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn incomplete_transaction_does_not_publish_partial_state() {
        let (root, store) = fixture("transaction_incomplete");
        let release = store
            .create(NewRelease {
                version: "6.3.0".to_string(),
                ..NewRelease::default()
            })
            .expect("release should be created");
        let release_path = store.release_dir(&release.id).join("release.json");
        let original = fs::read(&release_path).expect("original release record should be readable");
        let directory = store.transaction_root().join("TXN-INCOMPLETE");
        fs::create_dir_all(&directory).expect("transaction directory should be created");
        let replacement = b"{}";
        let manifest = ReleaseTransactionManifest {
            schema_version: RELEASE_SCHEMA_VERSION.to_string(),
            transaction_id: "TXN-INCOMPLETE".to_string(),
            release_id: release.id.clone(),
            created_at: timestamp(),
            writes: vec![ReleaseTransactionWrite {
                target: format!("releases/{}/release.json", release.id),
                staged_file: "missing.data".to_string(),
                sha256: hex::encode(Sha256::digest(replacement)),
            }],
        };
        atomic_write_json(&directory.join("manifest.json"), &manifest)
            .expect("manifest should write");

        let error = store
            .apply_transaction_directory(&directory)
            .expect_err("missing staged data must fail closed");
        assert!(
            error.contains("Failed to read staged data for transaction"),
            "unexpected error: {error}"
        );
        assert_eq!(
            fs::read(&release_path).unwrap(),
            original,
            "an incomplete transaction must not alter the release head"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn read_only_transaction_directory_returns_an_actionable_error() {
        use std::os::unix::fs::PermissionsExt;

        let (root, store) = fixture("read_only_transaction");
        let release = store
            .create(NewRelease {
                version: "6.4.0".to_string(),
                ..NewRelease::default()
            })
            .expect("release should be created");
        let transaction_root = store.transaction_root();
        fs::set_permissions(&transaction_root, fs::Permissions::from_mode(0o500)).unwrap();

        let probe = transaction_root.join("permission-probe");
        if fs::create_dir(&probe).is_ok() {
            fs::remove_dir(&probe).unwrap();
            fs::set_permissions(&transaction_root, fs::Permissions::from_mode(0o700)).unwrap();
            let _ = fs::remove_dir_all(root);
            return;
        }

        let result = store.schedule(NewReleaseSchedule {
            release: release.id.clone(),
            target_date: Some("2026-08-03".to_string()),
            submission_date: None,
            rollout_date: None,
            actor: Some("release-manager".to_string()),
        });
        fs::set_permissions(&transaction_root, fs::Permissions::from_mode(0o700)).unwrap();
        let error = result.expect_err("read-only transaction storage must refuse mutation");
        assert!(
            error.contains("transaction") || error.contains("Permission denied"),
            "unexpected error: {error}"
        );
        assert_eq!(
            store.load(&release.id).unwrap().release.target_date,
            None,
            "failed mutation must preserve the original release"
        );
        let _ = fs::remove_dir_all(root);
    }

    fn assert_schema_accepts(path: &Path, instance: &Value) {
        let schema: Value = serde_json::from_slice(
            &fs::read(path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display())),
        )
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
        let validator = jsonschema::validator_for(&schema)
            .unwrap_or_else(|error| panic!("failed to compile {}: {error}", path.display()));
        let errors: Vec<_> = validator
            .iter_errors(instance)
            .map(|error| error.to_string())
            .collect();
        assert!(
            errors.is_empty(),
            "{} rejected instance: {errors:?}",
            path.display()
        );
    }
}
