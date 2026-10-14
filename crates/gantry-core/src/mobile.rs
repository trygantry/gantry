//! Deterministic mobile release governance.
//!
//! The scanner is deliberately local-first: it reads project metadata and build
//! artifacts without invoking platform toolchains or uploading proprietary code.

use chrono::{Duration, Utc};
use jsonschema::validator_for;
use plist::Value as PlistValue;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use zip::ZipArchive;

mod rules;
pub use rules::{MOBILE_RULE_DEFINITIONS, MobileRuleDefinition};

pub const MOBILE_REPORT_SCHEMA_VERSION: &str = "2.0";
pub const MOBILE_POLICY_SCHEMA_VERSION: &str = "1.0";
pub const MOBILE_POLICY_PACK_VERSION: &str = "2026.07";
pub const MOBILE_POLICY_REVIEWED_AT: &str = "2026-07-13";
pub const MOBILE_AUDIT_CACHE_SCHEMA_VERSION: &str = "1.0";
pub const MOBILE_POLICY_SCHEMA: &str = include_str!("../schemas/mobile-policy.schema.json");
pub const MOBILE_REPORT_SCHEMA: &str = include_str!("../schemas/mobile-audit-report.schema.json");

#[derive(Clone, Default)]
pub struct MobileAuditCancellation {
    cancelled: Arc<AtomicBool>,
}

