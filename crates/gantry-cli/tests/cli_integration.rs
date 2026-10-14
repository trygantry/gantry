use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::SystemTime;

fn create_temp_dir(name: &str) -> PathBuf {
    let mut temp = std::env::temp_dir();
    temp.push(format!(
        "gantry_cli_test_{}_{}",
        name,
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp).unwrap();
    temp
}

fn run_gantry(args: &[&str], dir: &Path) -> Output {
    let exe = env!("CARGO_BIN_EXE_gantry");
    Command::new(exe)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to execute gantry-cli binary")
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn action_named_script(action: &str, name: &str) -> String {
    let marker = format!("    - name: {name}\n");
    let (_, main_step) = action
        .split_once(&marker)
        .unwrap_or_else(|| panic!("Gantry Action step should exist: {name}"));
    main_step
        .split_once("      run: |\n")
        .map(|(_, body)| {
            body.lines()
                .take_while(|line| line.starts_with("        ") || line.trim().is_empty())
                .map(|line| line.strip_prefix("        ").unwrap_or(line))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| panic!("Gantry Action script should exist: {name}"))
}

fn action_main_script(action: &str) -> String {
    action_named_script(action, "Run Gantry audit and validation")
}

fn parse_action_outputs(body: &str) -> BTreeMap<String, String> {
    body.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn write_android_apk_fixture(root: &Path, artifact: &Path) {
    let manifest = r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.example.fixture"><uses-sdk android:minSdkVersion="24" android:targetSdkVersion="35"/><application android:debuggable="false" android:usesCleartextTraffic="false"/></manifest>"#;
    let source_manifest = root.join("android/app/src/main/AndroidManifest.xml");
    fs::create_dir_all(source_manifest.parent().expect("manifest parent")).unwrap();
    fs::write(&source_manifest, manifest).unwrap();
    fs::write(
        root.join("android/app/build.gradle.kts"),
        r#"plugins { id("com.android.application") }
android { namespace = "com.example.fixture"; compileSdk = 35
defaultConfig { applicationId = "com.example.fixture"; minSdk = 24; targetSdk = 35; versionCode = 1; versionName = "1.0" }
buildTypes { getByName("release") { isDebuggable = false } } }"#,
    )
    .unwrap();
    fs::create_dir_all(artifact.parent().expect("artifact parent")).unwrap();
    let file = fs::File::create(artifact).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    archive.start_file("AndroidManifest.xml", options).unwrap();
    archive.write_all(manifest.as_bytes()).unwrap();
    archive.start_file("META-INF/CERT.RSA", options).unwrap();
    archive.write_all(b"synthetic signature fixture").unwrap();
    archive.finish().unwrap();
}

fn write_ios_ipa_fixture(root: &Path, artifact: &Path) {
    let info = fs::read(root.join("ios/GantryFixture/Info.plist")).unwrap();
    let privacy = fs::read(root.join("ios/GantryFixture/PrivacyInfo.xcprivacy")).unwrap();
    fs::create_dir_all(artifact.parent().expect("artifact parent")).unwrap();
    let file = fs::File::create(artifact).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    for (path, body) in [
        ("Payload/GantryFixture.app/Info.plist", info.as_slice()),
        (
            "Payload/GantryFixture.app/PrivacyInfo.xcprivacy",
            privacy.as_slice(),
        ),
        (
            "Payload/GantryFixture.app/_CodeSignature/CodeResources",
            b"synthetic signature fixture".as_slice(),
        ),
    ] {
        archive.start_file(path, options).unwrap();
        archive.write_all(body).unwrap();
    }
    archive.finish().unwrap();
}

#[test]
fn test_release_workflow_create_bind_audit_and_show_candidate() {
    let temp_dir = create_temp_dir("release_workflow");
    let init = run_gantry(&["init", "--auto"], &temp_dir);
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let created = run_gantry(
        &[
            "release",
            "create",
            "2.7.0",
            "--name",
            "Design partner",
            "--owner",
            "release@example.com",
            "--actor",
            "integration-test",
            "--json",
        ],
        &temp_dir,
    );
    assert!(
        created.status.success(),
        "release create failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    let release: serde_json::Value =
        serde_json::from_slice(&created.stdout).expect("create should return JSON");
    let release_id = release["id"]
        .as_str()
        .expect("release id should be present")
        .to_string();
    assert!(release_id.starts_with("REL-"));
    assert_eq!(release["stage"], "draft");

    let ensured = run_gantry(
        &[
            "release",
            "ensure",
            "2.7.0",
            "--actor",
            "integration-retry",
            "--json",
        ],
        &temp_dir,
    );
    assert!(ensured.status.success());
    let ensured: serde_json::Value =
        serde_json::from_slice(&ensured.stdout).expect("ensure should return JSON");
    assert_eq!(ensured["id"], release_id);

    let listed = run_gantry(&["release", "list", "--json"], &temp_dir);
    assert!(listed.status.success());
    let releases: serde_json::Value =
        serde_json::from_slice(&listed.stdout).expect("list should return JSON");
    assert_eq!(releases.as_array().map(Vec::len), Some(1));
    assert_eq!(releases[0]["id"], release_id);

    let portfolio = run_gantry(&["release", "portfolio", "--json"], &temp_dir);
    assert!(portfolio.status.success());
    let portfolio: serde_json::Value =
        serde_json::from_slice(&portfolio.stdout).expect("portfolio should return JSON");
    assert_eq!(portfolio["counts"]["total"], 1);
    assert_eq!(portfolio["counts"]["needs_attention"], 1);
    assert_eq!(portfolio["releases"][0]["id"], release_id);
    assert!(
        portfolio["review_queue"]
            .as_array()
            .is_some_and(|queue| { queue.iter().any(|item| item["kind"] == "missing_candidate") })
    );

    let artifact = temp_dir.join("build/app-release.apk");
    write_android_apk_fixture(&temp_dir, &artifact);
    let artifact_arg = artifact.to_string_lossy().to_string();
    let candidate = run_gantry(
        &[
            "release",
            "candidate",
            "add",
            &release_id,
            "--artifact",
            &artifact_arg,
            "--actor",
            "ci@example.com",
            "--json",
        ],
        &temp_dir,
    );
    assert!(
        candidate.status.success(),
        "candidate add failed: {}",
        String::from_utf8_lossy(&candidate.stderr)
    );
    let candidate: serde_json::Value =
        serde_json::from_slice(&candidate.stdout).expect("candidate should return JSON");
    assert_eq!(candidate["platform"], "android");
    assert_eq!(candidate["artifact_kind"], "apk");
    assert_eq!(candidate["artifact_path"], "build/app-release.apk");
    assert_eq!(candidate["artifact_path_relative"], true);
    assert_eq!(
        candidate["artifact_sha256"].as_str().map(str::len),
        Some(64)
    );

    let ensured_candidate = run_gantry(
        &[
            "release",
            "candidate",
            "ensure",
            &release_id,
            "--artifact",
            &artifact_arg,
            "--actor",
            "ci-retry@example.com",
            "--json",
        ],
        &temp_dir,
    );
    assert!(ensured_candidate.status.success());
    let ensured_candidate: serde_json::Value = serde_json::from_slice(&ensured_candidate.stdout)
        .expect("candidate ensure should return JSON");
    assert_eq!(ensured_candidate["id"], candidate["id"]);

    let shown = run_gantry(&["release", "show", "2.7.0", "--json"], &temp_dir);
    assert!(
        shown.status.success(),
        "release show failed: {}",
        String::from_utf8_lossy(&shown.stderr)
    );
    let view: serde_json::Value =
        serde_json::from_slice(&shown.stdout).expect("show should return JSON");
    assert_eq!(view["release"]["stage"], "candidate");
    assert_eq!(view["candidates"].as_array().map(Vec::len), Some(1));
    assert_eq!(view["events"].as_array().map(Vec::len), Some(2));
    assert_eq!(view["events"][1]["kind"], "candidate_bound");

    let audited = run_gantry(
        &[
            "release",
            "audit",
            &release_id,
            "--platform",
            "android",
            "--actor",
            "release@example.com",
            "--reuse-unchanged",
            "--ci",
            "--json",
        ],
        &temp_dir,
    );
    assert_eq!(
        audited.status.code(),
        Some(1),
        "synthetic unsigned APK should be No-Go: {}",
        String::from_utf8_lossy(&audited.stderr)
    );
    let audit: serde_json::Value =
        serde_json::from_slice(&audited.stdout).expect("audit should return JSON");
    assert_eq!(audit["audit"]["platform"], "android");
    assert_eq!(audit["audit"]["candidate_id"], candidate["id"]);
    assert_eq!(audit["audit"]["decision"], "no_go");
    let audit_stderr = String::from_utf8_lossy(&audited.stderr);
    assert!(audit_stderr.contains("::error "));
    assert!(audit_stderr.contains("title=MOB-"));
    assert_eq!(
        audit["audit"]["report_fingerprint"],
        audit["report"]["report_fingerprint"]
    );
    let report_path = audit["audit"]["report_path"]
        .as_str()
        .expect("report path should be present");
    assert!(temp_dir.join(".gantry").join(report_path).is_file());

    let retried_audit = run_gantry(
        &[
            "release",
            "audit",
            &release_id,
            "--platform",
            "android",
            "--actor",
            "release-retry@example.com",
            "--reuse-unchanged",
            "--ci",
            "--json",
        ],
        &temp_dir,
    );
    assert_eq!(retried_audit.status.code(), Some(1));
    let retried_audit: serde_json::Value =
        serde_json::from_slice(&retried_audit.stdout).expect("retry audit should return JSON");
    assert_eq!(retried_audit["audit"]["id"], audit["audit"]["id"]);
    let retry_view = run_gantry(&["release", "show", &release_id, "--json"], &temp_dir);
    let retry_view: serde_json::Value =
        serde_json::from_slice(&retry_view.stdout).expect("release should remain valid");
    assert_eq!(retry_view["audits"].as_array().map(Vec::len), Some(1));
    assert_eq!(retry_view["events"].as_array().map(Vec::len), Some(3));

    let approval = run_gantry(
        &[
            "release",
            "approve",
            &release_id,
            "--platform",
            "android",
            "--by",
            "manager@example.com",
            "--role",
            "release_manager",
            "--rationale",
            "Ship the candidate.",
            "--json",
        ],
        &temp_dir,
    );
    assert_eq!(approval.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&approval.stderr).contains("No-Go"));

    let finding_id = audit["report"]["findings"]
        .as_array()
        .expect("findings should be an array")
        .iter()
        .find(|finding| finding["title"] == "Required evidence: release owner approval")
        .and_then(|finding| finding["finding_instance_id"].as_str())
        .expect("release owner finding should exist")
        .to_string();
    let waived_finding_id = audit["report"]["findings"]
        .as_array()
        .expect("findings should be an array")
        .iter()
        .find(|finding| finding["finding_instance_id"] != finding_id && finding["status"] != "pass")
        .and_then(|finding| finding["finding_instance_id"].as_str())
        .expect("a non-pass finding should exist")
        .to_string();
    let evidence = run_gantry(
        &[
            "release",
            "evidence",
            "add",
            &release_id,
            "--platform",
            "android",
            "--finding",
            &finding_id,
            "--kind",
            "release_owner_approval",
            "--title",
            "Production candidate approval",
            "--issuer",
            "captain@example.com",
            "--expires-at",
            "2099-01-01T00:00:00Z",
            "--actor",
            "captain@example.com",
            "--json",
        ],
        &temp_dir,
    );
    assert!(
        evidence.status.success(),
        "release evidence add failed: {}",
        String::from_utf8_lossy(&evidence.stderr)
    );
    let evidence: serde_json::Value =
        serde_json::from_slice(&evidence.stdout).expect("evidence should return JSON");
    let evidence_id = evidence["evidence"]["id"]
        .as_str()
        .expect("evidence id should exist")
        .to_string();
    assert_eq!(evidence["link"]["candidate_id"], candidate["id"]);
    assert_eq!(
        evidence["evidence"]["artifact_sha"],
        candidate["artifact_sha256"]
    );
    assert!(
        temp_dir
            .join(".gantry/evidence")
            .join(format!("{evidence_id}.json"))
            .is_file()
    );

    let resolution = run_gantry(
        &[
            "release",
            "finding",
            "resolve",
            &release_id,
            &finding_id,
            "--platform",
            "android",
            "--reason",
            "Release captain approved the exact production candidate.",
            "--evidence",
            &evidence_id,
            "--actor",
            "captain@example.com",
            "--json",
        ],
        &temp_dir,
    );
    assert!(
        resolution.status.success(),
        "finding resolve failed: {}",
        String::from_utf8_lossy(&resolution.stderr)
    );
    let resolution: serde_json::Value =
        serde_json::from_slice(&resolution.stdout).expect("resolution should return JSON");
    assert_eq!(resolution["finding_instance_id"], finding_id);
    assert_eq!(resolution["evidence_ids"][0], evidence_id);

    let waiver = run_gantry(
        &[
            "release",
            "waive",
            &release_id,
            "--platform",
            "android",
            "--finding",
            &waived_finding_id,
            "--reason",
            "Accepted while the vendor rotates credentials.",
            "--approved-by",
            "security@example.com",
            "--expires-at",
            "2099-01-01T00:00:00Z",
            "--json",
        ],
        &temp_dir,
    );
    assert!(
        waiver.status.success(),
        "finding waiver failed: {}",
        String::from_utf8_lossy(&waiver.stderr)
    );
    let waiver: serde_json::Value =
        serde_json::from_slice(&waiver.stdout).expect("waiver should return JSON");
    assert_eq!(waiver["finding_instance_id"], waived_finding_id);

    let rejection = run_gantry(
        &[
            "release",
            "reject",
            &release_id,
            "--platform",
            "android",
            "--by",
            "manager@example.com",
            "--role",
            "release_manager",
            "--rationale",
            "Blocking findings remain.",
            "--json",
        ],
        &temp_dir,
    );
    assert!(
        rejection.status.success(),
        "release rejection failed: {}",
        String::from_utf8_lossy(&rejection.stderr)
    );
    let rejection: serde_json::Value =
        serde_json::from_slice(&rejection.stdout).expect("rejection should return JSON");
    assert_eq!(rejection["decision"], "rejected");

    let rejected_view = run_gantry(&["release", "show", &release_id, "--json"], &temp_dir);
    assert!(rejected_view.status.success());
    let rejected_view: serde_json::Value =
        serde_json::from_slice(&rejected_view.stdout).expect("rejected view should return JSON");
    assert_eq!(rejected_view["release"]["stage"], "rejected");
    assert_eq!(
        rejected_view["release"]["active_waiver_ids"][0],
        waiver["id"]
    );

    let packet = run_gantry(&["release", "packet", &release_id, "--json"], &temp_dir);
    assert!(
        packet.status.success(),
        "release packet failed: {}",
        String::from_utf8_lossy(&packet.stderr)
    );
    let packet: serde_json::Value =
        serde_json::from_slice(&packet.stdout).expect("packet should return JSON");
    let packet_id = packet["packet"]["packet_id"]
        .as_str()
        .expect("packet ID should exist")
        .to_string();
    assert_eq!(packet["packet"]["release"]["stage"], "rejected");
    assert!(packet["handoff_summary"].as_str().is_some_and(
        |summary| summary.contains("BLOCKED") && summary.contains("release@example.com")
    ));
    for key in ["json_path", "markdown_path", "html_path"] {
        let path = packet[key].as_str().expect("packet path should exist");
        assert!(temp_dir.join(".gantry").join(path).is_file());
    }
    let repeated_packet = run_gantry(&["release", "packet", &release_id, "--json"], &temp_dir);
    assert!(repeated_packet.status.success());
    let repeated_packet: serde_json::Value = serde_json::from_slice(&repeated_packet.stdout)
        .expect("repeated packet should return JSON");
    assert_eq!(repeated_packet["packet"]["packet_id"], packet_id);

    let refreshed = run_gantry(
        &[
            "release",
            "audit",
            &release_id,
            "--platform",
            "android",
            "--actor",
            "release@example.com",
            "--json",
        ],
        &temp_dir,
    );
    assert_eq!(refreshed.status.code(), Some(1));
    let refreshed: serde_json::Value =
        serde_json::from_slice(&refreshed.stdout).expect("refreshed audit should return JSON");
    assert!(
        refreshed["report"]["findings"]
            .as_array()
            .expect("findings should be an array")
            .iter()
            .any(|finding| {
                finding["finding_instance_id"] == finding_id && finding["status"] == "pass"
            })
    );

    let shown = run_gantry(&["release", "show", &release_id, "--json"], &temp_dir);
    assert!(shown.status.success());
    let view: serde_json::Value =
        serde_json::from_slice(&shown.stdout).expect("audited release should return JSON");
    assert_eq!(view["release"]["stage"], "in_review");
    assert_eq!(view["audits"].as_array().map(Vec::len), Some(2));
    assert_eq!(view["evidence_links"].as_array().map(Vec::len), Some(1));
    assert_eq!(view["resolutions"].as_array().map(Vec::len), Some(1));
    assert_eq!(view["decisions"].as_array().map(Vec::len), Some(1));
    assert_eq!(view["waivers"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        view["release"]["active_decisions"]
            .as_object()
            .map(serde_json::Map::len),
        Some(0)
    );
    assert_eq!(
        view["release"]["active_waiver_ids"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    assert_eq!(view["events"].as_array().map(Vec::len), Some(8));
    assert_eq!(view["events"][2]["kind"], "candidate_audited");
    assert_eq!(view["events"][3]["kind"], "evidence_linked");
    assert_eq!(view["events"][4]["kind"], "finding_resolved");
    assert_eq!(view["events"][5]["kind"], "finding_waived");
    assert_eq!(view["events"][6]["kind"], "release_rejected");
    assert_eq!(view["events"][7]["kind"], "candidate_audited");
    assert_eq!(
        view["events"][7]["details"]["invalidated_decision_id"],
        rejection["id"]
    );
    assert_eq!(
        view["events"][7]["details"]["invalidated_waiver_ids"][0],
        waiver["id"]
    );

    let refreshed_packet = run_gantry(&["release", "packet", &release_id, "--json"], &temp_dir);
    assert!(refreshed_packet.status.success());
    let refreshed_packet: serde_json::Value = serde_json::from_slice(&refreshed_packet.stdout)
        .expect("refreshed packet should return JSON");
    assert_ne!(refreshed_packet["packet"]["packet_id"], packet_id);
    assert_eq!(refreshed_packet["packet"]["release"]["stage"], "in_review");

    let replacement_artifact = temp_dir.join("build/app-release-2.apk");
    write_android_apk_fixture(&temp_dir, &replacement_artifact);
    let replacement_artifact_arg = replacement_artifact.to_string_lossy().to_string();
    let replacement = run_gantry(
        &[
            "release",
            "candidate",
            "add",
            &release_id,
            "--artifact",
            &replacement_artifact_arg,
            "--json",
        ],
        &temp_dir,
    );
    assert!(replacement.status.success());
    let replacement: serde_json::Value = serde_json::from_slice(&replacement.stdout)
        .expect("replacement candidate should return JSON");
    let diff = run_gantry(
        &[
            "release",
            "diff",
            &release_id,
            "--platform",
            "android",
            "--json",
        ],
        &temp_dir,
    );
    assert!(
        diff.status.success(),
        "release diff failed: {}",
        String::from_utf8_lossy(&diff.stderr)
    );
    let diff: serde_json::Value =
        serde_json::from_slice(&diff.stdout).expect("diff should return JSON");
    assert_eq!(diff["from_candidate"]["id"], candidate["id"]);
    assert_eq!(diff["to_candidate"]["id"], replacement["id"]);
    assert!(diff["to_audit"].is_null());
    assert_eq!(diff["resolved_findings"].as_array().map(Vec::len), Some(0));

    let event_log = temp_dir
        .join(".gantry/releases")
        .join(&release_id)
        .join("events.jsonl");
    assert!(event_log.is_file());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_release_schedule_is_durable_and_visible_in_portfolio() {
    let temp_dir = create_temp_dir("release_schedule");
    assert!(run_gantry(&["init", "--auto"], &temp_dir).status.success());
    assert!(
        run_gantry(
            &[
                "release",
                "create",
                "3.1.0",
                "--owner",
                "release@example.com",
                "--actor",
                "integration-test",
            ],
            &temp_dir,
        )
        .status
        .success()
    );
    let scheduled = run_gantry(
        &[
            "release",
            "schedule",
            "3.1.0",
            "--target-date",
            "2099-07-20",
            "--submission-date",
            "2099-07-18",
            "--actor",
            "release@example.com",
            "--json",
        ],
        &temp_dir,
    );
    assert!(
        scheduled.status.success(),
        "release schedule failed: {}",
        String::from_utf8_lossy(&scheduled.stderr)
    );
    let release: serde_json::Value = serde_json::from_slice(&scheduled.stdout).unwrap();
    assert_eq!(release["target_date"], "2099-07-20");
    assert_eq!(release["submission_date"], "2099-07-18");

    let shown = run_gantry(&["release", "show", "3.1.0", "--json"], &temp_dir);
    let view: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(view["events"][1]["kind"], "release_scheduled");
    let portfolio = run_gantry(&["release", "portfolio", "--json"], &temp_dir);
    let portfolio: serde_json::Value = serde_json::from_slice(&portfolio.stdout).unwrap();
    assert_eq!(portfolio["releases"][0]["target_date"], "2099-07-20");
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_vault_health_migration_and_redacted_support_bundle() {
    let temp_dir = create_temp_dir("vault_operations");
    assert!(run_gantry(&["init", "--auto"], &temp_dir).status.success());
    assert!(
        run_gantry(
            &[
                "release",
                "create",
                "4.0.0",
                "--owner",
                "private-owner@example.test",
                "--json",
            ],
            &temp_dir,
        )
        .status
        .success()
    );

    let health = run_gantry(&["vault", "check", "--json"], &temp_dir);
    assert!(health.status.success());
    let health: serde_json::Value = serde_json::from_slice(&health.stdout).unwrap();
    assert_eq!(health["healthy"], true);
    assert_eq!(health["verified_releases"], 1);

    let migration = run_gantry(&["vault", "migrate", "--dry-run", "--json"], &temp_dir);
    assert!(migration.status.success());
    let migration: serde_json::Value = serde_json::from_slice(&migration.stdout).unwrap();
    assert_eq!(migration["migrations_required"], 0);

    let bundle_path = temp_dir.join("support.json");
    let bundle = run_gantry(
        &[
            "vault",
            "support-bundle",
            "--output",
            bundle_path.to_str().unwrap(),
            "--json",
        ],
        &temp_dir,
    );
    assert!(bundle.status.success());
    let body = fs::read_to_string(&bundle_path).unwrap();
    assert!(!body.contains("private-owner@example.test"));
    assert!(!body.contains(temp_dir.to_string_lossy().as_ref()));
    assert!(body.contains("No source file contents or names are included."));
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_help_starts_with_compact_product_header() {
    let temp_dir = create_temp_dir("help_logo");
    let out = run_gantry(&["--help", "--no-color"], &temp_dir);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let header: Vec<&str> = stdout.lines().take(3).collect();

    assert!(out.status.success());
    assert_eq!(
        header,
        [
            "GANTRY  /  MOBILE RELEASE GOVERNANCE",
            "Local evidence. Exact candidates. Accountable decisions.",
            concat!(
                "v",
                env!("CARGO_PKG_VERSION"),
                "  ·  https://github.com/trygantry/gantry"
            ),
        ]
    );
    assert!(stdout.contains("QUICK START"));
    assert!(!stdout.contains('\u{1b}'));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_loop_init_validate_index_summarize_audit() {
    let temp_dir = create_temp_dir("loop_test");

    // 1. init --auto
    let out = run_gantry(&["init", "--auto"], &temp_dir);
    assert!(
        out.status.success(),
        "init --auto failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(temp_dir.join(".gantry").exists());
    assert!(temp_dir.join(".gantry/config.json").exists());

    // 2. validate -> should fail (exit 1) because default template has open P0 blocker
    let out = run_gantry(&["validate"], &temp_dir);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit status 1 for blocked launch"
    );

    // 3. index -> should succeed (exit 0) and generate index.json
    let out = run_gantry(&["index"], &temp_dir);
    assert!(
        out.status.success(),
        "index failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(temp_dir.join(".gantry/index.json").exists());

    // 4. summarize -> should succeed (exit 0)
    let out = run_gantry(&["summarize"], &temp_dir);
    assert!(
        out.status.success(),
        "summarize failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 5. audit -> should succeed (exit 0)
    let out = run_gantry(&["audit"], &temp_dir);
    assert!(
        out.status.success(),
        "audit failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Cleanup temp dir
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_scenario_clean_vault_go() {
    let temp_dir = create_temp_dir("clean_go");

    // Initialize vault
    let out = run_gantry(&["init", "--auto"], &temp_dir);
    assert!(out.status.success());

    // Write a clean config and go/no-go setup to produce a "Go" readiness
    let config_content = r#"{
        "vaultName": "Clean Vault",
        "version": "1.0.0",
        "releaseDate": "2026-06-15",
        "environments": {
            "production": {
                "requiredQaCompletion": 100,
                "blockOnP0": true,
                "requireSignoffs": false,
                "blockOnMissingTelemetry": false
            }
        }
    }"#;
    fs::write(temp_dir.join(".gantry/config.json"), config_content).unwrap();

    let go_no_go_content = r#"---
type: go_no_go
release: 1.0.0
target_date: 2026-06-15
status: go
---
# Gates
- [x] **Product:** Approved. <!-- @pm_dan 2026-06-01 -->"#;
    fs::write(temp_dir.join(".gantry/go_no_go.md"), go_no_go_content).unwrap();

    let qa_content = r#"---
type: qa
id: QA-REG
title: Regression
owner: "@qa"
last_verified: 2026-06-01
---
- [x] Test 1
- [x] Test 2"#;
    fs::write(temp_dir.join(".gantry/qa/regression.md"), qa_content).unwrap();

    // Remove the default template blocker to ensure zero blockers
    let _ = fs::remove_file(temp_dir.join(".gantry/blockers/BLK-101-purchase-crash.md"));

    // Write recommended files so readiness is Go (not At Risk due to missing documents)
    fs::write(temp_dir.join(".gantry/release-notes.md"), "# Release Notes").unwrap();
    fs::write(
        temp_dir.join(".gantry/engineering-notes.md"),
        "# Engineering Notes",
    )
    .unwrap();
    fs::write(
        temp_dir.join(".gantry/runbook.md"),
        "---\ntype: runbook\n---\n# Runbook",
    )
    .unwrap();
    fs::write(temp_dir.join(".gantry/postlaunch.md"), "# Post Launch").unwrap();

    // Run validate --env production
    let out = run_gantry(&["validate", "--env", "production"], &temp_dir);
    assert!(
        out.status.success(),
        "expected validation to pass, but got code {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Cleanup temp dir
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_scenario_p0_blocker_returns_nogo() {
    let temp_dir = create_temp_dir("p0_blocker");

    // Initialize vault
    let out = run_gantry(&["init", "--auto"], &temp_dir);
    assert!(out.status.success());

    // Write a config where we block on P0, require 0% QA
    let config_content = r#"{
        "vaultName": "Blocked Vault",
        "version": "1.0.0",
        "releaseDate": "2026-06-15",
        "environments": {
            "production": {
                "requiredQaCompletion": 0,
                "blockOnP0": true,
                "requireSignoffs": false,
                "blockOnMissingTelemetry": false
            }
        }
    }"#;
    fs::write(temp_dir.join(".gantry/config.json"), config_content).unwrap();

    // Write a P0 blocker
    let blocker_content = r#"---
type: blocker
id: BLK-CRASH
title: App crashes on startup
severity: P0
status: blocked
owner: "@dev"
source_files:
  - src/main.rs
last_verified: 2026-06-01
---
Details here."#;
    fs::write(
        temp_dir.join(".gantry/blockers/BLK-CRASH.md"),
        blocker_content,
    )
    .unwrap();

    // Run validate --env production -> should exit with status 1
    let out = run_gantry(&["validate", "--env", "production"], &temp_dir);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected validation to fail with 1 (No-Go)"
    );

    // Cleanup temp dir
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_scenario_malformed_vault_returns_error() {
    let temp_dir = create_temp_dir("malformed_config");

    // Initialize vault
    let out = run_gantry(&["init", "--auto"], &temp_dir);
    assert!(out.status.success());

    // Overwrite config with malformed JSON
    fs::write(
        temp_dir.join(".gantry/config.json"),
        "{ this is not valid json }}}",
    )
    .unwrap();

    // Run validate -> should fail with exit status 2
    let out = run_gantry(&["validate"], &temp_dir);
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit status 2 on malformed config"
    );

    // Run index -> should also fail with exit status 2
    let out = run_gantry(&["index"], &temp_dir);
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit status 2 on malformed config index attempt"
    );

    // Cleanup temp dir
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_scenario_waiver_evidence_scorecard_survive() {
    let temp_dir = create_temp_dir("waiver_evidence_scorecard");

    // Initialize vault
    let out = run_gantry(&["init", "--auto"], &temp_dir);
    assert!(out.status.success());

    // Clean config
    let config_content = r#"{
        "vaultName": "Full Vault",
        "version": "1.0.0",
        "releaseDate": "2026-06-15",
        "environments": {
            "production": {
                "requiredQaCompletion": 0,
                "blockOnP0": true,
                "requireSignoffs": false,
                "blockOnMissingTelemetry": false
            }
        }
    }"#;
    fs::write(temp_dir.join(".gantry/config.json"), config_content).unwrap();

    // Add a waiver
    let waiver_content = r#"[{
        "id": "WAV-TEST-001",
        "target_type": "blocker",
        "target_id": "BLK-KNOWN",
        "target_path": "blockers/BLK-KNOWN.md",
        "target_code": null,
        "reason": "Known issue, workaround deployed.",
        "approved_by": "@release_lead",
        "created_at": "2026-06-01",
        "expires_at": "2026-12-31",
        "status": "active"
    }]"#;
    fs::write(temp_dir.join(".gantry/waivers.json"), waiver_content).unwrap();

    // Add evidence
    let evidence_content = r#"{
        "id": "EVI-001",
        "kind": "test_report",
        "title": "Regression run",
        "source": "ci",
        "path": null,
        "url": null,
        "artifact_sha": null,
        "commit": "abc123",
        "environment": "production",
        "release": "1.0.0",
        "created_at": "2026-06-01T10:00:00Z",
        "expires_at": null,
        "metadata": {},
        "attachments": []
    }"#;
    fs::create_dir_all(temp_dir.join(".gantry/evidence")).unwrap();
    fs::write(
        temp_dir.join(".gantry/evidence/EVI-001.json"),
        evidence_content,
    )
    .unwrap();

    // Add scorecard
    let scorecard_content = r#"[{
        "id": "qa-complete",
        "category": "QA",
        "title": "All QA tests pass",
        "severity": "warning",
        "weight": 40,
        "predicate": "min_qa_completion",
        "params": { "min_pct": 100 },
        "failure_message": "QA is incomplete.",
        "suggested_action": "Complete QA."
    }]"#;
    fs::write(temp_dir.join(".gantry/scorecard.json"), scorecard_content).unwrap();

    // Run index -> should succeed
    let out = run_gantry(&["index", "--env", "production"], &temp_dir);
    assert!(out.status.success());

    // Read index.json and assert fields
    let index_str = fs::read_to_string(temp_dir.join(".gantry/index.json")).unwrap();
    let index: serde_json::Value = serde_json::from_str(&index_str).unwrap();

    // Assert that active_waivers has WAV-TEST-001
    let active_waivers = index.get("active_waivers").unwrap().as_array().unwrap();
    assert_eq!(active_waivers.len(), 1);
    assert_eq!(
        active_waivers[0].get("id").unwrap().as_str(),
        Some("WAV-TEST-001")
    );

    // Assert that evidence_records has EVI-001
    let evidence_records = index.get("evidence_records").unwrap().as_array().unwrap();
    assert_eq!(evidence_records.len(), 1);
    assert_eq!(
        evidence_records[0].get("id").unwrap().as_str(),
        Some("EVI-001")
    );

    // Assert that scorecard_results has rule qa-complete
    let scorecard_results = index.get("scorecard_results").unwrap().as_array().unwrap();
    assert!(!scorecard_results.is_empty());
    assert_eq!(
        scorecard_results[0].get("rule_id").unwrap().as_str(),
        Some("qa-complete")
    );

    // Cleanup temp dir
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_scenario_init_dry_run_and_quiet() {
    let temp_dir = create_temp_dir("init_dry_run");

    // Test 1: init --template saas-app --dry-run -> should not create .gantry/
    let out = run_gantry(&["init", "--template", "saas-app", "--dry-run"], &temp_dir);
    assert!(out.status.success());
    assert!(!temp_dir.join(".gantry").exists());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Dry-run mode: Selected template is 'saas-app'."));
    assert!(stdout.contains("config.json"));

    // Test 2: init --template saas-app --dry-run --json -> should return JSON report
    let out = run_gantry(
        &["init", "--template", "saas-app", "--dry-run", "--json"],
        &temp_dir,
    );
    assert!(out.status.success());
    assert!(!temp_dir.join(".gantry").exists());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json_val: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json_val.get("template").unwrap().as_str(), Some("saas-app"));
    assert!(json_val.get("files_to_create").unwrap().is_array());

    // Test 3: --quiet init --template saas-app --dry-run -> should be silent
    let out = run_gantry(
        &["--quiet", "init", "--template", "saas-app", "--dry-run"],
        &temp_dir,
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty());

    // Test 4: --quiet init --auto --dry-run -> should be silent
    let out = run_gantry(&["--quiet", "init", "--auto", "--dry-run"], &temp_dir);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());

    // Test 5: --quiet init --template saas-app --dry-run --json -> should output JSON report
    let out = run_gantry(
        &[
            "--quiet",
            "init",
            "--template",
            "saas-app",
            "--dry-run",
            "--json",
        ],
        &temp_dir,
    );
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json_val: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json_val.get("template").unwrap().as_str(), Some("saas-app"));

    // Test 6: --quiet init --auto --dry-run --json -> should output JSON report
    let out = run_gantry(
        &["--quiet", "init", "--auto", "--dry-run", "--json"],
        &temp_dir,
    );
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json_val: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(json_val.get("detected_project_type").is_some());

    // Cleanup temp dir
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_scenario_release_manifest_json() {
    let temp_dir = create_temp_dir("release_manifest_json");

    // Initialize vault
    let out = run_gantry(&["init", "--auto"], &temp_dir);
    assert!(out.status.success());

    // Run index to generate releases directory and manifest
    let out = run_gantry(&["index"], &temp_dir);
    assert!(out.status.success());

    // Test release manifest --json -> should print JSON manifest
    let out = run_gantry(&["release", "manifest", "--json"], &temp_dir);
    assert!(
        out.status.success(),
        "release manifest --json failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json_val: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(json_val.get("release").is_some());
    assert!(json_val.get("commit").is_some());

    // Cleanup temp dir
    let _ = fs::remove_dir_all(&temp_dir);
}

fn write_mobile_fixture(root: &Path, target_sdk: u32) {
    fs::write(
        root.join("pubspec.yaml"),
        "name: fixture\ndependencies:\n  flutter:\n    sdk: flutter\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("android/app/src/main")).unwrap();
    fs::write(
        root.join("android/app/build.gradle.kts"),
        format!(
            r#"plugins {{ id("com.android.application") }}
android {{
  namespace = "com.example.fixture"
  defaultConfig {{
    applicationId = "com.example.fixture"
    targetSdk = {target_sdk}
    versionCode = 1
    versionName = "1.0.0"
  }}
  buildTypes {{ release {{ signingConfig = signingConfigs.getByName("release") }} }}
}}"#
        ),
    )
    .unwrap();
    fs::write(
        root.join("android/app/src/main/AndroidManifest.xml"),
        r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android"><application android:debuggable="false" android:usesCleartextTraffic="false"/></manifest>"#,
    )
    .unwrap();
}

#[test]
fn test_mobile_detect_and_init_work_without_existing_vault() {
    let temp_dir = create_temp_dir("mobile_detect");
    write_mobile_fixture(&temp_dir, 35);

    let out = run_gantry(&["mobile", "detect", "--json"], &temp_dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let detected: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(detected["stacks"][0], "flutter");
    assert_eq!(detected["platforms"][0], "android");

    let out = run_gantry(&["mobile", "init", "--json"], &temp_dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(temp_dir.join(".gantry/mobile-policy.json").is_file());
    assert!(
        temp_dir
            .join(".gantry/schema/mobile-policy.schema.json")
            .is_file()
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_mobile_audit_returns_one_for_policy_blocker() {
    let temp_dir = create_temp_dir("mobile_nogo");
    write_mobile_fixture(&temp_dir, 34);

    let out = run_gantry(&["mobile", "audit", "--json", "--ci"], &temp_dir);
    assert_eq!(out.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["schema_version"], "2.0");
    assert_eq!(report["report"]["decision"], "no_go");
    assert_eq!(report["report"]["summary"]["blockers"], 1);
    assert!(report["report_path"].is_null());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_init_mobile_creates_vault_and_mobile_policy() {
    let temp_dir = create_temp_dir("init_mobile");
    write_mobile_fixture(&temp_dir, 35);

    let out = run_gantry(&["init", "--mobile", "--json"], &temp_dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(temp_dir.join(".gantry/config.json").is_file());
    assert!(temp_dir.join(".gantry/mobile-policy.json").is_file());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_canonical_action_has_one_mobile_audit_and_valid_bash() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let action = fs::read_to_string(root.join("action.yml")).unwrap();
    assert!(
        !root
            .join(".github/actions/gantry-validate/action.yml")
            .exists()
    );
    assert_eq!(action.matches("mobile_args=(mobile audit").count(), 1);
    assert_eq!(action.matches("release candidate ensure").count(), 1);
    assert_eq!(action.matches("--reuse-unchanged --ci").count(), 1);
    assert!(action.contains("if (( mobile_status != 0 )); then"));
    assert!(
        action.contains("actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4")
    );
    assert!(action.contains("packet-json-path"));
    assert!(action.contains("duration-ms"));
    assert!(action.contains("performance-budget-met"));
    assert!(action.contains("INPUT_POLICY"));
    assert!(action.contains("updateIssueComment"));
    assert!(!action.contains("--edit-comment"));
    let script = action_main_script(&action);
    let status = Command::new("bash")
        .args(["-n", "-c", &script])
        .status()
        .unwrap();
    assert!(status.success());

    let performance = action_named_script(&action, "Publish Gantry performance evidence");
    let output_path = create_temp_dir("action_performance").join("output.txt");
    let summary_path = output_path.with_file_name("summary.md");
    let started_ms = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .saturating_sub(10)
        .to_string();
    let performance_run = Command::new("bash")
        .args(["-c", &performance])
        .env("STARTED_MS", started_ms)
        .env("MOBILE_CACHE_HIT", "true")
        .env("GITHUB_OUTPUT", &output_path)
        .env("GITHUB_STEP_SUMMARY", &summary_path)
        .output()
        .unwrap();
    assert!(
        performance_run.status.success(),
        "performance evidence failed: {}",
        String::from_utf8_lossy(&performance_run.stderr)
    );
    let performance_output = fs::read_to_string(output_path).unwrap();
    assert!(performance_output.contains("duration-ms="));
    assert!(performance_output.contains("performance-budget-met=true"));
    assert!(
        fs::read_to_string(summary_path)
            .unwrap()
            .contains("Ten-minute onboarding budget")
    );
}

#[test]
fn test_branch_coverage_uses_a_pinned_nightly_toolchain() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let coverage = ci
        .split_once("  coverage:")
        .unwrap()
        .1
        .split_once("  msrv:")
        .unwrap()
        .0;

    assert!(coverage.contains("toolchain: nightly-2026-08-01"));
    assert!(coverage.contains("components: llvm-tools-preview"));
    assert!(coverage.contains("--branch"));
}

#[test]
fn test_action_persistent_release_retry_is_idempotent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let action = fs::read_to_string(root.join("action.yml")).unwrap();
    let script = action_main_script(&action)
        .replace(
            "${{ steps.install.outputs.bin }}",
            env!("CARGO_BIN_EXE_gantry"),
        )
        .replace("${{ github.event.pull_request.number }}", "0");

    let temp_dir = create_temp_dir("action_persistent");
    let artifact = temp_dir.join("build/app-release.apk");
    write_android_apk_fixture(&temp_dir, &artifact);
    let init = run_gantry(&["init", "--auto"], &temp_dir);
    assert!(init.status.success());
    for args in [
        vec!["init"],
        vec!["config", "user.email", "action@test.invalid"],
        vec!["config", "user.name", "Action Test"],
        vec!["add", "."],
        vec!["commit", "-m", "fixture"],
    ] {
        let git = Command::new("git")
            .args(&args)
            .current_dir(&temp_dir)
            .output()
            .unwrap();
        assert!(git.status.success());
    }
    let output_path = std::env::temp_dir().join(format!(
        "gantry-action-output-{}.txt",
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let run_action = || {
        fs::write(&output_path, "").unwrap();
        Command::new("bash")
            .args(["-c", &script])
            .current_dir(&temp_dir)
            .env("GITHUB_EVENT_NAME", "push")
            .env("GITHUB_ACTOR", "integration-test")
            .env("GITHUB_OUTPUT", &output_path)
            .env("INPUT_ENVIRONMENT", "")
            .env("INPUT_COMMENT", "false")
            .env("INPUT_FAIL_ON_NO_GO", "false")
            .env("INPUT_POLICY", "")
            .env("INPUT_MOBILE_AUDIT", "false")
            .env("INPUT_MOBILE_ARTIFACT", "build/app-release.apk")
            .env("INPUT_WORKSPACE", ".")
            .env("INPUT_RELEASE_VERSION", "3.2.1")
            .env("INPUT_RELEASE_NAME", "Action candidate")
            .env("INPUT_RELEASE_OWNER", "release@example.com")
            .env("INPUT_RELEASE_ACTOR", "github-actions:integration-test")
            .output()
            .expect("action script should execute")
    };

    let first = run_action();
    assert!(
        first.status.success(),
        "action failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        String::from_utf8_lossy(&first.stderr).contains("::error "),
        "expected stable blocker annotation: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_outputs = fs::read_to_string(&output_path).unwrap();
    let first_values = parse_action_outputs(&first_outputs);
    assert!(first_outputs.contains("release-id=REL-"));
    assert!(first_outputs.contains("candidate-id=CAN-"));
    assert!(first_outputs.contains("audit-id=AUD-"));
    assert!(first_outputs.contains("decision=no_go"));
    assert!(first_outputs.contains("packet-id=PKT-"));
    assert_eq!(
        first_values.get("cache-hit").map(String::as_str),
        Some("false")
    );
    assert_eq!(first_values.get("cache-key").map(String::len), Some(64));
    for line in first_outputs
        .lines()
        .filter(|line| line.starts_with("packet-") && line.contains("-path="))
    {
        let (_, path) = line.split_once('=').unwrap();
        assert!(temp_dir.join(path).is_file(), "missing packet file {path}");
    }

    let second = run_action();
    assert!(
        second.status.success(),
        "retry failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_outputs = fs::read_to_string(&output_path).unwrap();
    let second_values = parse_action_outputs(&second_outputs);
    assert_eq!(
        second_values.get("cache-hit").map(String::as_str),
        Some("true")
    );
    for key in [
        "release-id",
        "candidate-id",
        "audit-id",
        "decision",
        "report-fingerprint",
        "cache-key",
        "packet-id",
        "packet-json-path",
        "packet-markdown-path",
        "packet-html-path",
    ] {
        assert_eq!(
            second_values.get(key),
            first_values.get(key),
            "changed {key}"
        );
    }
    let shown = run_gantry(&["release", "show", "3.2.1", "--json"], &temp_dir);
    let view: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(view["candidates"].as_array().map(Vec::len), Some(1));
    assert_eq!(view["audits"].as_array().map(Vec::len), Some(1));
    assert_eq!(view["events"].as_array().map(Vec::len), Some(3));
    let _ = fs::remove_file(output_path);
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_shell_completions_cover_supported_operator_shells() {
    let temp_dir = create_temp_dir("completions");
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let output = run_gantry(&["completions", shell], &temp_dir);
        assert!(
            output.status.success(),
            "{shell} completion failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let body = String::from_utf8_lossy(&output.stdout);
        assert!(
            body.len() > 100,
            "{shell} completion was unexpectedly empty"
        );
        assert!(body.to_ascii_lowercase().contains("gantry"));
    }
    let unsupported = run_gantry(&["completions", "cmd"], &temp_dir);
    assert_eq!(unsupported.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unsupported.stderr).contains("Unsupported shell"));
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_published_schema_manifest_is_complete_and_consistent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema_root = root.join("schemas");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(schema_root.join("schema-manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["schema_version"], "1.0");
    assert_eq!(manifest["compatibility"], "exact_version_fail_closed");
    let contracts = manifest["contracts"].as_array().unwrap();
    let mut paths = BTreeSet::new();
    for contract in contracts {
        let path = contract["path"].as_str().unwrap();
        assert!(paths.insert(path), "duplicate schema manifest path {path}");
        let schema: serde_json::Value =
            serde_json::from_slice(&fs::read(schema_root.join(path)).unwrap()).unwrap();
        assert!(schema["$id"].as_str().is_some_and(|id| {
            id.starts_with("https://trygantry.dev/schemas/") && id.ends_with(".json")
        }));
        assert_eq!(
            schema["properties"]["schema_version"]["const"], contract["current_version"],
            "version mismatch for {path}"
        );
    }
    assert!(paths.contains("mobile-audit-cache.schema.json"));
    assert!(paths.contains("release-packet.schema.json"));
    assert!(paths.contains("release-portfolio.schema.json"));
    assert_eq!(paths.len(), contracts.len());
}

#[test]
fn test_flutter_android_sample_fixture_matches_public_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = root.join("fixtures/mobile/flutter-android-minimal");
    let temp_dir = create_temp_dir("public_mobile_fixture");
    copy_tree(&source, &temp_dir);

    let detected = run_gantry(&["mobile", "detect", "--json"], &temp_dir);
    assert!(detected.status.success());
    let detected: serde_json::Value = serde_json::from_slice(&detected.stdout).unwrap();
    assert!(
        detected["stacks"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("flutter"))
    );
    assert!(
        detected["platforms"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("android"))
    );
    assert_eq!(detected["android_apps"][0]["target_sdk"], 35);

    let manifest = fs::read(temp_dir.join("android/app/src/main/AndroidManifest.xml")).unwrap();
    let artifact = temp_dir.join("build/app-release.apk");
    fs::create_dir_all(artifact.parent().unwrap()).unwrap();
    let file = fs::File::create(&artifact).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    archive.start_file("AndroidManifest.xml", options).unwrap();
    archive.write_all(&manifest).unwrap();
    archive.start_file("META-INF/CERT.RSA", options).unwrap();
    archive.write_all(b"synthetic signature fixture").unwrap();
    archive.finish().unwrap();

    let audited = run_gantry(
        &[
            "mobile",
            "audit",
            "--artifact",
            "build/app-release.apk",
            "--json",
        ],
        &temp_dir,
    );
    assert!(matches!(audited.status.code(), Some(0 | 1)));
    let audited: serde_json::Value = serde_json::from_slice(&audited.stdout).unwrap();
    assert_eq!(audited["report"]["schema_version"], "2.0");
    assert_eq!(audited["report"]["artifact"]["platform"], "android");
    assert_eq!(
        audited["report"]["artifact"]["sha256"]
            .as_str()
            .map(str::len),
        Some(64)
    );
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_swift_ios_sample_fixture_matches_public_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = root.join("fixtures/mobile/swift-ios-minimal");
    let temp_dir = create_temp_dir("public_ios_fixture");
    copy_tree(&source, &temp_dir);

    let detected = run_gantry(&["mobile", "detect", "--json"], &temp_dir);
    assert!(detected.status.success());
    let detected: serde_json::Value = serde_json::from_slice(&detected.stdout).unwrap();
    assert!(
        detected["stacks"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("native_ios"))
    );
    assert!(
        detected["platforms"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("ios"))
    );
    assert_eq!(detected["ios_targets"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        detected["ios_targets"][0]["bundle_identifier"],
        "dev.trygantry.contract"
    );
    assert_eq!(detected["ios_targets"][0]["minimum_os_version"], "16.0");

    let artifact = temp_dir.join("build/GantryFixture.ipa");
    write_ios_ipa_fixture(&temp_dir, &artifact);
    let audited = run_gantry(
        &[
            "mobile",
            "audit",
            "--artifact",
            "build/GantryFixture.ipa",
            "--json",
        ],
        &temp_dir,
    );
    assert!(matches!(audited.status.code(), Some(0 | 1)));
    let audited: serde_json::Value = serde_json::from_slice(&audited.stdout).unwrap();
    assert_eq!(audited["report"]["schema_version"], "2.0");
    assert_eq!(audited["report"]["artifact"]["platform"], "ios");
    assert_eq!(
        audited["report"]["artifact"]["application_id"],
        "dev.trygantry.contract"
    );
    assert_eq!(
        audited["report"]["artifact"]["privacy_manifest_present"],
        true
    );
    assert_eq!(
        audited["report"]["artifact"]["sha256"]
            .as_str()
            .map(str::len),
        Some(64)
    );
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_macos_ios_contract_workflow_is_authoritative() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workflow = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    assert!(workflow.contains("mobile-ios-contract:"));
    assert!(workflow.contains("runs-on: macos-latest"));
    assert!(workflow.contains("build-ios-contract-ipa.sh"));
    assert!(workflow.contains("signature_verification.status !== \"verified\""));
    assert!(workflow.contains("cold.cache_hit !== false || warm.cache_hit !== true"));
    assert!(
        workflow.contains("actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4")
    );
}