impl MobileAuditCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("Mobile audit cancelled.".to_string())
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MobileStack {
    Flutter,
    ReactNative,
    NativeIos,
    NativeAndroid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MobilePlatform {
    Ios,
    Android,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MobileSeverity {
    Blocker,
    Warning,
    Advisory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MobileFindingStatus {
    Pass,
    Fail,
    Warning,
    Manual,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MobileDecision {
    Go,
    AtRisk,
    NoGo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MobileArtifactKind {
    Ipa,
    Aab,
    Apk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MobilePolicy {
    pub schema_version: String,
    pub policy_pack_version: String,
    pub reviewed_at: String,
    pub minimum_android_target_sdk: u32,
    pub minimum_wear_target_sdk: u32,
    pub fail_on_warnings: bool,
    pub fail_on_manual: bool,
    pub fail_on_unavailable: bool,
    pub approved_signing_identities: Vec<String>,
    pub trusted_evidence_issuers: Vec<String>,
    pub production_placeholder_patterns: Vec<String>,
    pub required_manual_evidence: Vec<String>,
    pub required_public_urls: Vec<String>,
}

impl Default for MobilePolicy {
    fn default() -> Self {
        Self {
            schema_version: MOBILE_POLICY_SCHEMA_VERSION.to_string(),
            policy_pack_version: MOBILE_POLICY_PACK_VERSION.to_string(),
            reviewed_at: MOBILE_POLICY_REVIEWED_AT.to_string(),
            minimum_android_target_sdk: 35,
            minimum_wear_target_sdk: 34,
            fail_on_warnings: false,
            fail_on_manual: false,
            fail_on_unavailable: true,
            approved_signing_identities: Vec::new(),
            trusted_evidence_issuers: Vec::new(),
            production_placeholder_patterns: vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
                "example.com".to_string(),
                "changeme".to_string(),
            ],
            required_manual_evidence: vec![
                "store_listing_review".to_string(),
                "production_smoke_test".to_string(),
                "release_owner_approval".to_string(),
                "rollback_plan".to_string(),
            ],
            required_public_urls: vec!["privacy_policy".to_string(), "support".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IosTarget {
    pub name: String,
    pub info_plist: String,
    pub bundle_identifier: Option<String>,
    pub version: Option<String>,
    pub build: Option<String>,
    pub minimum_os_version: Option<String>,
    pub privacy_manifest: Option<String>,
    pub entitlements: Option<String>,
    pub usage_descriptions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AndroidApp {
    pub name: String,
    pub gradle_file: String,
    pub manifest: Option<String>,
    pub application_id: Option<String>,
    pub namespace: Option<String>,
    pub target_sdk: Option<u32>,
    pub min_sdk: Option<u32>,
    pub version_code: Option<String>,
    pub version_name: Option<String>,
    pub permissions: Vec<String>,
    pub release_signing_configured: bool,
    pub debuggable: Option<bool>,
    pub cleartext_traffic: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileDependencyInventory {
    pub ecosystem: String,
    pub path: String,
    pub package_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MobileProject {
    pub root: String,
    pub stacks: Vec<MobileStack>,
    pub platforms: Vec<MobilePlatform>,
    pub ios_targets: Vec<IosTarget>,
    pub android_apps: Vec<AndroidApp>,
    pub dependency_inventory: Vec<MobileDependencyInventory>,
    pub inspected_files: usize,
    pub scan_limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MobileSignatureStatus {
    Verified,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileSignatureVerification {
    pub status: MobileSignatureStatus,
    pub verifier: Option<String>,
    pub details: String,
    pub certificate_fingerprints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileArtifact {
    pub path: String,
    pub kind: MobileArtifactKind,
    pub platform: MobilePlatform,
    pub sha256: String,
    pub size_bytes: u64,
    pub application_id: Option<String>,
    pub version: Option<String>,
    pub build: Option<String>,
    pub minimum_os_version: Option<String>,
    pub target_sdk: Option<u32>,
    pub permissions: Vec<String>,
    pub signature_material: String,
    pub signature_verification: MobileSignatureVerification,
    pub privacy_manifest_present: Option<bool>,
    pub privacy_manifests: Vec<String>,
    pub embedded_components: Vec<String>,
    pub package_entries: usize,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileFinding {
    /// Stable policy rule identifier retained for 1.x consumers.
    pub id: String,
    pub rule_id: String,
    pub subject_id: String,
    pub finding_instance_id: String,
    pub title: String,
    pub category: String,
    pub severity: MobileSeverity,
    pub status: MobileFindingStatus,
    pub observed: Option<String>,
    pub expected: Option<String>,
    pub locations: Vec<String>,
    pub rationale: String,
    pub remediation: String,
    pub authoritative_source: Option<String>,
    pub source_reviewed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MobileAuditSummary {
    pub passed: usize,
    pub blockers: usize,
    pub warnings: usize,
    pub manual: usize,
    pub unavailable: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileAuditReport {
    pub schema_version: String,
    pub generated_at: String,
    pub project_root: String,
    pub git_commit: String,
    pub git_branch: Option<String>,
    pub git_dirty: Option<bool>,
    pub tool_version: String,
    pub policy_digest: String,
    pub report_fingerprint: String,
    pub policy: MobilePolicy,
    pub decision: MobileDecision,
    pub decision_reasons: Vec<String>,
    pub summary: MobileAuditSummary,
    pub project: MobileProject,
    pub artifact: Option<MobileArtifact>,
    pub findings: Vec<MobileFinding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MobileAuditCacheResult {
    pub schema_version: String,
    pub cache_hit: bool,
    pub cache_key: Option<String>,
    pub cache_path: Option<String>,
    pub report: MobileAuditReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct MobileAuditCacheDescriptor {
    pub schema_version: String,
    pub eligible: bool,
    pub cache_key: Option<String>,
    pub cache_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MobileAuditCacheIdentity {
    schema_version: String,
    report_schema_version: String,
    tool_version: String,
    git_commit: String,
    artifact_sha256: String,
    artifact_size_bytes: u64,
    artifact_extension: String,
    policy_digest: String,
    authority_tools_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MobileAuditCacheEntry {
    schema_version: String,
    cache_key: String,
    identity: MobileAuditCacheIdentity,
    report: MobileAuditReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobileReportFormat {
    Json,
    Markdown,
    Html,
}

impl MobileReportFormat {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "md" | "markdown" => Ok(Self::Markdown),
            "html" => Ok(Self::Html),
            other => Err(format!("Unsupported mobile report format: {other}")),
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Markdown => "md",
            Self::Html => "html",
        }
    }
}

pub fn mobile_policy_path(project_root: &Path) -> PathBuf {
    project_root.join(".gantry").join("mobile-policy.json")
}

pub fn load_mobile_policy(project_root: &Path) -> Result<MobilePolicy, String> {
    let path = mobile_policy_path(project_root);
    if !path.exists() {
        return Ok(MobilePolicy::default());
    }
    let bytes = fs::read(&path)
        .map_err(|e| format!("Failed to read mobile policy {}: {e}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Invalid mobile policy {}: {e}", path.display()))?;
    let schema: serde_json::Value = serde_json::from_str(MOBILE_POLICY_SCHEMA)
        .map_err(|e| format!("Bundled mobile policy schema is invalid: {e}"))?;
    let validator = validator_for(&schema)
        .map_err(|e| format!("Bundled mobile policy schema could not compile: {e}"))?;
    let errors: Vec<String> = validator
        .iter_errors(&value)
        .take(5)
        .map(|error| error.to_string())
        .collect();
    if !errors.is_empty() {
        return Err(format!(
            "Invalid mobile policy {}: {}",
            path.display(),
            errors.join("; ")
        ));
    }
    let policy: MobilePolicy = serde_json::from_value(value)
        .map_err(|e| format!("Invalid mobile policy {}: {e}", path.display()))?;
    validate_mobile_policy(&policy)
        .map_err(|e| format!("Invalid mobile policy {}: {e}", path.display()))?;
    Ok(policy)
}

pub fn validate_mobile_policy(policy: &MobilePolicy) -> Result<(), String> {
    if policy.schema_version != MOBILE_POLICY_SCHEMA_VERSION {
        return Err(format!(
            "unsupported schema_version {}; expected {}",
            policy.schema_version, MOBILE_POLICY_SCHEMA_VERSION
        ));
    }
    if policy.policy_pack_version.trim().is_empty() || policy.reviewed_at.trim().is_empty() {
        return Err("policy_pack_version and reviewed_at must be non-empty".to_string());
    }
    chrono::NaiveDate::parse_from_str(&policy.reviewed_at, "%Y-%m-%d")
        .map_err(|_| "reviewed_at must use YYYY-MM-DD".to_string())?;
    for (name, value) in [
        (
            "minimum_android_target_sdk",
            policy.minimum_android_target_sdk,
        ),
        ("minimum_wear_target_sdk", policy.minimum_wear_target_sdk),
    ] {
        if !(21..=100).contains(&value) {
            return Err(format!("{name} must be between 21 and 100"));
        }
    }
    if policy.production_placeholder_patterns.len() > 128 {
        return Err("production_placeholder_patterns may contain at most 128 entries".to_string());
    }
    if policy
        .production_placeholder_patterns
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > 256)
    {
        return Err("placeholder patterns must be non-empty and at most 256 bytes".to_string());
    }
    let mut kinds = BTreeSet::new();
    for kind in &policy.required_manual_evidence {
        let normalized = normalize_evidence_name(kind);
        if normalized.is_empty() || !kinds.insert(normalized) {
            return Err("required_manual_evidence values must be non-empty and unique".to_string());
        }
    }
    let mut urls = BTreeSet::new();
    for kind in &policy.required_public_urls {
        let normalized = normalize_evidence_name(kind);
        if normalized.is_empty() || !urls.insert(normalized) {
            return Err("required_public_urls values must be non-empty and unique".to_string());
        }
    }
    let mut identities = BTreeSet::new();
    for identity in &policy.approved_signing_identities {
        let normalized = identity.trim().to_ascii_lowercase();
        if normalized.is_empty() || !identities.insert(normalized) {
            return Err(
                "approved_signing_identities values must be non-empty and unique".to_string(),
            );
        }
    }
    let mut issuers = BTreeSet::new();
    for issuer in &policy.trusted_evidence_issuers {
        let normalized = issuer.trim().to_ascii_lowercase();
        if normalized.is_empty() || !issuers.insert(normalized) {
            return Err("trusted_evidence_issuers values must be non-empty and unique".to_string());
        }
    }
    Ok(())
}

pub fn init_mobile_policy(project_root: &Path, force: bool) -> Result<PathBuf, String> {
    let path = mobile_policy_path(project_root);
    if path.exists() && !force {
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    let schema_path = project_root
        .join(".gantry")
        .join("schema")
        .join("mobile-policy.schema.json");
    if let Some(parent) = schema_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    fs::write(&schema_path, MOBILE_POLICY_SCHEMA)
        .map_err(|e| format!("Failed to write {}: {e}", schema_path.display()))?;
    let policy = MobilePolicy::default();
    validate_mobile_policy(&policy)?;
    let body = serde_json::to_string_pretty(&policy)
        .map_err(|e| format!("Failed to serialize mobile policy: {e}"))?;
    fs::write(&path, format!("{body}\n"))
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(path)
}

pub fn detect_mobile_project(project_root: &Path) -> Result<MobileProject, String> {
    let (files, scan_limitations) = collect_project_files(project_root)?;
    let mut stacks = BTreeSet::new();
    let mut platforms = BTreeSet::new();

    if let Some(pubspec) = files
        .iter()
        .find(|p| relative(project_root, p) == "pubspec.yaml")
    {
        if read_text(pubspec).is_some_and(|s| s.contains("flutter:")) {
            stacks.insert(MobileStack::Flutter);
        }
    }
    if let Some(package) = files
        .iter()
        .find(|p| relative(project_root, p) == "package.json")
    {
        if read_text(package).is_some_and(|s| s.contains("react-native")) {
            stacks.insert(MobileStack::ReactNative);
        }
    }
    if files.iter().any(|p| {
        let rel = relative(project_root, p);
        rel.ends_with("project.pbxproj") || rel.ends_with("Info.plist")
    }) {
        platforms.insert(MobilePlatform::Ios);
        if !stacks.contains(&MobileStack::Flutter) && !stacks.contains(&MobileStack::ReactNative) {
            stacks.insert(MobileStack::NativeIos);
        }
    }
    if files.iter().any(|p| {
        let rel = relative(project_root, p);
        rel.ends_with("build.gradle")
            || rel.ends_with("build.gradle.kts")
            || rel.ends_with("AndroidManifest.xml")
    }) {
        platforms.insert(MobilePlatform::Android);
        if !stacks.contains(&MobileStack::Flutter) && !stacks.contains(&MobileStack::ReactNative) {
            stacks.insert(MobileStack::NativeAndroid);
        }
    }

    let ios_targets = inspect_ios_sources(project_root, &files);
    let android_apps = inspect_android_sources(project_root, &files);
    let dependency_inventory = inspect_dependency_inventory(project_root, &files);
    Ok(MobileProject {
        root: ".".to_string(),
        stacks: stacks.into_iter().collect(),
        platforms: platforms.into_iter().collect(),
        ios_targets,
        android_apps,
        dependency_inventory,
        inspected_files: files.len(),
        scan_limitations,
    })
}

pub fn run_mobile_audit(
    project_root: &Path,
    artifact_path: Option<&Path>,
) -> Result<MobileAuditReport, String> {
    run_mobile_audit_with_cancellation(
        project_root,
        artifact_path,
        &MobileAuditCancellation::default(),
    )
}

pub fn run_mobile_audit_with_cancellation(
    project_root: &Path,
    artifact_path: Option<&Path>,
    cancellation: &MobileAuditCancellation,
) -> Result<MobileAuditReport, String> {
    cancellation.check()?;
    let policy = load_mobile_policy(project_root)?;
    cancellation.check()?;
    let project = detect_mobile_project(project_root)?;
    cancellation.check()?;
    let mut artifact = artifact_path
        .map(|path| inspect_mobile_artifact_with_cancellation(path, cancellation))
        .transpose()?;
    cancellation.check()?;
    if let Some(value) = artifact.as_mut() {
        value.path = report_path(project_root, Path::new(&value.path));
    }
    let git_commit = current_git_commit(project_root).unwrap_or_else(|| "unknown".to_string());
    let mut findings = evaluate_source_rules(project_root, &project, &policy);
    cancellation.check()?;
    if let Some(ref artifact) = artifact {
        findings.extend(evaluate_artifact_rules(artifact, &policy));
    } else {
        findings.push(manual_finding(
            "MOB-ART-001",
            "Release artifact inspection",
            "artifact",
            "No IPA, AAB, or APK was supplied. Run the audit against the exact candidate artifact before release.",
            "Build the release candidate and pass it with --artifact.",
        ));
    }
    let evidence = load_mobile_evidence(project_root);
    cancellation.check()?;
    findings.extend(evidence.rejections.iter().map(|rejection| {
        finding(
            "MOB-EVD-INVALID",
            "Invalid mobile evidence record",
            "evidence",
            MobileSeverity::Warning,
            MobileFindingStatus::Warning,
            Some(rejection.reason.clone()),
            Some(
                "A parseable, current, explicitly typed, release-bound evidence record".to_string(),
            ),
            vec![rejection.path.clone()],
            "Rejected evidence is never allowed to satisfy a release requirement.",
            "Correct or remove the rejected record, then rerun the audit.",
            None,
        )
    }));
    findings.extend(policy.required_manual_evidence.iter().map(|kind| {
        let matching = matching_mobile_evidence(
            &evidence.records,
            kind,
            &git_commit,
            artifact.as_ref().map(|a| a.sha256.as_str()),
            &policy.trusted_evidence_issuers,
        );
        if matching.is_empty() {
            manual_finding(
                &mobile_evidence_rule_id(kind),
                &format!("Required evidence: {}", kind.replace('_', " ")),
                "evidence",
                "This evidence requires a human or connected provider to attest it for the candidate release.",
                &format!("Record {kind} evidence in the Gantry vault and bind it to the release commit or artifact."),
            )
        } else {
            finding(
                &mobile_evidence_rule_id(kind),
                &format!("Required evidence: {}", kind.replace('_', " ")),
                "evidence",
                MobileSeverity::Warning,
                MobileFindingStatus::Pass,
                Some(matching.iter().map(|e| e.id.clone()).collect::<Vec<_>>().join(", ")),
                Some("Current release-bound evidence".to_string()),
                matching.iter().filter_map(|e| e.path.clone()).collect(),
                "Release evidence is current and bound to this candidate commit or artifact.",
                "None.",
                None,
            )
        }
    }));
    findings.sort_by(|a, b| a.finding_instance_id.cmp(&b.finding_instance_id));
    cancellation.check()?;
    let summary = summarize_findings(&findings);
    let mut decision_reasons = Vec::new();
    if summary.blockers > 0 {
        decision_reasons.push(format!("{} blocker finding(s)", summary.blockers));
    }
    if policy.fail_on_warnings && summary.warnings > 0 {
        decision_reasons.push(format!("{} warning(s) blocked by policy", summary.warnings));
    }
    if policy.fail_on_manual && summary.manual > 0 {
        decision_reasons.push(format!(
            "{} manual item(s) blocked by policy",
            summary.manual
        ));
    }
    if policy.fail_on_unavailable && summary.unavailable > 0 {
        decision_reasons.push(format!(
            "{} unavailable check(s) blocked by policy",
            summary.unavailable
        ));
    }
    let decision = if !decision_reasons.is_empty() {
        MobileDecision::NoGo
    } else if summary.warnings > 0 || summary.manual > 0 || summary.unavailable > 0 {
        MobileDecision::AtRisk
    } else {
        MobileDecision::Go
    };
    let policy_digest = sha256_json(&policy)?;
    let mut report = MobileAuditReport {
        schema_version: MOBILE_REPORT_SCHEMA_VERSION.to_string(),
        generated_at: Utc::now().to_rfc3339(),
        project_root: ".".to_string(),
        git_commit,
        git_branch: current_git_branch(project_root),
        git_dirty: current_git_dirty(project_root),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        policy_digest,
        report_fingerprint: String::new(),
        policy,
        decision,
        decision_reasons,
        summary,
        project,
        artifact,
        findings,
    };
    report.report_fingerprint = calculate_mobile_report_fingerprint(&report)?;
    Ok(report)
}

pub fn run_mobile_audit_cached(
    project_root: &Path,
    artifact_path: &Path,
) -> Result<MobileAuditCacheResult, String> {
    let identity = mobile_audit_cache_identity(project_root, artifact_path)?;
    let Some(identity) = identity else {
        return run_mobile_audit(project_root, Some(artifact_path)).map(|report| {
            MobileAuditCacheResult {
                schema_version: MOBILE_AUDIT_CACHE_SCHEMA_VERSION.to_string(),
                cache_hit: false,
                cache_key: None,
                cache_path: None,
                report,
            }
        });
    };
    let cache_key = sha256_json(&identity)?;
    let relative_path = PathBuf::from(".gantry")
        .join("cache")
        .join("mobile")
        .join(format!("{cache_key}.json"));
    let cache_path = project_root.join(&relative_path);

    if let Ok(body) = fs::read_to_string(&cache_path)
        && let Ok(entry) = serde_json::from_str::<MobileAuditCacheEntry>(&body)
        && verify_mobile_audit_cache_entry(&entry, &cache_key, &identity).is_ok()
    {
        return Ok(MobileAuditCacheResult {
            schema_version: MOBILE_AUDIT_CACHE_SCHEMA_VERSION.to_string(),
            cache_hit: true,
            cache_key: Some(cache_key),
            cache_path: Some(relative(
                &project_root
                    .canonicalize()
                    .unwrap_or_else(|_| project_root.to_path_buf()),
                &cache_path,
            )),
            report: entry.report,
        });
    }

    let report = run_mobile_audit(project_root, Some(artifact_path))?;
    let entry = MobileAuditCacheEntry {
        schema_version: MOBILE_AUDIT_CACHE_SCHEMA_VERSION.to_string(),
        cache_key: cache_key.clone(),
        identity: identity.clone(),
        report: report.clone(),
    };
    verify_mobile_audit_cache_entry(&entry, &cache_key, &identity)?;
    write_mobile_cache_entry(&cache_path, &entry)?;
    Ok(MobileAuditCacheResult {
        schema_version: MOBILE_AUDIT_CACHE_SCHEMA_VERSION.to_string(),
        cache_hit: false,
        cache_key: Some(cache_key),
        cache_path: Some(relative(
            &project_root
                .canonicalize()
                .unwrap_or_else(|_| project_root.to_path_buf()),
            &cache_path,
        )),
        report,
    })
}

pub fn describe_mobile_audit_cache(
    project_root: &Path,
    artifact_path: &Path,
) -> Result<MobileAuditCacheDescriptor, String> {
    let Some(identity) = mobile_audit_cache_identity(project_root, artifact_path)? else {
        return Ok(MobileAuditCacheDescriptor {
            schema_version: MOBILE_AUDIT_CACHE_SCHEMA_VERSION.to_string(),
            eligible: false,
            cache_key: None,
            cache_path: None,
        });
    };
    let cache_key = sha256_json(&identity)?;
    Ok(MobileAuditCacheDescriptor {
        schema_version: MOBILE_AUDIT_CACHE_SCHEMA_VERSION.to_string(),
        eligible: true,
        cache_path: Some(format!(".gantry/cache/mobile/{cache_key}.json")),
        cache_key: Some(cache_key),
    })
}

fn mobile_audit_cache_identity(
    project_root: &Path,
    artifact_path: &Path,
) -> Result<Option<MobileAuditCacheIdentity>, String> {
    let Some(git_commit) = current_git_commit(project_root) else {
        return Ok(None);
    };
    if current_git_dirty_for_cache(project_root) != Some(false) {
        return Ok(None);
    }
    let artifact_path = artifact_path.canonicalize().map_err(|error| {
        format!(
            "Failed to resolve artifact {} for caching: {error}",
            artifact_path.display()
        )
    })?;
    let artifact_sha256 = sha256_file(&artifact_path)?;
    let artifact_size_bytes = fs::metadata(&artifact_path)
        .map_err(|error| format!("Failed to inspect {}: {error}", artifact_path.display()))?
        .len();
    let artifact_extension = artifact_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let policy_digest = sha256_json(&load_mobile_policy(project_root)?)?;
    Ok(Some(MobileAuditCacheIdentity {
        schema_version: MOBILE_AUDIT_CACHE_SCHEMA_VERSION.to_string(),
        report_schema_version: MOBILE_REPORT_SCHEMA_VERSION.to_string(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        git_commit,
        artifact_sha256,
        artifact_size_bytes,
        artifact_extension,
        policy_digest,
        authority_tools_digest: authority_tools_digest()?,
    }))
}

fn current_git_dirty_for_cache(project_root: &Path) -> Option<bool> {
    let repository_prefix = git_stdout(project_root, &["rev-parse", "--show-prefix"])
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string();
    let output = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all", "--", "."])
        .current_dir(project_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let status = String::from_utf8_lossy(&output.stdout);
    Some(status.lines().any(|line| {
        let repository_path = line.get(3..).unwrap_or_default().trim_matches('"');
        let path = repository_path
            .strip_prefix(&repository_prefix)
            .and_then(|value| value.strip_prefix('/'))
            .unwrap_or(repository_path);
        !path.starts_with(".gantry/releases/")
            && !path.starts_with(".gantry/cache/")
            && !path.starts_with(".gantry/mobile/audits/")
            && path != ".gantry/index.json"
            && path != ".gantry/telemetry.json"
    }))
}

fn authority_tools_digest() -> Result<String, String> {
    let tools = [
        "codesign",
        "apksigner",
        "jarsigner",
        "bundletool",
        "apkanalyzer",
    ];
    let mut identities = BTreeMap::new();
    for tool in tools {
        identities.insert(tool, command_file_identity(tool));
    }
    sha256_json(&identities)
}

fn command_file_identity(name: &str) -> String {
    let Some(paths) = std::env::var_os("PATH") else {
        return "missing".to_string();
    };
    for directory in std::env::split_paths(&paths) {
        let path = directory.join(name);
        if let Ok(metadata) = fs::metadata(&path)
            && metadata.is_file()
        {
            let modified = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|value| value.as_secs())
                .unwrap_or_default();
            return format!("{}:{}:{modified}", path.display(), metadata.len());
        }
    }
    "missing".to_string()
}

fn verify_mobile_audit_cache_entry(
    entry: &MobileAuditCacheEntry,
    cache_key: &str,
    identity: &MobileAuditCacheIdentity,
) -> Result<(), String> {
    if entry.schema_version != MOBILE_AUDIT_CACHE_SCHEMA_VERSION
        || entry.cache_key != cache_key
        || &entry.identity != identity
        || entry.report.schema_version != MOBILE_REPORT_SCHEMA_VERSION
        || entry.report.tool_version != identity.tool_version
        || entry.report.git_commit != identity.git_commit
        || entry.report.policy_digest != identity.policy_digest
        || entry.report.artifact.as_ref().is_none_or(|artifact| {
            artifact.sha256 != identity.artifact_sha256
                || artifact.size_bytes != identity.artifact_size_bytes
        })
        || calculate_mobile_report_fingerprint(&entry.report)? != entry.report.report_fingerprint
    {
        return Err("Mobile audit cache entry failed identity verification.".to_string());
    }
    Ok(())
}

fn write_mobile_cache_entry(path: &Path, entry: &MobileAuditCacheEntry) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Cache path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create cache directory: {error}"))?;
    let temp = parent.join(format!(
        ".cache-{}-{}.tmp",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let body = serde_json::to_vec_pretty(entry)
        .map_err(|error| format!("Failed to serialize mobile audit cache: {error}"))?;
    fs::write(&temp, body)
        .map_err(|error| format!("Failed to write {}: {error}", temp.display()))?;
    if let Err(error) = fs::rename(&temp, path) {
        if path.exists() {
            fs::remove_file(path)
                .map_err(|remove| format!("Failed to replace cache after {error}: {remove}"))?;
            fs::rename(&temp, path)
                .map_err(|retry| format!("Failed to publish cache entry: {retry}"))?;
        } else {
            return Err(format!("Failed to publish cache entry: {error}"));
        }
    }
    Ok(())
}

pub fn calculate_mobile_report_fingerprint(report: &MobileAuditReport) -> Result<String, String> {
    // Generation time is provenance, not decision identity. Excluding it makes repeated
    // audits of the same source, candidate, policy, and evidence comparable.
    let mut identity = report.clone();
    identity.generated_at.clear();
    identity.report_fingerprint.clear();
    sha256_json(&identity)
}

fn collect_project_files(project_root: &Path) -> Result<(Vec<PathBuf>, Vec<String>), String> {
    fn walk(
        root: &Path,
        canonical_root: &Path,
        dir: &Path,
        depth: usize,
        out: &mut Vec<PathBuf>,
        limitations: &mut Vec<String>,
    ) -> Result<(), String> {
        if depth > 14 {
            limitations.push(format!(
                "Maximum scan depth reached at {}",
                relative(root, dir)
            ));
            return Ok(());
        }
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => {
                limitations.push(format!("Could not read {}: {error}", relative(root, dir)));
                return Ok(());
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    limitations.push(format!("Could not read a directory entry: {error}"));
                    continue;
                }
            };
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    limitations.push(format!(
                        "Could not inspect {}: {error}",
                        relative(root, &path)
                    ));
                    continue;
                }
            };
            if metadata.file_type().is_symlink() {
                limitations.push(format!("Skipped symbolic link {}", relative(root, &path)));
                continue;
            }
            if metadata.is_dir() {
                if matches!(
                    name.as_ref(),
                    ".git"
                        | ".gantry"
                        | ".claude"
                        | ".symlinks"
                        | "target"
                        | "build"
                        | ".dart_tool"
                        | "node_modules"
                        | ".gradle"
                        | "Pods"
                        | "DerivedData"
                        | ".idea"
                        | ".venv"
                        | "venv"
                        | "env"
                        | "tmp_env"
                ) || name.starts_with("DerivedData")
                    || name.starts_with("build_")
                    || name.ends_with(".xcarchive")
                {
                    continue;
                }
                let canonical = match path.canonicalize() {
                    Ok(value) => value,
                    Err(error) => {
                        limitations.push(format!(
                            "Could not resolve {}: {error}",
                            relative(root, &path)
                        ));
                        continue;
                    }
                };
                if !canonical.starts_with(canonical_root) {
                    limitations.push(format!(
                        "Skipped directory outside project root: {}",
                        relative(root, &path)
                    ));
                    continue;
                }
                walk(root, canonical_root, &path, depth + 1, out, limitations)?;
            } else if metadata.is_file() {
                let rel = relative(root, &path);
                if is_mobile_source_file(&rel) {
                    if metadata.len() > 2_000_000 && is_dependency_inventory_file(&rel) {
                        out.push(path);
                    } else if metadata.len() > 2_000_000 {
                        limitations.push(format!("Skipped source file over 2 MiB: {rel}"));
                    } else if requires_utf8_source(&rel) && fs::read_to_string(&path).is_err() {
                        limitations.push(format!("Skipped non-UTF-8 source file: {rel}"));
                    } else {
                        out.push(path);
                    }
                }
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    let mut limitations = Vec::new();
    let canonical_root = project_root.canonicalize().map_err(|e| {
        format!(
            "Failed to resolve project root {}: {e}",
            project_root.display()
        )
    })?;
    walk(
        project_root,
        &canonical_root,
        project_root,
        0,
        &mut files,
        &mut limitations,
    )?;
    files.sort();
    limitations.sort();
    limitations.dedup();
    Ok((files, limitations))
}

fn requires_utf8_source(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    lower.ends_with(".dart")
        || lower.ends_with(".gradle")
        || lower.ends_with(".gradle.kts")
        || lower.ends_with(".yaml")
        || lower.ends_with(".json")
        || lower.ends_with(".xcconfig")
        || lower.ends_with(".properties")
        || lower.ends_with(".lock")
        || lower.ends_with(".toml")
}

fn is_dependency_inventory_file(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    lower.ends_with("podfile.lock")
        || lower.ends_with("package.resolved")
        || lower.ends_with("pubspec.lock")
        || lower.ends_with("package-lock.json")
        || lower.ends_with("libs.versions.toml")
}

fn report_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .filter(|value| !value.as_os_str().is_empty())
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
}

fn is_mobile_source_file(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    lower.ends_with("pubspec.yaml")
        || lower.ends_with("package.json")
        || lower.ends_with("project.pbxproj")
        || lower.ends_with("info.plist")
        || lower.ends_with("privacyinfo.xcprivacy")
        || lower.ends_with(".entitlements")
        || lower.ends_with("androidmanifest.xml")
        || lower.ends_with("build.gradle")
        || lower.ends_with("build.gradle.kts")
        || lower.ends_with("gradle.properties")
        || lower.ends_with(".dart")
        || lower.ends_with(".xcconfig")
        || lower.ends_with(".properties")
        || lower.ends_with("podfile.lock")
        || lower.ends_with("package.resolved")
        || lower.ends_with("pubspec.lock")
        || lower.ends_with("package-lock.json")
        || lower.ends_with("libs.versions.toml")
}

fn inspect_dependency_inventory(root: &Path, files: &[PathBuf]) -> Vec<MobileDependencyInventory> {
    let mut inventory = Vec::new();
    for path in files {
        let rel = relative(root, path);
        let lower = rel.to_ascii_lowercase();
        let ecosystem = if lower.ends_with("podfile.lock") {
            Some("cocoapods")
        } else if lower.ends_with("package.resolved") {
            Some("swift_package_manager")
        } else if lower.ends_with("pubspec.lock") {
            Some("dart_pub")
        } else if lower.ends_with("package-lock.json") {
            Some("npm")
        } else if lower.ends_with("libs.versions.toml") {
            Some("gradle_version_catalog")
        } else {
            None
        };
        let Some(ecosystem) = ecosystem else {
            continue;
        };
        let package_count = read_text(path).and_then(|content| match ecosystem {
            "swift_package_manager" => serde_json::from_str::<serde_json::Value>(&content)
                .ok()
                .and_then(|value| {
                    value
                        .get("pins")
                        .and_then(|pins| pins.as_array())
                        .map(Vec::len)
                }),
            "npm" => serde_json::from_str::<serde_json::Value>(&content)
                .ok()
                .and_then(|value| {
                    value
                        .get("packages")
                        .and_then(|packages| packages.as_object())
                        .map(|packages| packages.len().saturating_sub(1))
                }),
            "cocoapods" => Some(
                content
                    .lines()
                    .filter(|line| line.starts_with("  - "))
                    .count(),
            ),
            "dart_pub" => Some(
                content
                    .lines()
                    .filter(|line| line.starts_with("  ") && line.trim_end().ends_with(':'))
                    .count(),
            ),
            "gradle_version_catalog" => Some(
                content
                    .lines()
                    .filter(|line| !line.trim_start().starts_with('#') && line.contains('='))
                    .count(),
            ),
            _ => None,
        });
        inventory.push(MobileDependencyInventory {
            ecosystem: ecosystem.to_string(),
            path: rel,
            package_count,
        });
    }
    inventory.sort_by(|a, b| a.path.cmp(&b.path));
    inventory
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn read_text(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > 2_000_000 {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn plist_string(dict: &plist::Dictionary, key: &str) -> Option<String> {
    dict.get(key).and_then(|v| match v {
        PlistValue::String(s) => Some(s.clone()),
        PlistValue::Integer(i) => i
            .as_signed()
            .map(|v| v.to_string())
            .or_else(|| i.as_unsigned().map(|v| v.to_string())),
        _ => None,
    })
}

fn inspect_ios_sources(project_root: &Path, files: &[PathBuf]) -> Vec<IosTarget> {
    let privacy_manifests: Vec<&PathBuf> = files
        .iter()
        .filter(|p| relative(project_root, p).ends_with("PrivacyInfo.xcprivacy"))
        .collect();
    let entitlements: Vec<&PathBuf> = files
        .iter()
        .filter(|p| relative(project_root, p).ends_with(".entitlements"))
        .collect();
    let mut targets = Vec::new();
    for path in files
        .iter()
        .filter(|p| is_ios_target_plist(&relative(project_root, p)))
    {
        let rel = relative(project_root, path);
        let Ok(value) = PlistValue::from_file(path) else {
            continue;
        };
        let Some(dict) = value.as_dictionary() else {
            continue;
        };
        let parent = path.parent().unwrap_or(project_root);
        let privacy_manifest = privacy_manifests
            .iter()
            .filter(|candidate| candidate.parent() == Some(parent) || candidate.starts_with(parent))
            .map(|candidate| relative(project_root, candidate))
            .min();
        let entitlement = entitlements
            .iter()
            .filter(|candidate| candidate.parent() == Some(parent) || candidate.starts_with(parent))
            .map(|candidate| relative(project_root, candidate))
            .min();
        let mut usage_descriptions: Vec<String> = dict
            .keys()
            .filter(|key| key.starts_with("NS") && key.ends_with("UsageDescription"))
            .cloned()
            .collect();
        usage_descriptions.sort();
        let name = plist_string(dict, "CFBundleDisplayName")
            .or_else(|| plist_string(dict, "CFBundleName"))
            .unwrap_or_else(|| {
                path.parent()
                    .and_then(Path::file_name)
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "iOS target".to_string())
            });
        targets.push(IosTarget {
            name,
            info_plist: rel,
            bundle_identifier: plist_string(dict, "CFBundleIdentifier"),
            version: plist_string(dict, "CFBundleShortVersionString"),
            build: plist_string(dict, "CFBundleVersion"),
            minimum_os_version: plist_string(dict, "MinimumOSVersion"),
            privacy_manifest,
            entitlements: entitlement,
            usage_descriptions,
        });
    }
    targets.sort_by(|a, b| a.info_plist.cmp(&b.info_plist));
    targets
}

fn is_ios_target_plist(rel: &str) -> bool {
    if !rel.starts_with("ios/") || !rel.ends_with("Info.plist") {
        return false;
    }
    !rel.contains("/Flutter/")
        && !rel.contains("/Pods/")
        && !rel.contains("/Build/")
        && !rel.contains(".framework/")
        && !rel.contains(".bundle/")
        && !rel.contains("Tests/")
        && !rel.ends_with("GoogleService-Info.plist")
}

fn inspect_android_sources(project_root: &Path, files: &[PathBuf]) -> Vec<AndroidApp> {
    let manifests: Vec<&PathBuf> = files
        .iter()
        .filter(|p| relative(project_root, p).ends_with("AndroidManifest.xml"))
        .collect();
    let mut apps = Vec::new();
    for gradle in files.iter().filter(|p| {
        let rel = relative(project_root, p);
        rel.starts_with("android/")
            && (rel.ends_with("build.gradle") || rel.ends_with("build.gradle.kts"))
            && read_text(p).is_some_and(|s| {
                s.contains("com.android.application") || s.contains("com.android.dynamic-feature")
            })
    }) {
        let content = read_text(gradle).unwrap_or_default();
        let module_dir = gradle.parent().unwrap_or(project_root);
        let manifest = manifests
            .iter()
            .filter(|path| path.starts_with(module_dir))
            .min_by_key(|path| {
                let rel = relative(project_root, path);
                (
                    if rel.contains("/src/main/") { 0 } else { 1 },
                    path.components().count(),
                )
            });
        let parsed_manifest = manifest.and_then(|path| parse_android_manifest_source(path));
        let target_sdk = extract_gradle_u32(&content, &["targetSdk", "targetSdkVersion"]);
        let min_sdk = extract_gradle_u32(&content, &["minSdk", "minSdkVersion"]);
        let release_section = named_brace_block(&content, "release").unwrap_or_default();
        apps.push(AndroidApp {
            name: module_dir
                .file_name()
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_else(|| "android-app".to_string()),
            gradle_file: relative(project_root, gradle),
            manifest: manifest.map(|p| relative(project_root, p)),
            application_id: extract_gradle_string(&content, "applicationId")
                .or_else(|| parsed_manifest.as_ref().and_then(|m| m.package.clone())),
            namespace: extract_gradle_string(&content, "namespace"),
            target_sdk,
            min_sdk,
            version_code: extract_gradle_value(&content, "versionCode"),
            version_name: extract_gradle_value(&content, "versionName"),
            permissions: parsed_manifest
                .as_ref()
                .map(|m| m.permissions.clone())
                .unwrap_or_default(),
            release_signing_configured: release_section.contains("signingConfig")
                && !release_section.contains("signingConfigs.debug"),
            debuggable: parsed_manifest.as_ref().and_then(|m| m.debuggable),
            cleartext_traffic: parsed_manifest.as_ref().and_then(|m| m.cleartext_traffic),
        });
    }
    apps.sort_by(|a, b| a.gradle_file.cmp(&b.gradle_file));
    apps
}

#[derive(Default)]
struct ParsedAndroidManifest {
    package: Option<String>,
    permissions: Vec<String>,
    debuggable: Option<bool>,
    cleartext_traffic: Option<bool>,
}

fn parse_android_manifest_source(path: &Path) -> Option<ParsedAndroidManifest> {
    let text = read_text(path)?;
    let doc = roxmltree::Document::parse(&text).ok()?;
    let root = doc.root_element();
    let mut permissions: Vec<String> = root
        .children()
        .filter(|n| n.is_element() && n.tag_name().name().starts_with("uses-permission"))
        .filter_map(|n| android_attr(n, "name").map(str::to_string))
        .collect();
    permissions.sort();
    permissions.dedup();
    let application = root
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "application");
    Some(ParsedAndroidManifest {
        package: root.attribute("package").map(str::to_string),
        permissions,
        debuggable: application
            .and_then(|n| android_attr(n, "debuggable"))
            .and_then(parse_bool),
        cleartext_traffic: application
            .and_then(|n| android_attr(n, "usesCleartextTraffic"))
            .and_then(parse_bool),
    })
}

fn android_attr<'a, 'input>(node: roxmltree::Node<'a, 'input>, name: &str) -> Option<&'a str> {
    node.attribute(("http://schemas.android.com/apk/res/android", name))
        .or_else(|| node.attribute(name))
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn extract_gradle_string(content: &str, key: &str) -> Option<String> {
    extract_gradle_value(content, key).map(|s| s.trim_matches(['\'', '"']).to_string())
}

fn extract_gradle_u32(content: &str, keys: &[&str]) -> Option<u32> {
    keys.iter()
        .find_map(|key| extract_gradle_value(content, key).and_then(|s| first_u32(&s)))
}

fn extract_gradle_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.split("//").next().unwrap_or(line).trim();
        let Some(pos) = line.find(key) else {
            continue;
        };
        if pos > 0
            && line[..pos]
                .chars()
                .last()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            continue;
        }
        let mut rest = line[pos + key.len()..].trim_start();
        rest = rest.strip_prefix('=').unwrap_or(rest).trim_start();
        if rest.is_empty() || rest.starts_with('{') {
            continue;
        }
        let value = if rest.starts_with(['\'', '"']) {
            let quote = rest.chars().next()?;
            rest[1..]
                .split(quote)
                .next()
                .unwrap_or_default()
                .to_string()
        } else {
            rest.split(|c: char| c.is_whitespace() || c == ',' || c == ')')
                .next()
                .unwrap_or_default()
                .to_string()
        };
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

fn first_u32(value: &str) -> Option<u32> {
    let digits: String = value
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn named_brace_block<'a>(content: &'a str, marker: &str) -> Option<&'a str> {
    let mut search = 0usize;
    while let Some(offset) = content[search..].find(marker) {
        let start = search + offset;
        let before_ok = start == 0
            || content[..start]
                .chars()
                .next_back()
                .is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_');
        let after = start + marker.len();
        let after_ok = content[after..]
            .chars()
            .next()
            .is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_');
        if before_ok && after_ok {
            let open = after + content[after..].find('{')?;
            let mut depth = 0usize;
            for (relative, character) in content[open..].char_indices() {
                match character {
                    '{' => depth += 1,
                    '}' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            return Some(&content[start..open + relative + 1]);
                        }
                    }
                    _ => {}
                }
            }
            return None;
        }
        search = after;
    }
    None
}

pub fn inspect_mobile_artifact(path: &Path) -> Result<MobileArtifact, String> {
    inspect_mobile_artifact_with_cancellation(path, &MobileAuditCancellation::default())
}

fn inspect_mobile_artifact_with_cancellation(
    path: &Path,
    cancellation: &MobileAuditCancellation,
) -> Result<MobileArtifact, String> {
    cancellation.check()?;
    if !path.is_file() {
        return Err(format!(
            "Mobile artifact does not exist: {}",
            path.display()
        ));
    }
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let kind = match extension.as_str() {
        "ipa" => MobileArtifactKind::Ipa,
        "aab" => MobileArtifactKind::Aab,
        "apk" => MobileArtifactKind::Apk,
        _ => return Err("Artifact must be an .ipa, .aab, or .apk file".to_string()),
    };
    let size_bytes = fs::metadata(path)
        .map_err(|e| format!("Failed to stat mobile artifact {}: {e}", path.display()))?
        .len();
    let sha256 = sha256_file_with_cancellation(path, cancellation)?;
    cancellation.check()?;
    let apk_signing_block = kind == MobileArtifactKind::Apk && has_apk_signing_block(path)?;
    let file = File::open(path)
        .map_err(|e| format!("Failed to open mobile artifact {}: {e}", path.display()))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| format!("Invalid mobile artifact archive {}: {e}", path.display()))?;
    validate_archive_limits_with_cancellation(&mut archive, cancellation)?;
    cancellation.check()?;
    let package_entries = archive.len();
    match kind {
        MobileArtifactKind::Ipa => {
            inspect_ipa(path, &mut archive, sha256, size_bytes, package_entries)
        }
        MobileArtifactKind::Aab | MobileArtifactKind::Apk => inspect_android_artifact(
            path,
            kind,
            &mut archive,
            apk_signing_block,
            sha256,
            size_bytes,
            package_entries,
        ),
    }
}

fn validate_archive_limits(archive: &mut ZipArchive<File>) -> Result<(), String> {
    validate_archive_limits_with_cancellation(archive, &MobileAuditCancellation::default())
}

fn validate_archive_limits_with_cancellation(
    archive: &mut ZipArchive<File>,
    cancellation: &MobileAuditCancellation,
) -> Result<(), String> {
    const MAX_ENTRIES: usize = 100_000;
    const MAX_ENTRY_SIZE: u64 = 512 * 1024 * 1024;
    const MAX_TOTAL_SIZE: u64 = 4 * 1024 * 1024 * 1024;
    if archive.len() > MAX_ENTRIES {
        return Err(format!(
            "Artifact contains more than {MAX_ENTRIES} ZIP entries"
        ));
    }
    let mut total = 0u64;
    for index in 0..archive.len() {
        if index % 256 == 0 {
            cancellation.check()?;
        }
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("Failed to inspect artifact entry: {e}"))?;
        if entry.size() > MAX_ENTRY_SIZE {
            return Err(format!(
                "Artifact entry {} exceeds the 512 MiB safety limit",
                entry.name()
            ));
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| "Artifact uncompressed size overflow".to_string())?;
        if total > MAX_TOTAL_SIZE {
            return Err("Artifact exceeds the 4 GiB total uncompressed safety limit".to_string());
        }
    }
    Ok(())
}

fn read_limited(reader: &mut impl Read, limit: u64, label: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read {label}: {e}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!(
            "{label} exceeds the {} MiB safety limit",
            limit / 1024 / 1024
        ));
    }
    Ok(bytes)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    sha256_file_with_cancellation(path, &MobileAuditCancellation::default())
}

fn sha256_file_with_cancellation(
    path: &Path,
    cancellation: &MobileAuditCancellation,
) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|e| format!("Failed to open mobile artifact {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        cancellation.check()?;
        let read = file
            .read(&mut buffer)
            .map_err(|e| format!("Failed to hash mobile artifact {}: {e}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn has_apk_signing_block(path: &Path) -> Result<bool, String> {
    const EOCD_SEARCH: u64 = 65_557;
    let mut file = File::open(path).map_err(|e| {
        format!(
            "Failed to inspect APK signing block {}: {e}",
            path.display()
        )
    })?;
    let size = file
        .metadata()
        .map_err(|e| format!("Failed to stat APK {}: {e}", path.display()))?
        .len();
    let tail = size.min(EOCD_SEARCH);
    file.seek(SeekFrom::End(-(tail as i64)))
        .map_err(|e| format!("Failed to seek APK {}: {e}", path.display()))?;
    let mut bytes = Vec::with_capacity(tail as usize);
    file.read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read APK signing tail {}: {e}", path.display()))?;
    let Some(eocd) = bytes.windows(4).rposition(|window| window == b"PK\x05\x06") else {
        return Ok(false);
    };
    if eocd + 20 > bytes.len() {
        return Ok(false);
    }
    let central_offset_bytes: [u8; 4] = bytes[eocd + 16..eocd + 20]
        .try_into()
        .map_err(|_| format!("Malformed APK central directory in {}", path.display()))?;
    let central_offset = u32::from_le_bytes(central_offset_bytes) as u64;
    if central_offset < 32 {
        return Ok(false);
    }
    file.seek(SeekFrom::Start(central_offset - 24))
        .map_err(|e| format!("Failed to seek APK signing footer {}: {e}", path.display()))?;
    let mut footer = [0u8; 24];
    file.read_exact(&mut footer)
        .map_err(|e| format!("Failed to read APK signing footer {}: {e}", path.display()))?;
    if &footer[8..] != b"APK Sig Block 42" {
        return Ok(false);
    }
    let block_size_bytes: [u8; 8] = footer[..8]
        .try_into()
        .map_err(|_| format!("Malformed APK signing footer in {}", path.display()))?;
    let block_size = u64::from_le_bytes(block_size_bytes);
    if block_size < 24 || block_size + 8 > central_offset {
        return Ok(false);
    }
    file.seek(SeekFrom::Start(central_offset - block_size - 8))
        .map_err(|e| format!("Failed to seek APK signing header {}: {e}", path.display()))?;
    let mut header = [0u8; 8];
    file.read_exact(&mut header)
        .map_err(|e| format!("Failed to read APK signing header {}: {e}", path.display()))?;
    Ok(u64::from_le_bytes(header) == block_size)
}

fn inspect_ipa(
    path: &Path,
    archive: &mut ZipArchive<File>,
    sha256: String,
    size_bytes: u64,
    package_entries: usize,
) -> Result<MobileArtifact, String> {
    let mut info_bytes = None;
    let mut privacy_manifest_present = false;
    let mut privacy_manifests = BTreeSet::new();
    let mut embedded_components = BTreeSet::new();
    let mut code_signature = false;
    let mut provisioning_profile = false;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("Failed to read IPA entry: {e}"))?;
        let name = entry.name().to_string();
        if is_root_app_info_plist(&name) {
            if entry.size() > 8 * 1024 * 1024 {
                return Err("IPA root Info.plist exceeds the 8 MiB safety limit".to_string());
            }
            info_bytes = Some(read_limited(&mut entry, 8 * 1024 * 1024, "IPA Info.plist")?);
        }
        if is_root_app_entry(&name, "PrivacyInfo.xcprivacy") {
            privacy_manifest_present = true;
        }
        if name.ends_with("PrivacyInfo.xcprivacy") {
            privacy_manifests.insert(name.clone());
        }
        if name.ends_with(".appex/Info.plist") || name.ends_with(".framework/Info.plist") {
            embedded_components.insert(name.trim_end_matches("/Info.plist").to_string());
        }
        if name.contains(".app/_CodeSignature/CodeResources") {
            code_signature = true;
        }
        if name.ends_with(".app/embedded.mobileprovision") {
            provisioning_profile = true;
        }
    }
    let bytes = info_bytes
        .ok_or_else(|| "IPA does not contain a root Payload/<name>.app/Info.plist".to_string())?;
    let value = PlistValue::from_reader(Cursor::new(bytes))
        .map_err(|e| format!("Failed to parse IPA Info.plist: {e}"))?;
    let dict = value
        .as_dictionary()
        .ok_or_else(|| "IPA Info.plist is not a dictionary".to_string())?;
    let signature_material = match (code_signature, provisioning_profile) {
        (true, true) => "code signature resources and provisioning profile present",
        (true, false) => "code signature resources present; provisioning profile absent",
        (false, true) => "provisioning profile present; code signature resources absent",
        (false, false) => "no code signature resources or provisioning profile detected",
    }
    .to_string();
    let signature_verification = verify_ipa_signature(path, code_signature);
    Ok(MobileArtifact {
        path: path.to_string_lossy().into_owned(),
        kind: MobileArtifactKind::Ipa,
        platform: MobilePlatform::Ios,
        sha256,
        size_bytes,
        application_id: plist_string(dict, "CFBundleIdentifier"),
        version: plist_string(dict, "CFBundleShortVersionString"),
        build: plist_string(dict, "CFBundleVersion"),
        minimum_os_version: plist_string(dict, "MinimumOSVersion"),
        target_sdk: None,
        permissions: Vec::new(),
        signature_material,
        signature_verification,
        privacy_manifest_present: Some(privacy_manifest_present),
        privacy_manifests: privacy_manifests.into_iter().collect(),
        embedded_components: embedded_components.into_iter().collect(),
        package_entries,
        limitations: vec![
            "App Store acceptance and provisioning-profile policy remain external checks even when codesign verification succeeds.".to_string(),
            "Nested extension entitlement/profile agreement requires release evidence from the archive/export workflow.".to_string(),
        ],
    })
}

fn is_root_app_info_plist(name: &str) -> bool {
    is_root_app_entry(name, "Info.plist")
}

fn is_root_app_entry(name: &str, suffix: &str) -> bool {
    if !name.starts_with("Payload/") || !name.ends_with(suffix) {
        return false;
    }
    let remainder = &name["Payload/".len()..];
    remainder.matches('/').count() == 1 && remainder.contains(".app/")
}

fn verify_ipa_signature(path: &Path, material_present: bool) -> MobileSignatureVerification {
    if !material_present {
        return signature_result(
            MobileSignatureStatus::Failed,
            None,
            "No root application code-signature resources were found.",
            Vec::new(),
        );
    }
    if !command_exists("codesign") {
        return signature_result(
            MobileSignatureStatus::Unavailable,
            None,
            "codesign is not available on this host; signature material was observed but not cryptographically verified.",
            Vec::new(),
        );
    }
    let temp = match extract_root_ipa_app(path) {
        Ok(value) => value,
        Err(error) => {
            return signature_result(
                MobileSignatureStatus::Failed,
                Some("codesign"),
                &format!("The IPA could not be safely prepared for verification: {error}"),
                Vec::new(),
            );
        }
    };
    let output = Command::new("codesign")
        .current_dir(&temp.root)
        .args(["--verify", "--deep", "--strict", "--verbose=2"])
        .arg(&temp.app_path)
        .output();
    match output {
        Ok(output) => {
            let mut details = command_details(&output)
                .replace(temp.root.to_string_lossy().as_ref(), "<temporary-ipa>");
            if output.status.success() {
                if let Ok(identity) = Command::new("codesign")
                    .current_dir(&temp.root)
                    .args(["--display", "--verbose=4"])
                    .arg(&temp.app_path)
                    .output()
                {
                    details.push('\n');
                    details.push_str(
                        &command_details(&identity)
                            .replace(temp.root.to_string_lossy().as_ref(), "<temporary-ipa>"),
                    );
                }
            }
            signature_result(
                if output.status.success() {
                    MobileSignatureStatus::Verified
                } else {
                    MobileSignatureStatus::Failed
                },
                Some("codesign"),
                &details,
                Vec::new(),
            )
        }
        Err(error) => signature_result(
            MobileSignatureStatus::Unavailable,
            Some("codesign"),
            &format!("Unable to execute codesign: {error}"),
            Vec::new(),
        ),
    }
}

fn verify_android_signature(
    path: &Path,
    kind: &MobileArtifactKind,
    jar_signature: bool,
    apk_signing_block: bool,
) -> MobileSignatureVerification {
    let material_present = jar_signature || apk_signing_block;
    if !material_present {
        return signature_result(
            MobileSignatureStatus::Failed,
            None,
            "No supported Android signature material was found.",
            Vec::new(),
        );
    }
    let verifier = match kind {
        MobileArtifactKind::Apk if command_exists("apksigner") => "apksigner",
        MobileArtifactKind::Aab if command_exists("jarsigner") => "jarsigner",
        MobileArtifactKind::Apk if jar_signature && command_exists("jarsigner") => "jarsigner",
        _ => {
            return signature_result(
                MobileSignatureStatus::Unavailable,
                None,
                "apksigner/jarsigner is not available; signature material was observed but not cryptographically verified.",
                Vec::new(),
            );
        }
    };
    let mut command = Command::new(verifier);
    command.current_dir(path.parent().unwrap_or_else(|| Path::new(".")));
    if verifier == "apksigner" {
        command
            .args(["verify", "--verbose", "--print-certs"])
            .arg(path);
    } else {
        command.args(["-verify", "-certs"]).arg(path);
    }
    match command.output() {
        Ok(output) => {
            let details = command_details(&output).replace(
                path.to_string_lossy().as_ref(),
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .as_ref(),
            );
            let fingerprints = extract_certificate_fingerprints(&details);
            let cryptographically_verified = if verifier == "jarsigner" {
                let normalized = details.to_ascii_lowercase();
                output.status.success()
                    && normalized.contains("jar verified")
                    && !normalized.contains("jar is unsigned")
            } else {
                output.status.success()
            };
            signature_result(
                if cryptographically_verified {
                    MobileSignatureStatus::Verified
                } else {
                    MobileSignatureStatus::Failed
                },
                Some(verifier),
                &details,
                fingerprints,
            )
        }
        Err(error) => signature_result(
            MobileSignatureStatus::Unavailable,
            Some(verifier),
            &format!("Unable to execute {verifier}: {error}"),
            Vec::new(),
        ),
    }
}

fn signature_result(
    status: MobileSignatureStatus,
    verifier: Option<&str>,
    details: &str,
    certificate_fingerprints: Vec<String>,
) -> MobileSignatureVerification {
    MobileSignatureVerification {
        status,
        verifier: verifier.map(str::to_string),
        details: details.chars().take(4096).collect(),
        certificate_fingerprints,
    }
}

fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|directory| {
            let candidate = directory.join(name);
            candidate.is_file()
        })
    })
}

fn command_details(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!(
        "{}{}{}",
        stdout.trim(),
        if stdout.trim().is_empty() || stderr.trim().is_empty() {
            ""
        } else {
            "\n"
        },
        stderr.trim()
    );
    if combined.is_empty() {
        format!("Verifier exited with {}", output.status)
    } else {
        combined.chars().take(4096).collect()
    }
}

fn extract_certificate_fingerprints(details: &str) -> Vec<String> {
    let mut values = BTreeSet::new();
    for line in details.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("sha-256") || lower.contains("sha256") {
            if let Some((_, value)) = line.split_once(':') {
                let normalized: String = value.chars().filter(|c| c.is_ascii_hexdigit()).collect();
                if normalized.len() == 64 {
                    values.insert(normalized.to_ascii_uppercase());
                }
            }
        }
    }
    values.into_iter().collect()
}

struct ExtractedIpaApp {
    root: PathBuf,
    app_path: PathBuf,
}

impl Drop for ExtractedIpaApp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn extract_root_ipa_app(path: &Path) -> Result<ExtractedIpaApp, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open IPA: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid IPA archive: {e}"))?;
    validate_archive_limits(&mut archive)?;
    let root_info = (0..archive.len())
        .find_map(|index| {
            archive.by_index(index).ok().and_then(|entry| {
                let name = entry.name().to_string();
                is_root_app_info_plist(&name).then_some(name)
            })
        })
        .ok_or_else(|| "IPA root application was not found".to_string())?;
    let prefix = root_info.trim_end_matches("Info.plist").to_string();
    let unique = format!(
        "gantry-ipa-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    let root = std::env::temp_dir().join(unique);
    fs::create_dir_all(&root)
        .map_err(|e| format!("Failed to create verification directory: {e}"))?;
    let app_path = root.join(prefix.trim_end_matches('/'));
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("Failed to read IPA entry: {e}"))?;
        let name = entry.name().to_string();
        if !name.starts_with(&prefix) {
            continue;
        }
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| format!("Unsafe IPA entry path: {name}"))?;
        let destination = root.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&destination)
                .map_err(|e| format!("Failed to create verification directory: {e}"))?;
            continue;
        }
        let unix_mode = entry.unix_mode();
        if unix_mode.is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err(format!("Symbolic-link IPA entry is not accepted: {name}"));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create verification directory: {e}"))?;
        }
        let mut output =
            File::create(&destination).map_err(|e| format!("Failed to extract {name}: {e}"))?;
        let copied = std::io::copy(&mut entry.take(512 * 1024 * 1024 + 1), &mut output)
            .map_err(|e| format!("Failed to extract {name}: {e}"))?;
        if copied > 512 * 1024 * 1024 {
            return Err(format!("IPA entry exceeds extraction limit: {name}"));
        }
        #[cfg(unix)]
        if let Some(mode) = unix_mode {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&destination, fs::Permissions::from_mode(mode & 0o777));
        }
    }
    Ok(ExtractedIpaApp { root, app_path })
}

fn inspect_android_artifact(
    path: &Path,
    kind: MobileArtifactKind,
    archive: &mut ZipArchive<File>,
    apk_signing_block: bool,
    sha256: String,
    size_bytes: u64,
    package_entries: usize,
) -> Result<MobileArtifact, String> {
    let mut manifest_bytes = None;
    let mut jar_signature = false;
    let mut modules = BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("Failed to read Android artifact entry: {e}"))?;
        let name = entry.name().to_string();
        if kind == MobileArtifactKind::Aab {
            if let Some((module, _)) = name.split_once('/') {
                if !module.is_empty() && module != "META-INF" {
                    modules.insert(module.to_string());
                }
            }
        }
        let is_manifest = match kind {
            MobileArtifactKind::Apk => name == "AndroidManifest.xml",
            MobileArtifactKind::Aab => name == "base/manifest/AndroidManifest.xml",
            MobileArtifactKind::Ipa => false,
        };
        if is_manifest {
            if entry.size() > 16 * 1024 * 1024 {
                return Err(
                    "Packaged AndroidManifest.xml exceeds the 16 MiB safety limit".to_string(),
                );
            }
            manifest_bytes = Some(read_limited(
                &mut entry,
                16 * 1024 * 1024,
                "packaged Android manifest",
            )?);
        }
        let upper = name.to_ascii_uppercase();
        if upper.starts_with("META-INF/")
            && (upper.ends_with(".RSA") || upper.ends_with(".DSA") || upper.ends_with(".EC"))
        {
            jar_signature = true;
        }
    }
    let signature_material = match (jar_signature, apk_signing_block) {
        (true, true) => "JAR and APK signing-block material present",
        (true, false) => "JAR signature material present",
        (false, true) => "APK signing-block material present",
        (false, false) => "no supported signature material detected",
    }
    .to_string();

    let manifest_bytes = manifest_bytes.ok_or_else(|| {
        format!(
            "{} does not contain the expected packaged AndroidManifest.xml",
            path.display()
        )
    })?;
    let mut parsed =
        if manifest_bytes.starts_with(b"<?xml") || manifest_bytes.starts_with(b"<manifest") {
            parse_packaged_xml_manifest(&manifest_bytes)
        } else {
            parse_binary_android_manifest(&manifest_bytes)
        };
    if kind == MobileArtifactKind::Aab {
        if let Some(official) = inspect_aab_with_bundletool(path) {
            merge_packaged_manifest(&mut parsed, official);
        }
    } else if kind == MobileArtifactKind::Apk {
        if let Some(official) = inspect_apk_with_apkanalyzer(path) {
            merge_packaged_manifest(&mut parsed, official);
        }
    }
    let mut limitations = Vec::new();
    if parsed.application_id.is_none() {
        limitations.push(
            "The packaged manifest encoding could not be decoded completely; verify identity and SDK metadata with bundletool/apkanalyzer in CI.".to_string(),
        );
    }
    if kind == MobileArtifactKind::Aab {
        limitations.push(
            "AAB manifests use protobuf encoding; the portable inspector uses deterministic string-pool extraction and may require bundletool for numeric SDK fields.".to_string(),
        );
    }
    let signature_verification =
        verify_android_signature(path, &kind, jar_signature, apk_signing_block);
    limitations.push("Play App Signing enrollment and store acceptance require a connected Google Play provider.".to_string());
    Ok(MobileArtifact {
        path: path.to_string_lossy().into_owned(),
        kind: kind.clone(),
        platform: MobilePlatform::Android,
        sha256,
        size_bytes,
        application_id: parsed.application_id,
        version: parsed.version_name,
        build: parsed.version_code,
        minimum_os_version: None,
        target_sdk: parsed.target_sdk,
        permissions: parsed.permissions,
        signature_material,
        signature_verification,
        privacy_manifest_present: None,
        privacy_manifests: Vec::new(),
        embedded_components: modules.into_iter().collect(),
        package_entries,
        limitations,
    })
}

#[derive(Default)]
struct PackagedAndroidManifest {
    application_id: Option<String>,
    version_name: Option<String>,
    version_code: Option<String>,
    target_sdk: Option<u32>,
    permissions: Vec<String>,
}

fn parse_packaged_xml_manifest(bytes: &[u8]) -> PackagedAndroidManifest {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return PackagedAndroidManifest::default();
    };
    let Ok(doc) = roxmltree::Document::parse(text) else {
        return PackagedAndroidManifest::default();
    };
    let root = doc.root_element();
    let mut permissions: Vec<String> = root
        .children()
        .filter(|n| n.is_element() && n.tag_name().name().starts_with("uses-permission"))
        .filter_map(|n| android_attr(n, "name").map(str::to_string))
        .collect();
    permissions.sort();
    permissions.dedup();
    let target_sdk = root
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "uses-sdk")
        .and_then(|n| android_attr(n, "targetSdkVersion"))
        .and_then(first_u32);
    PackagedAndroidManifest {
        application_id: root.attribute("package").map(str::to_string),
        version_name: android_attr(root, "versionName").map(str::to_string),
        version_code: android_attr(root, "versionCode").map(str::to_string),
        target_sdk,
        permissions,
    }
}

fn merge_packaged_manifest(
    target: &mut PackagedAndroidManifest,
    authoritative: PackagedAndroidManifest,
) {
    target.application_id = authoritative
        .application_id
        .or(target.application_id.take());
    target.version_name = authoritative.version_name.or(target.version_name.take());
    target.version_code = authoritative.version_code.or(target.version_code.take());
    target.target_sdk = authoritative.target_sdk.or(target.target_sdk);
    if !authoritative.permissions.is_empty() {
        target.permissions = authoritative.permissions;
    }
}

fn inspect_aab_with_bundletool(path: &Path) -> Option<PackagedAndroidManifest> {
    if !command_exists("bundletool") {
        return None;
    }
    let output = Command::new("bundletool")
        .current_dir(path.parent().unwrap_or_else(|| Path::new(".")))
        .args(["dump", "manifest", "--module=base"])
        .arg(format!("--bundle={}", path.display()))
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.len() > 16 * 1024 * 1024 {
        return None;
    }
    Some(parse_packaged_xml_manifest(&output.stdout))
}

fn inspect_apk_with_apkanalyzer(path: &Path) -> Option<PackagedAndroidManifest> {
    if !command_exists("apkanalyzer") {
        return None;
    }
    let output = Command::new("apkanalyzer")
        .current_dir(path.parent().unwrap_or_else(|| Path::new(".")))
        .args(["manifest", "print"])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.len() > 16 * 1024 * 1024 {
        return None;
    }
    Some(parse_packaged_xml_manifest(&output.stdout))
}

// Extracts the UTF-8/UTF-16 string pool from Android binary XML. Numeric typed
// attributes are intentionally not guessed; unavailable fields stay explicit.
fn parse_binary_android_manifest(bytes: &[u8]) -> PackagedAndroidManifest {
    let strings = extract_android_string_pool(bytes).unwrap_or_default();
    let application_id = strings
        .iter()
        .find(|s| looks_like_application_id(s))
        .cloned();
    let mut permissions: Vec<String> = strings
        .iter()
        .filter(|s| s.starts_with("android.permission."))
        .cloned()
        .collect();
    permissions.sort();
    permissions.dedup();
    let version_name = strings
        .windows(2)
        .find(|pair| pair[0] == "versionName")
        .map(|pair| pair[1].clone())
        .filter(|s| !s.starts_with("http"));
    PackagedAndroidManifest {
        application_id,
        version_name,
        permissions,
        ..Default::default()
    }
}

fn extract_android_string_pool(bytes: &[u8]) -> Option<Vec<String>> {
    let mut offset = 8usize;
    while offset + 28 <= bytes.len() {
        let chunk_type = u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?);
        let header_size =
            u16::from_le_bytes(bytes[offset + 2..offset + 4].try_into().ok()?) as usize;
        let chunk_size =
            u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        if chunk_size < header_size || offset + chunk_size > bytes.len() {
            return None;
        }
        if chunk_type == 0x0001 {
            let count =
                u32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().ok()?) as usize;
            let flags = u32::from_le_bytes(bytes[offset + 16..offset + 20].try_into().ok()?);
            let strings_start =
                u32::from_le_bytes(bytes[offset + 20..offset + 24].try_into().ok()?) as usize;
            let utf8 = flags & 0x100 != 0;
            let index_start = offset + header_size;
            let data_start = offset + strings_start;
            if index_start + count * 4 > bytes.len() || data_start > bytes.len() {
                return None;
            }
            let mut strings = Vec::with_capacity(count);
            for index in 0..count {
                let p = index_start + index * 4;
                let rel = u32::from_le_bytes(bytes[p..p + 4].try_into().ok()?) as usize;
                let at = data_start + rel;
                strings.push(if utf8 {
                    decode_android_utf8(bytes, at).unwrap_or_default()
                } else {
                    decode_android_utf16(bytes, at).unwrap_or_default()
                });
            }
            return Some(strings);
        }
        offset += chunk_size;
    }
    None
}

fn decode_length8(bytes: &[u8], at: &mut usize) -> Option<usize> {
    let first = *bytes.get(*at)?;
    *at += 1;
    if first & 0x80 == 0 {
        Some(first as usize)
    } else {
        let second = *bytes.get(*at)?;
        *at += 1;
        Some((((first & 0x7f) as usize) << 8) | second as usize)
    }
}

fn decode_android_utf8(bytes: &[u8], mut at: usize) -> Option<String> {
    let _utf16_len = decode_length8(bytes, &mut at)?;
    let byte_len = decode_length8(bytes, &mut at)?;
    let value = bytes.get(at..at + byte_len)?;
    Some(String::from_utf8_lossy(value).into_owned())
}

fn decode_android_utf16(bytes: &[u8], mut at: usize) -> Option<String> {
    let first = u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?);
    at += 2;
    let len = if first & 0x8000 == 0 {
        first as usize
    } else {
        let second = u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?);
        at += 2;
        (((first & 0x7fff) as usize) << 16) | second as usize
    };
    let raw = bytes.get(at..at + len * 2)?;
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(String::from_utf16_lossy(&units))
}

fn looks_like_application_id(value: &str) -> bool {
    value.len() >= 5
        && value.contains('.')
        && !value.contains('/')
        && !value.contains(':')
        && !value.starts_with("android.")
        && value.split('.').all(|part| {
            !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

fn is_build_variable(value: &str) -> bool {
    value.contains("$(")
        || value.contains("${")
        || value.contains("project.")
        || value.contains("providers.")
}

fn evaluate_source_rules(
    project_root: &Path,
    project: &MobileProject,
    policy: &MobilePolicy,
) -> Vec<MobileFinding> {
    let mut findings = Vec::new();
    if project.platforms.is_empty() {
        findings.push(finding(
            "MOB-PRJ-001",
            "Mobile project detected",
            "project",
            MobileSeverity::Blocker,
            MobileFindingStatus::Fail,
            Some("No iOS or Android project metadata detected".to_string()),
            Some("At least one supported mobile platform".to_string()),
            vec![project.root.clone()],
            "A mobile release audit must be anchored to a detected application project.",
            "Run from the repository root or add supported Flutter, iOS, or Android project metadata.",
            None,
        ));
        return findings;
    }
    findings.push(finding(
        "MOB-PRJ-001",
        "Mobile project detected",
        "project",
        MobileSeverity::Blocker,
        MobileFindingStatus::Pass,
        Some(format!("{:?}", project.platforms)),
        Some("At least one supported mobile platform".to_string()),
        vec![project.root.clone()],
        "The project boundary and platform must be known before release checks are meaningful.",
        "None.",
        None,
    ));
    if !project.scan_limitations.is_empty() {
        findings.push(finding(
            "MOB-PRJ-002",
            "Project scan coverage",
            "project",
            MobileSeverity::Warning,
            MobileFindingStatus::Unavailable,
            Some(project.scan_limitations.join("; ")),
            Some("All relevant first-party mobile source files readable within the project boundary".to_string()),
            Vec::new(),
            "A release decision must expose incomplete scan coverage instead of silently omitting files.",
            "Review the listed paths, remove unsafe links, or make required files readable before release.",
            None,
        ));
    }

    for app in &project.android_apps {
        let location = vec![app.gradle_file.clone()];
        let required_target_sdk = if app.name.to_ascii_lowercase().contains("wear") {
            policy.minimum_wear_target_sdk
        } else {
            policy.minimum_android_target_sdk
        };
        findings.push(match app.target_sdk {
            Some(value) if value >= required_target_sdk => finding(
                "MOB-AND-001",
                &format!("Android target SDK ({})", app.name),
                "android_policy",
                MobileSeverity::Blocker,
                MobileFindingStatus::Pass,
                Some(value.to_string()),
                Some(format!(">= {}", required_target_sdk)),
                location,
                "Google Play requires current target API levels for app updates.",
                "None.",
                Some("https://developer.android.com/google/play/requirements/target-sdk"),
            ),
            Some(value) => finding(
                "MOB-AND-001",
                &format!("Android target SDK ({})", app.name),
                "android_policy",
                MobileSeverity::Blocker,
                MobileFindingStatus::Fail,
                Some(value.to_string()),
                Some(format!(">= {}", required_target_sdk)),
                location,
                "An outdated target SDK can prevent Play submission and misses current platform protections.",
                "Raise targetSdk, resolve compatibility changes, and rebuild the release candidate.",
                Some("https://developer.android.com/google/play/requirements/target-sdk"),
            ),
            None => finding(
                "MOB-AND-001",
                &format!("Android target SDK ({})", app.name),
                "android_policy",
                MobileSeverity::Warning,
                MobileFindingStatus::Unavailable,
                Some("Resolved by Gradle or a framework variable".to_string()),
                Some(format!(">= {}", required_target_sdk)),
                location,
                "Static inspection cannot safely guess values resolved by Gradle.",
                "Audit the built AAB/APK and verify the resolved target SDK in CI with bundletool or apkanalyzer.",
                Some("https://developer.android.com/google/play/requirements/target-sdk"),
            ),
        });
        if app.debuggable == Some(true) {
            findings.push(finding(
                "MOB-AND-002",
                &format!("Android release is not debuggable ({})", app.name),
                "security",
                MobileSeverity::Blocker,
                MobileFindingStatus::Fail,
                Some("android:debuggable=true".to_string()),
                Some("debuggable absent or false".to_string()),
                app.manifest.clone().into_iter().collect(),
                "A debuggable production application exposes runtime inspection and data extraction paths.",
                "Remove the production debuggable flag and rebuild.",
                Some("https://developer.android.com/privacy-and-security/risks/android-debuggable"),
            ));
        } else {
            findings.push(finding(
                "MOB-AND-002",
                &format!(
                    "Android release is not explicitly debuggable ({})",
                    app.name
                ),
                "security",
                MobileSeverity::Blocker,
                MobileFindingStatus::Pass,
                app.debuggable.map(|v| v.to_string()),
                Some("debuggable absent or false".to_string()),
                app.manifest.clone().into_iter().collect(),
                "Production applications should not opt into debug behavior.",
                "None.",
                Some("https://developer.android.com/privacy-and-security/risks/android-debuggable"),
            ));
        }
        if app.cleartext_traffic == Some(true) {
            findings.push(finding(
                "MOB-AND-003",
                &format!("Android cleartext traffic disabled ({})", app.name),
                "security",
                MobileSeverity::Blocker,
                MobileFindingStatus::Fail,
                Some("android:usesCleartextTraffic=true".to_string()),
                Some("false or a reviewed network security configuration".to_string()),
                app.manifest.clone().into_iter().collect(),
                "Cleartext production traffic can expose credentials and user data.",
                "Disable cleartext traffic or document a narrowly scoped network security exception.",
                Some("https://developer.android.com/privacy-and-security/security-config"),
            ));
        }
        let identity_ok = app
            .application_id
            .as_deref()
            .is_some_and(looks_like_application_id);
        let android_identity_status = if identity_ok {
            MobileFindingStatus::Pass
        } else if app.application_id.as_deref().is_some_and(is_build_variable) {
            MobileFindingStatus::Unavailable
        } else {
            MobileFindingStatus::Warning
        };
        findings.push(finding(
            "MOB-AND-004",
            &format!("Android application identity ({})", app.name),
            "identity",
            MobileSeverity::Blocker,
            android_identity_status,
            app.application_id.clone(),
            Some("A stable reverse-domain applicationId".to_string()),
            vec![app.gradle_file.clone()],
            "Store continuity, signing, billing, and deep links depend on stable package identity.",
            "Set applicationId explicitly or prove the resolved value from the built artifact.",
            None,
        ));
        if !app.release_signing_configured {
            findings.push(finding(
                "MOB-AND-005",
                &format!("Android release signing configuration ({})", app.name),
                "signing",
                MobileSeverity::Warning,
                MobileFindingStatus::Manual,
                Some("No non-debug signingConfig found statically".to_string()),
                Some("Release signing configured outside source-controlled secrets".to_string()),
                vec![app.gradle_file.clone()],
                "A release candidate must be signed by the intended protected key or Play signing workflow.",
                "Verify the CI signing path and inspect the exact AAB/APK; never commit signing secrets.",
                None,
            ));
        }
        let sensitive: Vec<String> = app
            .permissions
            .iter()
            .filter(|p| is_sensitive_android_permission(p))
            .cloned()
            .collect();
        if !sensitive.is_empty() {
            findings.push(finding(
                "MOB-AND-006",
                &format!("Sensitive Android permissions reviewed ({})", app.name),
                "privacy",
                MobileSeverity::Warning,
                MobileFindingStatus::Manual,
                Some(sensitive.join(", ")),
                Some("Permissions match product behavior and Play declarations".to_string()),
                app.manifest.clone().into_iter().collect(),
                "Sensitive permissions affect consent UX, privacy disclosure, and store review.",
                "Remove unused permissions and attach a permission/store-declaration review to the release.",
                Some("https://support.google.com/googleplay/android-developer/answer/9888170"),
            ));
        }
    }

    for target in &project.ios_targets {
        let info_location = vec![target.info_plist.clone()];
        let identity_present = target
            .bundle_identifier
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty());
        let identity_status = if target
            .bundle_identifier
            .as_deref()
            .is_some_and(is_build_variable)
        {
            MobileFindingStatus::Unavailable
        } else if identity_present {
            MobileFindingStatus::Pass
        } else {
            MobileFindingStatus::Warning
        };
        findings.push(finding(
            "MOB-IOS-001",
            &format!("iOS bundle identity ({})", target.name),
            "identity",
            MobileSeverity::Blocker,
            identity_status,
            target.bundle_identifier.clone(),
            Some("CFBundleIdentifier present and resolved in the artifact".to_string()),
            info_location.clone(),
            "Store continuity, signing, entitlements, and deep links depend on a stable bundle identifier.",
            "Set a bundle identifier and confirm the resolved value in the IPA.",
            None,
        ));
        let version_present = target
            .version
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty())
            && target
                .build
                .as_deref()
                .is_some_and(|v| !v.trim().is_empty());
        let version_unresolved = target.version.as_deref().is_some_and(is_build_variable)
            || target.build.as_deref().is_some_and(is_build_variable);
        findings.push(finding(
            "MOB-IOS-002",
            &format!("iOS marketing version and build ({})", target.name),
            "versioning",
            MobileSeverity::Blocker,
            if version_unresolved {
                MobileFindingStatus::Unavailable
            } else if version_present {
                MobileFindingStatus::Pass
            } else {
                MobileFindingStatus::Warning
            },
            Some(format!("version={:?}, build={:?}", target.version, target.build)),
            Some("CFBundleShortVersionString and CFBundleVersion".to_string()),
            info_location.clone(),
            "Every submitted build needs an unambiguous store version and monotonically managed build number.",
            "Set both values and prove their resolved values from the IPA.",
            None,
        ));
        match &target.privacy_manifest {
            Some(rel) => {
                let manifest_path = project_root.join(rel);
                let valid = PlistValue::from_file(&manifest_path)
                    .ok()
                    .and_then(|v| v.into_dictionary())
                    .is_some();
                findings.push(finding(
                    "MOB-IOS-003",
                    &format!("iOS privacy manifest is valid ({})", target.name),
                    "privacy",
                    MobileSeverity::Blocker,
                    if valid { MobileFindingStatus::Pass } else { MobileFindingStatus::Fail },
                    Some(rel.clone()),
                    Some("A parseable PrivacyInfo.xcprivacy dictionary".to_string()),
                    vec![rel.clone()],
                    "Apple rejects invalid privacy manifests and requires them for covered SDKs and APIs.",
                    "Repair the property list and verify required-reason APIs and collected-data declarations.",
                    Some("https://developer.apple.com/documentation/bundleresources/adding-a-privacy-manifest-to-your-app-or-third-party-sdk"),
                ));
            }
            None => findings.push(finding(
                "MOB-IOS-003",
                &format!("iOS privacy manifest coverage ({})", target.name),
                "privacy",
                MobileSeverity::Warning,
                MobileFindingStatus::Manual,
                Some("No nearby PrivacyInfo.xcprivacy detected".to_string()),
                Some("Manifest present when the target or included SDKs require it".to_string()),
                info_location.clone(),
                "Privacy manifest need depends on APIs and third-party SDK inventory; absence cannot always be classified statically.",
                "Inventory SDKs and required-reason APIs, then add or attach evidence that a manifest is not required.",
                Some("https://developer.apple.com/support/third-party-SDK-requirements/"),
            )),
        }
        if target.usage_descriptions.is_empty() {
            findings.push(finding(
                "MOB-IOS-004",
                &format!("iOS permission-purpose strings reviewed ({})", target.name),
                "privacy",
                MobileSeverity::Warning,
                MobileFindingStatus::Manual,
                Some("No NS*UsageDescription keys detected".to_string()),
                Some("Purpose strings for every protected resource the target accesses".to_string()),
                info_location,
                "Missing purpose strings can cause runtime termination or inaccurate consent UX, but not every target needs them.",
                "Map protected API usage to Info.plist purpose strings and attach a permission review.",
                Some("https://developer.apple.com/documentation/bundleresources/information-property-list/protected-resources"),
            ));
        }
    }

    findings.extend(scan_production_placeholders(project_root, policy));
    findings.extend(evaluate_public_urls(project_root, policy));
    findings
}

fn evaluate_artifact_rules(artifact: &MobileArtifact, policy: &MobilePolicy) -> Vec<MobileFinding> {
    let mut findings = Vec::new();
    findings.push(finding(
        "MOB-ART-001",
        "Release artifact inspected and hashed",
        "artifact",
        MobileSeverity::Blocker,
        MobileFindingStatus::Pass,
        Some(format!("{} sha256={}", artifact.path, artifact.sha256)),
        Some("Exact candidate artifact with SHA-256 provenance".to_string()),
        vec![artifact.path.clone()],
        "Release evidence must describe the exact immutable candidate, not only source intent.",
        "None.",
        None,
    ));
    findings.push(finding(
        "MOB-ART-002",
        "Artifact application identity",
        "identity",
        MobileSeverity::Blocker,
        if artifact.application_id.as_deref().is_some_and(|v| !v.trim().is_empty()) {
            MobileFindingStatus::Pass
        } else {
            MobileFindingStatus::Fail
        },
        artifact.application_id.clone(),
        Some("Resolved application/bundle identifier".to_string()),
        vec![artifact.path.clone()],
        "The release decision must be tied to a known store identity.",
        "Inspect with platform tooling if portable decoding is unavailable; do not approve an unidentified artifact.",
        None,
    ));
    findings.push(finding(
        "MOB-ART-003",
        "Artifact version and build identity",
        "versioning",
        MobileSeverity::Blocker,
        if artifact.version.is_some() && artifact.build.is_some() {
            MobileFindingStatus::Pass
        } else {
            MobileFindingStatus::Warning
        },
        Some(format!(
            "version={:?}, build={:?}",
            artifact.version, artifact.build
        )),
        Some("Resolved store version and build/version code".to_string()),
        vec![artifact.path.clone()],
        "Release history and store submission require a unique version/build identity.",
        "Resolve missing values with platform tooling and attach the output to the release.",
        None,
    ));
    let signing_status = match artifact.signature_verification.status {
        MobileSignatureStatus::Verified if policy.approved_signing_identities.is_empty() => {
            MobileFindingStatus::Pass
        }
        MobileSignatureStatus::Verified => {
            let details = artifact.signature_verification.details.to_ascii_lowercase();
            let fingerprints: BTreeSet<String> = artifact
                .signature_verification
                .certificate_fingerprints
                .iter()
                .map(|value| value.to_ascii_lowercase())
                .collect();
            if policy.approved_signing_identities.iter().any(|approved| {
                let approved = approved.trim().to_ascii_lowercase();
                fingerprints.contains(&approved) || details.contains(&approved)
            }) {
                MobileFindingStatus::Pass
            } else {
                MobileFindingStatus::Fail
            }
        }
        MobileSignatureStatus::Failed => MobileFindingStatus::Fail,
        MobileSignatureStatus::Unavailable => MobileFindingStatus::Unavailable,
    };
    findings.push(finding(
        "MOB-ART-004",
        "Artifact signature verification",
        "signing",
        MobileSeverity::Blocker,
        signing_status,
        Some(format!(
            "material={}; verifier={}; result={}",
            artifact.signature_material,
            artifact.signature_verification.verifier.as_deref().unwrap_or("unavailable"),
            artifact.signature_verification.details
        )),
        Some(if policy.approved_signing_identities.is_empty() {
            "Cryptographically valid platform signature".to_string()
        } else {
            format!("Verified signature matching one of: {}", policy.approved_signing_identities.join(", "))
        }),
        vec![artifact.path.clone()],
        "Signature files alone do not prove that an artifact is intact or signed by a valid certificate.",
        "Install the platform verifier, sign the exact candidate with the protected production workflow, and rerun the audit.",
        None,
    ));
    if artifact.platform == MobilePlatform::Android {
        findings.push(match artifact.target_sdk {
            Some(value) if value >= policy.minimum_android_target_sdk => finding(
                "MOB-ART-005",
                "Packaged Android target SDK",
                "android_policy",
                MobileSeverity::Blocker,
                MobileFindingStatus::Pass,
                Some(value.to_string()),
                Some(format!(">= {}", policy.minimum_android_target_sdk)),
                vec![artifact.path.clone()],
                "The packaged value is the release-relevant source of truth.",
                "None.",
                Some("https://developer.android.com/google/play/requirements/target-sdk"),
            ),
            Some(value) => finding(
                "MOB-ART-005",
                "Packaged Android target SDK",
                "android_policy",
                MobileSeverity::Blocker,
                MobileFindingStatus::Fail,
                Some(value.to_string()),
                Some(format!(">= {}", policy.minimum_android_target_sdk)),
                vec![artifact.path.clone()],
                "The submitted artifact must satisfy the current Play target API requirement.",
                "Raise targetSdk and rebuild the candidate.",
                Some("https://developer.android.com/google/play/requirements/target-sdk"),
            ),
            None => finding(
                "MOB-ART-005",
                "Packaged Android target SDK",
                "android_policy",
                MobileSeverity::Warning,
                MobileFindingStatus::Unavailable,
                Some("Portable manifest decoder could not resolve the numeric value".to_string()),
                Some(format!(">= {}", policy.minimum_android_target_sdk)),
                vec![artifact.path.clone()],
                "AAB protobuf and binary manifest fields sometimes require official platform tooling.",
                "Run bundletool or apkanalyzer in CI and attach its output before approval.",
                Some("https://developer.android.com/google/play/requirements/target-sdk"),
            ),
        });
    }
    if artifact.platform == MobilePlatform::Ios && artifact.privacy_manifest_present == Some(false)
    {
        findings.push(finding(
            "MOB-ART-006",
            "Packaged iOS privacy manifest coverage",
            "privacy",
            MobileSeverity::Warning,
            MobileFindingStatus::Manual,
            Some("No PrivacyInfo.xcprivacy at the app root".to_string()),
            Some("Manifest packaged when APIs or SDKs require it".to_string()),
            vec![artifact.path.clone()],
            "Source presence does not guarantee the manifest was copied into the candidate.",
            "Verify the SDK/API inventory and package the applicable privacy manifest.",
            Some(
                "https://developer.apple.com/documentation/bundleresources/privacy-manifest-files",
            ),
        ));
    }
    findings
}

fn scan_production_placeholders(project_root: &Path, policy: &MobilePolicy) -> Vec<MobileFinding> {
    let Ok((files, _)) = collect_project_files(project_root) else {
        return Vec::new();
    };
    let mut matches = Vec::new();
    for path in files {
        let rel = relative(project_root, &path);
        let lower = rel.to_ascii_lowercase();
        if lower.contains("/test/")
            || lower.starts_with("test/")
            || lower.starts_with("integration_test/")
        {
            continue;
        }
        if !(lower.ends_with(".dart")
            || lower.ends_with(".plist")
            || lower.ends_with(".xcconfig")
            || lower.ends_with("build.gradle")
            || lower.ends_with("build.gradle.kts")
            || lower.ends_with("gradle.properties"))
        {
            continue;
        }
        let Some(content) = read_text(&path) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//")
                || trimmed.starts_with('#')
                || trimmed.starts_with("<!--")
                || trimmed.starts_with('*')
            {
                continue;
            }
            let allow = line.contains("gantry: allow-placeholder(reason=") && line.contains(')');
            if allow {
                continue;
            }
            let line_lower = line.to_ascii_lowercase();
            for pattern in &policy.production_placeholder_patterns {
                if line_lower.contains(&pattern.to_ascii_lowercase()) {
                    matches.push(format!("{rel}:{}: {pattern}", index + 1));
                }
            }
        }
    }
    matches.sort();
    matches.dedup();
    if matches.is_empty() {
        vec![finding(
            "MOB-CFG-001",
            "No production placeholder endpoints detected",
            "configuration",
            MobileSeverity::Blocker,
            MobileFindingStatus::Pass,
            None,
            Some("No configured placeholder patterns in mobile release configuration".to_string()),
            Vec::new(),
            "Development endpoints and placeholder credentials are common release regressions.",
            "None.",
            None,
        )]
    } else {
        vec![finding(
            "MOB-CFG-001",
            "No production placeholder endpoints detected",
            "configuration",
            MobileSeverity::Blocker,
            MobileFindingStatus::Fail,
            Some(matches.join("; ")),
            Some("No configured placeholder patterns in mobile release configuration".to_string()),
            matches
                .iter()
                .filter_map(|m| m.split(':').next().map(str::to_string))
                .collect(),
            "A production build pointed at development or placeholder infrastructure can leak data or break the release.",
            "Replace the values or move environment configuration to a reviewed release-time mechanism.",
            None,
        )]
    }
}

fn evaluate_public_urls(project_root: &Path, policy: &MobilePolicy) -> Vec<MobileFinding> {
    let mut observed = BTreeMap::new();
    let roots = [
        project_root.join(".gantry").join("store_metadata"),
        project_root.join(".gantry").join("store-metadata"),
    ];
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            collect_public_urls(&value, &mut observed);
        }
    }
    let mut missing = Vec::new();
    let mut invalid = Vec::new();
    for required in &policy.required_public_urls {
        match observed.get(required) {
            Some(url) if url.starts_with("https://") => {}
            Some(url) => invalid.push(format!("{required}={url}")),
            None => missing.push(required.clone()),
        }
    }
    if !invalid.is_empty() {
        vec![finding(
            "MOB-STR-001",
            "Required public store URLs use HTTPS",
            "store_metadata",
            MobileSeverity::Blocker,
            MobileFindingStatus::Fail,
            Some(invalid.join(", ")),
            Some("Public HTTPS privacy/support URLs".to_string()),
            Vec::new(),
            "Invalid or non-HTTPS public URLs can break store review and user access to required disclosures.",
            "Publish stable HTTPS pages and update the store metadata record.",
            None,
        )]
    } else if !missing.is_empty() {
        vec![finding(
            "MOB-STR-001",
            "Required public store URLs reviewed",
            "store_metadata",
            MobileSeverity::Warning,
            MobileFindingStatus::Manual,
            Some(format!(
                "Missing from local store metadata: {}",
                missing.join(", ")
            )),
            Some("Public HTTPS privacy/support URLs or release-bound store evidence".to_string()),
            Vec::new(),
            "Store URLs live outside many source trees, so absence is a required reconciliation step rather than a guessed blocker.",
            "Add privacyPolicyUrl/supportUrl to .gantry/store_metadata JSON or attach store_listing_review evidence.",
            None,
        )]
    } else {
        vec![finding(
            "MOB-STR-001",
            "Required public store URLs reviewed",
            "store_metadata",
            MobileSeverity::Warning,
            MobileFindingStatus::Pass,
            Some(observed.values().cloned().collect::<Vec<_>>().join(", ")),
            Some("Public HTTPS privacy/support URLs".to_string()),
            Vec::new(),
            "The required public disclosure and support endpoints are represented locally.",
            "Confirm the same values are configured in each store before release.",
            None,
        )]
    }
}

fn collect_public_urls(value: &serde_json::Value, output: &mut BTreeMap<String, String>) {
    let Some(object) = value.as_object() else {
        return;
    };
    for (key, value) in object {
        let normalized = normalize_evidence_name(key);
        let canonical = match normalized.as_str() {
            "privacy_policy_url" | "privacypolicyurl" | "privacy_url" => Some("privacy_policy"),
            "support_url" | "supporturl" => Some("support"),
            "terms_url" | "terms_of_service_url" | "termsofserviceurl" => Some("terms"),
            _ => None,
        };
        if let (Some(canonical), Some(url)) = (canonical, value.as_str()) {
            output.insert(canonical.to_string(), url.to_string());
        }
        if value.is_object() {
            collect_public_urls(value, output);
        }
    }
}

fn is_sensitive_android_permission(permission: &str) -> bool {
    [
        "CAMERA",
        "RECORD_AUDIO",
        "READ_CONTACTS",
        "WRITE_CONTACTS",
        "ACCESS_FINE_LOCATION",
        "ACCESS_COARSE_LOCATION",
        "ACCESS_BACKGROUND_LOCATION",
        "READ_PHONE_STATE",
        "READ_SMS",
        "BODY_SENSORS",
        "POST_NOTIFICATIONS",
        "READ_MEDIA_",
    ]
    .iter()
    .any(|needle| permission.ends_with(needle) || permission.contains(needle))
}

#[allow(clippy::too_many_arguments)]
fn finding(
    id: &str,
    title: &str,
    category: &str,
    severity: MobileSeverity,
    status: MobileFindingStatus,
    observed: Option<String>,
    expected: Option<String>,
    locations: Vec<String>,
    rationale: &str,
    remediation: &str,
    authoritative_source: Option<&str>,
) -> MobileFinding {
    let subject_id = locations
        .first()
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| normalize_evidence_name(title));
    let finding_instance_id = format!("{id}:{subject_id}");
    MobileFinding {
        id: id.to_string(),
        rule_id: id.to_string(),
        subject_id,
        finding_instance_id,
        title: title.to_string(),
        category: category.to_string(),
        severity,
        status,
        observed,
        expected,
        locations,
        rationale: rationale.to_string(),
        remediation: remediation.to_string(),
        authoritative_source: authoritative_source.map(str::to_string),
        source_reviewed_at: authoritative_source.map(|_| MOBILE_POLICY_REVIEWED_AT.to_string()),
    }
}

fn manual_finding(
    id: &str,
    title: &str,
    category: &str,
    rationale: &str,
    remediation: &str,
) -> MobileFinding {
    finding(
        id,
        title,
        category,
        MobileSeverity::Warning,
        MobileFindingStatus::Manual,
        None,
        Some("Release-bound evidence".to_string()),
        Vec::new(),
        rationale,
        remediation,
        None,
    )
}

fn stable_rule_suffix(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    hex::encode(&digest[..3]).to_ascii_uppercase()
}

pub fn mobile_evidence_rule_id(kind: &str) -> String {
    let normalized = normalize_evidence_name(kind);
    format!("MOB-EVD-{}", stable_rule_suffix(&normalized))
}

struct MobileEvidenceRejection {
    path: String,
    reason: String,
}

#[derive(Default)]
struct MobileEvidenceLoad {
    records: Vec<crate::EvidenceRecord>,
    rejections: Vec<MobileEvidenceRejection>,
}

fn load_mobile_evidence(project_root: &Path) -> MobileEvidenceLoad {
    let directory = project_root.join(".gantry").join("evidence");
    let Ok(entries) = fs::read_dir(directory) else {
        return MobileEvidenceLoad::default();
    };
    let mut loaded = MobileEvidenceLoad::default();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let rel = report_path(project_root, &path);
        let Ok(bytes) = fs::read(&path) else {
            loaded.rejections.push(MobileEvidenceRejection {
                path: rel,
                reason: "Evidence file could not be read".to_string(),
            });
            continue;
        };
        match serde_json::from_slice::<crate::EvidenceRecord>(&bytes) {
            Ok(record) => match validate_evidence_record(&record) {
                Ok(()) => loaded.records.push(record),
                Err(reason) => loaded
                    .rejections
                    .push(MobileEvidenceRejection { path: rel, reason }),
            },
            Err(error) => loaded.rejections.push(MobileEvidenceRejection {
                path: rel,
                reason: format!("Invalid evidence JSON: {error}"),
            }),
        }
    }
    loaded.records.sort_by(|a, b| a.id.cmp(&b.id));
    loaded.rejections.sort_by(|a, b| a.path.cmp(&b.path));
    loaded
}

fn validate_evidence_record(record: &crate::EvidenceRecord) -> Result<(), String> {
    if record.id.trim().is_empty() || record.source.trim().is_empty() {
        return Err("id and source must be non-empty".to_string());
    }
    let created = chrono::DateTime::parse_from_rfc3339(&record.created_at)
        .map_err(|_| "created_at must be valid RFC 3339".to_string())?;
    if created > Utc::now() + Duration::minutes(5) {
        return Err("created_at is in the future".to_string());
    }
    if let Some(value) = record.expires_at.as_deref() {
        let expires = chrono::DateTime::parse_from_rfc3339(value)
            .map_err(|_| "expires_at must be valid RFC 3339".to_string())?;
        if expires <= created {
            return Err("expires_at must be later than created_at".to_string());
        }
    }
    if let Some(value) = record.artifact_sha.as_deref() {
        if value.len() != 64 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("artifact_sha must be a 64-character SHA-256 digest".to_string());
        }
    }
    let explicit_kind = record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("mobile_evidence_kind"))
        .and_then(|value| value.as_str());
    if explicit_kind.is_some_and(|value| normalize_evidence_name(value).is_empty()) {
        return Err("metadata.mobile_evidence_kind must be a non-empty string".to_string());
    }
    if explicit_kind.is_some() {
        let schema_version = record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("schema_version"))
            .and_then(|value| value.as_str());
        if schema_version != Some("1.0") {
            return Err("mobile evidence metadata.schema_version must equal 1.0".to_string());
        }
    }
    Ok(())
}

fn matching_mobile_evidence<'a>(
    records: &'a [crate::EvidenceRecord],
    required_kind: &str,
    git_commit: &str,
    artifact_sha: Option<&str>,
    trusted_issuers: &[String],
) -> Vec<&'a crate::EvidenceRecord> {
    records
        .iter()
        .filter(|record| {
            if record.expires_at.as_deref().is_some_and(|value| {
                chrono::DateTime::parse_from_rfc3339(value)
                    .is_ok_and(|expires| expires <= Utc::now())
            }) {
                return false;
            }
            let metadata_kind = record
                .metadata
                .as_ref()
                .and_then(|m| m.get("mobile_evidence_kind"))
                .and_then(|v| v.as_str());
            let kind_matches =
                metadata_kind.map(normalize_evidence_name).as_deref() == Some(required_kind);
            if !kind_matches {
                return false;
            }
            if !trusted_issuers.is_empty() {
                let issuer = record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("issuer"))
                    .and_then(|value| value.as_str())
                    .map(str::trim);
                if !trusted_issuers
                    .iter()
                    .any(|trusted| issuer.is_some_and(|value| value.eq_ignore_ascii_case(trusted)))
                {
                    return false;
                }
            }
            let commit_matches =
                git_commit != "unknown" && record.commit.as_deref() == Some(git_commit);
            let artifact_matches =
                artifact_sha.is_some() && record.artifact_sha.as_deref() == artifact_sha;
            commit_matches || artifact_matches
        })
        .collect()
}

fn normalize_evidence_name(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn summarize_findings(findings: &[MobileFinding]) -> MobileAuditSummary {
    let mut summary = MobileAuditSummary::default();
    for finding in findings {
        match finding.status {
            MobileFindingStatus::Pass => summary.passed += 1,
            MobileFindingStatus::Fail if finding.severity == MobileSeverity::Blocker => {
                summary.blockers += 1
            }
            MobileFindingStatus::Fail | MobileFindingStatus::Warning => summary.warnings += 1,
            MobileFindingStatus::Manual => summary.manual += 1,
            MobileFindingStatus::Unavailable => summary.unavailable += 1,
        }
    }
    summary
}

fn current_git_commit(project_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(project_root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn current_git_branch(project_root: &Path) -> Option<String> {
    git_stdout(project_root, &["rev-parse", "--abbrev-ref", "HEAD"]).filter(|value| value != "HEAD")
}

fn current_git_dirty(project_root: &Path) -> Option<bool> {
    git_stdout(
        project_root,
        &["status", "--porcelain", "--untracked-files=normal"],
    )
    .map(|value| !value.is_empty())
}

fn git_stdout(project_root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn sha256_json<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|e| format!("Failed to serialize mobile governance data: {e}"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn render_mobile_report(
    report: &MobileAuditReport,
    format: MobileReportFormat,
) -> Result<String, String> {
    match format {
        MobileReportFormat::Json => serde_json::to_string_pretty(report)
            .map(|s| format!("{s}\n"))
            .map_err(|e| format!("Failed to serialize mobile audit report: {e}")),
        MobileReportFormat::Markdown => Ok(render_mobile_markdown(report)),
        MobileReportFormat::Html => Ok(render_mobile_html(report)),
    }
}

pub fn write_mobile_report(
    report: &MobileAuditReport,
    output: Option<&Path>,
    format: MobileReportFormat,
) -> Result<PathBuf, String> {
    let path = output.map(Path::to_path_buf).unwrap_or_else(|| {
        let timestamp = Utc::now().format("%Y%m%dT%H%M%S%.9fZ");
        let fingerprint = report
            .report_fingerprint
            .chars()
            .take(12)
            .collect::<String>();
        PathBuf::from(&report.project_root)
            .join(".gantry")
            .join("mobile")
            .join("audits")
            .join(format!(
                "mobile-audit-{timestamp}-{fingerprint}.{}",
                format.extension()
            ))
    });
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Failed to create report directory {}: {e}",
                parent.display()
            )
        })?;
    }
    let body = render_mobile_report(report, format)?;
    let temp_name = format!(
        ".{}.tmp-{}-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    let temp_path = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(temp_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|e| {
            format!(
                "Failed to create temporary report {}: {e}",
                temp_path.display()
            )
        })?;
    if let Err(error) = file
        .write_all(body.as_bytes())
        .and_then(|_| file.sync_all())
    {
        let _ = fs::remove_file(&temp_path);
        return Err(format!(
            "Failed to write mobile report {}: {error}",
            path.display()
        ));
    }
    drop(file);
    fs::rename(&temp_path, &path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!("Failed to publish mobile report {}: {e}", path.display())
    })?;
    Ok(path)
}

fn render_mobile_markdown(report: &MobileAuditReport) -> String {
    let artifact = report
        .artifact
        .as_ref()
        .map(|a| format!("`{}` (`{}`)", a.path, a.sha256))
        .unwrap_or_else(|| "Not supplied".to_string());
    let mut out = format!(
        "# Mobile Release Audit\n\n- Decision: **{:?}**\n- Decision reasons: {}\n- Generated: `{}`\n- Tool: `{}`\n- Commit: `{}`\n- Branch: `{}`\n- Dirty worktree: `{}`\n- Policy pack: `{}` (reviewed `{}`)\n- Policy digest: `{}`\n- Report fingerprint: `{}`\n- Artifact: {}\n- Results: {} passed, {} blockers, {} warnings, {} manual, {} unavailable\n\n## Findings\n\n",
        report.decision,
        if report.decision_reasons.is_empty() {
            "None".to_string()
        } else {
            report.decision_reasons.join("; ")
        },
        report.generated_at,
        report.tool_version,
        report.git_commit,
        report.git_branch.as_deref().unwrap_or("unknown"),
        report
            .git_dirty
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        report.policy.policy_pack_version,
        report.policy.reviewed_at,
        report.policy_digest,
        report.report_fingerprint,
        artifact,
        report.summary.passed,
        report.summary.blockers,
        report.summary.warnings,
        report.summary.manual,
        report.summary.unavailable,
    );
    for finding in &report.findings {
        out.push_str(&format!(
            "### {} — {}\n\n- Rule: `{}`\n- Subject: `{}`\n- Status: **{:?}**\n- Severity: `{:?}`\n- Category: `{}`\n",
            finding.finding_instance_id, finding.title, finding.rule_id, finding.subject_id, finding.status, finding.severity, finding.category
        ));
        if let Some(observed) = &finding.observed {
            out.push_str(&format!("- Observed: {}\n", markdown_inline(observed)));
        }
        if let Some(expected) = &finding.expected {
            out.push_str(&format!("- Expected: {}\n", markdown_inline(expected)));
        }
        if !finding.locations.is_empty() {
            out.push_str(&format!(
                "- Locations: {}\n",
                finding
                    .locations
                    .iter()
                    .map(|v| format!("`{v}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if let Some(source) = &finding.authoritative_source {
            out.push_str(&format!(
                "- Authority: [{}]({}) (reviewed `{}`)\n",
                source,
                source,
                finding.source_reviewed_at.as_deref().unwrap_or("unknown")
            ));
        }
        out.push_str(&format!(
            "\n{}\n\nRemediation: {}\n\n",
            finding.rationale, finding.remediation
        ));
    }
    if let Some(artifact) = &report.artifact {
        out.push_str("## Inspector limitations\n\n");
        for limitation in &artifact.limitations {
            out.push_str(&format!("- {limitation}\n"));
        }
        out.push('\n');
    }
    out
}

fn markdown_inline(value: &str) -> String {
    value.replace('\n', " ").replace('|', "\\|")
}

fn render_mobile_html(report: &MobileAuditReport) -> String {
    let mut body = format!(
        "<h1>Mobile Release Audit</h1><dl><dt>Decision</dt><dd><strong>{:?}</strong></dd><dt>Decision reasons</dt><dd>{}</dd><dt>Generated</dt><dd><code>{}</code></dd><dt>Commit</dt><dd><code>{}</code></dd><dt>Policy digest</dt><dd><code>{}</code></dd><dt>Report fingerprint</dt><dd><code>{}</code></dd></dl><p>{} passed · {} blockers · {} warnings · {} manual · {} unavailable</p><h2>Findings</h2>",
        report.decision,
        html_escape(&report.decision_reasons.join("; ")),
        html_escape(&report.generated_at),
        html_escape(&report.git_commit),
        html_escape(&report.policy_digest),
        html_escape(&report.report_fingerprint),
        report.summary.passed,
        report.summary.blockers,
        report.summary.warnings,
        report.summary.manual,
        report.summary.unavailable
    );
    for finding in &report.findings {
        body.push_str(&format!(
            "<article><h3>{} — {}</h3><dl><dt>Status</dt><dd><strong>{:?}</strong></dd><dt>Severity</dt><dd>{:?}</dd><dt>Category</dt><dd>{}</dd></dl>",
            html_escape(&finding.finding_instance_id), html_escape(&finding.title), finding.status, finding.severity, html_escape(&finding.category)
        ));
        if let Some(observed) = &finding.observed {
            body.push_str(&format!(
                "<p><b>Observed:</b> {}</p>",
                html_escape(observed)
            ));
        }
        body.push_str(&format!(
            "<p>{}</p><p><b>Remediation:</b> {}</p>",
            html_escape(&finding.rationale),
            html_escape(&finding.remediation)
        ));
        if let Some(source) = &finding.authoritative_source {
            body.push_str(&format!(
                "<p><a href=\"{}\" rel=\"noreferrer\">Authoritative source</a></p>",
                html_escape(source)
            ));
        }
        body.push_str("</article>");
    }
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Gantry mobile release audit</title><style>body{{font:15px/1.55 system-ui,sans-serif;max-width:1040px;margin:40px auto;padding:0 24px;color:#182029}}h1{{font-size:32px}}h2{{margin-top:36px;border-bottom:1px solid #dce2e8;padding-bottom:8px}}article{{border-bottom:1px solid #dce2e8;padding:12px 0 20px}}dl{{display:grid;grid-template-columns:max-content 1fr;gap:4px 12px}}dt{{font-weight:700}}dd{{margin:0;overflow-wrap:anywhere}}code{{background:#f3f5f7;padding:2px 5px;border-radius:4px}}a{{color:#075985}}</style></head><body>{body}</body></html>\n"
    )
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn mobile_rule_catalog() -> serde_json::Value {
    serde_json::json!({
        "schema_version": MOBILE_REPORT_SCHEMA_VERSION,
        "policy_pack_version": MOBILE_POLICY_PACK_VERSION,
        "reviewed_at": MOBILE_POLICY_REVIEWED_AT,
        "rules": MOBILE_RULE_DEFINITIONS
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    use zip::write::SimpleFileOptions;

    static TEST_PROJECT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    struct TestProject(PathBuf);

    impl TestProject {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "gantry-mobile-{name}-{}-{}",
                std::process::id(),
                TEST_PROJECT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, rel: &str, value: &str) {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, value).unwrap();
        }
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn ios_info_plist(bundle: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Fixture</string>
<key>CFBundleIdentifier</key><string>{bundle}</string>
<key>CFBundleShortVersionString</key><string>1.2.3</string>
<key>CFBundleVersion</key><string>42</string>
</dict></plist>"#
        )
    }

    fn privacy_manifest() -> &'static str {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>NSPrivacyTracking</key><false/></dict></plist>"#
    }

    fn make_flutter_project(target_sdk: u32) -> TestProject {
        let project = TestProject::new("project");
        project.write(
            "pubspec.yaml",
            "name: fixture\ndependencies:\n  flutter:\n    sdk: flutter\n",
        );
        project.write(
            "ios/Runner/Info.plist",
            &ios_info_plist("com.example.fixture"),
        );
        project.write("ios/Runner/PrivacyInfo.xcprivacy", privacy_manifest());
        project.write(
            "android/app/build.gradle.kts",
            &format!(
                r#"plugins {{ id("com.android.application") }}
android {{
 namespace = "com.example.fixture"
 defaultConfig {{
  applicationId = "com.example.fixture"
  targetSdk = {target_sdk}
  minSdk = 26
  versionCode = 42
  versionName = "1.2.3"
 }}
 buildTypes {{ release {{ signingConfig = signingConfigs.getByName("release") }} }}
}}"#
            ),
        );
        project.write(
            "android/app/src/main/AndroidManifest.xml",
            r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android"><application android:debuggable="false" android:usesCleartextTraffic="false"/></manifest>"#,
        );
        project
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for (name, body) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(body).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn detects_first_party_flutter_targets() {
        let project = make_flutter_project(35);
        project.write(
            "ios/.symlinks/plugins/noise/example/ios/Runner/Info.plist",
            &ios_info_plist("com.noise.plugin"),
        );
        let detected = detect_mobile_project(&project.0).unwrap();
        assert_eq!(detected.stacks, vec![MobileStack::Flutter]);
        assert_eq!(detected.ios_targets.len(), 1);
        assert_eq!(detected.android_apps.len(), 1);
        assert_eq!(detected.android_apps[0].target_sdk, Some(35));
    }

    #[test]
    fn outdated_android_target_is_no_go() {
        let project = make_flutter_project(34);
        let report = run_mobile_audit(&project.0, None).unwrap();
        assert_eq!(report.decision, MobileDecision::NoGo);
        assert!(
            report
                .findings
                .iter()
                .any(|f| { f.id == "MOB-AND-001" && f.status == MobileFindingStatus::Fail })
        );
    }

    #[test]
    fn inspects_ipa_identity_hash_and_signature_material() {
        let project = TestProject::new("ipa");
        let ipa = project.0.join("Fixture.ipa");
        let info = ios_info_plist("com.example.fixture");
        write_zip(
            &ipa,
            &[
                ("Payload/Fixture.app/Info.plist", info.as_bytes()),
                (
                    "Payload/Fixture.app/PrivacyInfo.xcprivacy",
                    privacy_manifest().as_bytes(),
                ),
                (
                    "Payload/Fixture.app/_CodeSignature/CodeResources",
                    b"signed",
                ),
                ("Payload/Fixture.app/embedded.mobileprovision", b"profile"),
            ],
        );
        let artifact = inspect_mobile_artifact(&ipa).unwrap();
        assert_eq!(
            artifact.application_id.as_deref(),
            Some("com.example.fixture")
        );
        assert_eq!(artifact.version.as_deref(), Some("1.2.3"));
        assert_eq!(artifact.build.as_deref(), Some("42"));
        assert_eq!(artifact.sha256.len(), 64);
        assert_eq!(artifact.privacy_manifest_present, Some(true));
        assert!(
            artifact
                .signature_material
                .contains("provisioning profile present")
        );
        assert_ne!(
            artifact.signature_verification.status,
            MobileSignatureStatus::Verified,
            "synthetic signature files must never be treated as verified"
        );
    }

    #[test]
    fn inspects_plain_xml_apk_manifest() {
        let project = TestProject::new("apk");
        let apk = project.0.join("fixture.apk");
        let manifest = br#"<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.example.fixture" android:versionCode="42" android:versionName="1.2.3"><uses-sdk android:targetSdkVersion="35"/><uses-permission android:name="android.permission.CAMERA"/><application/></manifest>"#;
        write_zip(
            &apk,
            &[
                ("AndroidManifest.xml", manifest),
                ("META-INF/FIXTURE.RSA", b"certificate"),
            ],
        );
        let artifact = inspect_mobile_artifact(&apk).unwrap();
        assert_eq!(
            artifact.application_id.as_deref(),
            Some("com.example.fixture")
        );
        assert_eq!(artifact.target_sdk, Some(35));
        assert_eq!(artifact.permissions, vec!["android.permission.CAMERA"]);
        assert!(artifact.signature_material.contains("JAR signature"));
        assert_ne!(
            artifact.signature_verification.status,
            MobileSignatureStatus::Verified,
            "synthetic JAR entries must never be treated as verified"
        );
    }

    #[test]
    fn exports_json_markdown_and_html() {
        let project = make_flutter_project(35);
        let report = run_mobile_audit(&project.0, None).unwrap();
        let json = render_mobile_report(&report, MobileReportFormat::Json).unwrap();
        let markdown = render_mobile_report(&report, MobileReportFormat::Markdown).unwrap();
        let html = render_mobile_report(&report, MobileReportFormat::Html).unwrap();
        assert!(json.contains("policy_pack_version"));
        assert!(markdown.contains("# Mobile Release Audit"));
        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("<dl>"));
        assert_eq!(report.schema_version, "2.0");
        assert_eq!(report.project_root, ".");
        assert_eq!(report.report_fingerprint.len(), 64);
        assert_eq!(report.policy_digest.len(), 64);
        let schema: serde_json::Value = serde_json::from_str(MOBILE_REPORT_SCHEMA).unwrap();
        let validator = validator_for(&schema).unwrap();
        let report_value = serde_json::to_value(&report).unwrap();
        let schema_errors: Vec<_> = validator.iter_errors(&report_value).collect();
        assert!(schema_errors.is_empty(), "{schema_errors:?}");
        assert!(report.findings.iter().all(|finding| {
            !finding.subject_id.is_empty()
                && finding.finding_instance_id.starts_with(&finding.rule_id)
        }));
    }

    #[test]
    fn report_fingerprint_excludes_generation_time() {
        let project = make_flutter_project(35);
        let first = run_mobile_audit(&project.0, None).unwrap();
        let mut second = first.clone();
        second.generated_at = "2099-01-01T00:00:00Z".to_string();
        second.report_fingerprint.clear();
        second.report_fingerprint = calculate_mobile_report_fingerprint(&second).unwrap();

        assert_eq!(first.report_fingerprint, second.report_fingerprint);
    }

    #[test]
    fn derived_audit_cache_is_identity_bound_and_bypasses_dirty_source() {
        let project = make_flutter_project(35);
        let artifact = project.0.join("build/app-release.apk");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        let manifest =
            fs::read(project.0.join("android/app/src/main/AndroidManifest.xml")).unwrap();
        write_zip(
            &artifact,
            &[
                ("AndroidManifest.xml", manifest.as_slice()),
                ("META-INF/CERT.RSA", b"synthetic signature fixture"),
            ],
        );
        for args in [
            vec!["init"],
            vec!["config", "user.email", "cache@test.invalid"],
            vec!["config", "user.name", "Cache Test"],
            vec!["add", "."],
            vec!["commit", "-m", "fixture"],
        ] {
            let output = Command::new("git")
                .args(&args)
                .current_dir(&project.0)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let descriptor = describe_mobile_audit_cache(&project.0, &artifact).unwrap();
        assert!(descriptor.eligible);
        let first = run_mobile_audit_cached(&project.0, &artifact).unwrap();
        assert!(!first.cache_hit);
        assert_eq!(first.cache_key, descriptor.cache_key);
        assert!(
            first
                .cache_path
                .as_ref()
                .is_some_and(|path| project.0.join(path).is_file())
        );
        let second = run_mobile_audit_cached(&project.0, &artifact).unwrap();
        assert!(second.cache_hit);
        assert_eq!(second.cache_key, first.cache_key);
        assert_eq!(
            second.report.report_fingerprint,
            first.report.report_fingerprint
        );

        project.write(
            "android/app/src/main/AndroidManifest.xml",
            r#"<manifest><application android:debuggable="true"/></manifest>"#,
        );
        let dirty = run_mobile_audit_cached(&project.0, &artifact).unwrap();
        assert!(!dirty.cache_hit);
        assert!(dirty.cache_key.is_none());
    }

    #[test]
    fn derived_audit_cache_normalizes_monorepo_workspace_paths() {
        let repository = TestProject::new("cache-monorepo");
        repository.write(".gitignore", "apps/mobile/build/\n");
        repository.write(
            "apps/mobile/pubspec.yaml",
            "name: fixture\ndependencies:\n  flutter:\n    sdk: flutter\n",
        );
        repository.write(
            "apps/mobile/android/app/build.gradle.kts",
            r#"android { defaultConfig { applicationId = "dev.trygantry.fixture"; targetSdk = 35 } }"#,
        );
        repository.write(
            "apps/mobile/android/app/src/main/AndroidManifest.xml",
            r#"<manifest><application android:debuggable="false"/></manifest>"#,
        );
        let workspace = repository.0.join("apps/mobile");
        let artifact = workspace.join("build/app-release.apk");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        write_zip(
            &artifact,
            &[
                (
                    "AndroidManifest.xml",
                    br#"<manifest package="dev.trygantry.fixture"><uses-sdk android:targetSdkVersion="35"/><application android:debuggable="false"/></manifest>"#,
                ),
                ("META-INF/CERT.RSA", b"synthetic signature fixture"),
            ],
        );
        for args in [
            vec!["init"],
            vec!["config", "user.email", "cache@test.invalid"],
            vec!["config", "user.name", "Cache Test"],
            vec!["add", "."],
            vec!["commit", "-m", "fixture"],
        ] {
            let output = Command::new("git")
                .args(&args)
                .current_dir(&repository.0)
                .output()
                .unwrap();
            assert!(output.status.success());
        }

        let first = run_mobile_audit_cached(&workspace, &artifact).unwrap();
        assert!(!first.cache_hit);
        let second = run_mobile_audit_cached(&workspace, &artifact).unwrap();
        assert!(second.cache_hit);

        repository.write(
            "apps/mobile/.gantry/releases/REL-GENERATED/release.json",
            "generated",
        );
        assert!(
            describe_mobile_audit_cache(&workspace, &artifact)
                .unwrap()
                .eligible,
            "generated workspace records must not disable the derived cache"
        );

        repository.write("README.md", "dirty outside the mobile workspace");
        assert!(
            describe_mobile_audit_cache(&workspace, &artifact)
                .unwrap()
                .eligible,
            "unrelated monorepo changes outside the workspace must not change its cache identity"
        );

        repository.write(
            "apps/mobile/android/app/src/main/AndroidManifest.xml",
            r#"<manifest><application android:debuggable="true"/></manifest>"#,
        );
        assert!(
            !describe_mobile_audit_cache(&workspace, &artifact)
                .unwrap()
                .eligible,
            "dirty mobile workspace source must still bypass the cache"
        );
    }

    #[test]
    fn policy_validation_rejects_unsafe_or_ambiguous_values() {
        let policy = MobilePolicy {
            minimum_android_target_sdk: 200,
            ..MobilePolicy::default()
        };
        assert!(validate_mobile_policy(&policy).is_err());
        let mut policy = MobilePolicy::default();
        policy
            .required_manual_evidence
            .push("rollback-plan".to_string());
        assert!(validate_mobile_policy(&policy).is_err());
    }

    #[test]
    fn invalid_evidence_timestamp_is_reported_and_cannot_match() {
        let project = make_flutter_project(35);
        project.write(
            ".gantry/evidence/EVI-BAD.json",
            r#"{"id":"EVI-BAD","kind":"approval","title":"release_owner_approval","source":"cli","path":null,"url":null,"artifact_sha":null,"commit":"unknown","environment":null,"release":null,"created_at":"not-a-date","expires_at":"also-not-a-date","metadata":{"mobile_evidence_kind":"release_owner_approval"},"attachments":null}"#,
        );
        let report = run_mobile_audit(&project.0, None).unwrap();
        assert!(report.findings.iter().any(|finding| {
            finding.rule_id == "MOB-EVD-INVALID" && finding.status == MobileFindingStatus::Warning
        }));
        assert!(report.findings.iter().any(|finding| {
            finding.title.contains("release owner approval")
                && finding.status == MobileFindingStatus::Manual
        }));
    }

    #[test]
    fn cancelled_audit_stops_before_publishing_a_report() {
        let project = make_flutter_project(35);
        let cancellation = MobileAuditCancellation::default();
        cancellation.cancel();
        let error = run_mobile_audit_with_cancellation(&project.0, None, &cancellation)
            .expect_err("cancelled audit should stop");
        assert_eq!(error, "Mobile audit cancelled.");
        assert!(!project.0.join(".gantry/mobile/audits").exists());
    }

    #[test]
    fn placeholder_comments_and_reasoned_scoped_ignores_do_not_block() {
        let project = make_flutter_project(35);
        project.write(
            "lib/config.dart",
            "// localhost is used in docs\nconst sample = 'example.com'; // gantry: allow-placeholder(reason=sample fixture)\n",
        );
        let findings = scan_production_placeholders(&project.0, &MobilePolicy::default());
        assert_eq!(findings[0].status, MobileFindingStatus::Pass);
    }

    #[test]
    fn duplicate_rules_have_distinct_finding_instances() {
        let project = make_flutter_project(35);
        project.write(
            "android/wear/build.gradle",
            "plugins { id 'com.android.application' }\nandroid { namespace 'com.example.wear' defaultConfig { applicationId 'com.example.wear' targetSdk 35 } }",
        );
        let report = run_mobile_audit(&project.0, None).unwrap();
        let instances: BTreeSet<_> = report
            .findings
            .iter()
            .filter(|finding| finding.rule_id == "MOB-AND-001")
            .map(|finding| finding.finding_instance_id.clone())
            .collect();
        assert_eq!(instances.len(), 2);
    }

    #[test]
    fn verifies_a_genuinely_signed_jar_artifact_when_jdk_tools_are_available() {
        if !command_exists("keytool") || !command_exists("jarsigner") {
            return;
        }
        let project = TestProject::new("signed-apk");
        let apk = project.0.join("signed.apk");
        let keystore = project.0.join("test.p12");
        let manifest = br#"<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.example.signed" android:versionCode="1" android:versionName="1.0"><uses-sdk android:targetSdkVersion="35"/><application/></manifest>"#;
        write_zip(&apk, &[("AndroidManifest.xml", manifest)]);
        let generated = Command::new("keytool")
            .current_dir(&project.0)
            .args([
                "-genkeypair",
                "-storetype",
                "PKCS12",
                "-storepass",
                "changeit",
                "-keypass",
                "changeit",
                "-alias",
                "gantry",
                "-dname",
                "CN=Gantry Test",
                "-keyalg",
                "RSA",
                "-validity",
                "1",
                "-noprompt",
                "-keystore",
            ])
            .arg(&keystore)
            .output()
            .unwrap();
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );
        let signed = Command::new("jarsigner")
            .current_dir(&project.0)
            .args([
                "-storepass",
                "changeit",
                "-keypass",
                "changeit",
                "-keystore",
            ])
            .arg(&keystore)
            .arg(&apk)
            .arg("gantry")
            .output()
            .unwrap();
        assert!(
            signed.status.success(),
            "{}",
            String::from_utf8_lossy(&signed.stderr)
        );
        let artifact = inspect_mobile_artifact(&apk).unwrap();
        assert_eq!(
            artifact.signature_verification.status,
            MobileSignatureStatus::Verified
        );
    }

    #[test]
    fn corrupt_archives_return_errors_without_panicking() {
        let project = TestProject::new("corrupt");
        for length in [0usize, 1, 7, 31, 128, 1024] {
            let path = project.0.join(format!("corrupt-{length}.apk"));
            let bytes: Vec<u8> = (0..length)
                .map(|index| (index.wrapping_mul(31)) as u8)
                .collect();
            fs::write(&path, bytes).unwrap();
            assert!(inspect_mobile_artifact(&path).is_err());
        }

        let truncated_eocd = project.0.join("truncated-signing-block.apk");
        let mut bytes = vec![0u8; 20];
        bytes[0..4].copy_from_slice(b"PK\x05\x06");
        bytes[16..20].copy_from_slice(&32u32.to_le_bytes());
        fs::write(&truncated_eocd, bytes).unwrap();
        assert!(has_apk_signing_block(&truncated_eocd).is_err());
    }

    #[test]
    fn bounded_reader_rejects_oversized_inputs() {
        let mut reader = std::io::Cursor::new(vec![7u8; 1025]);
        let error = read_limited(&mut reader, 1024, "fixture")
            .expect_err("input above the configured ceiling must fail");
        assert!(error.contains("safety limit"));
    }

    #[test]
    fn representative_artifact_inspection_meets_local_budget() {
        let project = TestProject::new("artifact-budget");
        let apk = project.0.join("representative.apk");
        let manifest = br#"<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.example.budget" android:versionCode="1" android:versionName="1.0"><uses-sdk android:targetSdkVersion="35"/><application/></manifest>"#;
        let mut state = 0x9E37_79B9_u32;
        let filler: Vec<u8> = (0..16 * 1024 * 1024)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        write_zip(
            &apk,
            &[
                ("AndroidManifest.xml", manifest),
                ("assets/release-payload.bin", filler.as_slice()),
                ("META-INF/FIXTURE.RSA", b"synthetic signature fixture"),
            ],
        );

        let started = std::time::Instant::now();
        let artifact = inspect_mobile_artifact(&apk).expect("representative APK should inspect");
        let elapsed = started.elapsed();
        assert!(
            artifact.size_bytes > 15 * 1024 * 1024,
            "representative artifact was unexpectedly small: {} bytes",
            artifact.size_bytes
        );
        assert!(
            elapsed < std::time::Duration::from_secs(30),
            "16 MiB representative artifact inspection exceeded 30 seconds: {elapsed:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_source_directory_is_reported_as_incomplete_coverage() {
        use std::os::unix::fs::PermissionsExt;

        let project = make_flutter_project(35);
        let restricted = project.0.join("private-mobile-source");
        fs::create_dir_all(&restricted).unwrap();
        fs::write(restricted.join("secrets.dart"), "const secret = 'fixture';").unwrap();
        fs::set_permissions(&restricted, fs::Permissions::from_mode(0o000)).unwrap();

        if fs::read_dir(&restricted).is_ok() {
            fs::set_permissions(&restricted, fs::Permissions::from_mode(0o700)).unwrap();
            return;
        }

        let report = run_mobile_audit(&project.0, None);
        fs::set_permissions(&restricted, fs::Permissions::from_mode(0o700)).unwrap();
        let report = report.expect("an unreadable subtree must not crash the audit");
        assert!(report.findings.iter().any(|finding| {
            finding.rule_id == "MOB-PRJ-002"
                && finding.status == MobileFindingStatus::Unavailable
                && finding
                    .observed
                    .as_deref()
                    .is_some_and(|value| value.contains("private-mobile-source"))
        }));
    }
}
