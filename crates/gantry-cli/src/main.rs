use clap::{ColorChoice, CommandFactory, FromArgMatches, Parser, Subcommand};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process;

mod handlers;
mod output;
mod prompter;

use output::Output;

fn require_vault_context(
    context: Option<&gantry_core::VaultContext>,
) -> Result<&gantry_core::VaultContext, String> {
    context.ok_or_else(|| "This command requires an initialized Gantry vault.".to_string())
}

fn require_evaluation_options(
    options: Option<&gantry_core::EvaluationOptions>,
) -> Result<&gantry_core::EvaluationOptions, String> {
    options.ok_or_else(|| "This command requires an initialized Gantry vault.".to_string())
}

fn get_logo_string() -> &'static str {
    static LOGO: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    LOGO.get_or_init(|| {
        let no_color = std::env::args().any(|x| x == "--no-color");
        let wordmark = if no_color {
            "GANTRY".to_string()
        } else {
            use colored::Colorize;
            "GANTRY".bright_cyan().bold().to_string()
        };
        let mut logo_buf = format!("{wordmark}  /  MOBILE RELEASE GOVERNANCE\n");
        logo_buf.push_str("Local evidence. Exact candidates. Accountable decisions.\n");
        logo_buf.push('v');
        logo_buf.push_str(env!("CARGO_PKG_VERSION"));
        logo_buf.push_str("  ·  https://github.com/trygantry/gantry");
        logo_buf
    })
}

#[derive(Parser, Debug)]
#[command(
    name = "gantry",
    version,
    about = "Local-first mobile release governance",
    long_about = "Inspect source and exact IPA/AAB/APK candidates, enforce repository-owned policy, and record accountable release decisions.",
    before_help = get_logo_string(),
    after_help = "QUICK START\n  gantry mobile detect\n  gantry mobile init\n  gantry mobile audit --artifact <app.ipa|app.aab|app.apk>\n  gantry release portfolio\n\nRun 'gantry <command> --help' for command-specific examples and options."
)]
pub struct Cli {
    #[arg(long, global = true, env = "GANTRY_VAULT_PATH")]
    pub vault_path: Option<PathBuf>,
    #[arg(long, global = true, env = "GANTRY_ENV")]
    pub env: Option<String>,
    #[arg(short, long, global = true)]
    pub quiet: bool,
    #[arg(long, global = true)]
    pub no_color: bool,
    #[command(subcommand)]
    pub command: Option<Commands>,
}

use clap::ValueEnum;

/// Valid Gantry project template types.
#[derive(ValueEnum, Debug, Clone, PartialEq, Eq)]
pub enum TemplateType {
    MobileApp,
    SaasApp,
    AiApp,
    BackendApi,
    DesktopApp,
    ChromeExtension,
}

impl TemplateType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TemplateType::MobileApp => "mobile-app",
            TemplateType::SaasApp => "saas-app",
            TemplateType::AiApp => "ai-app",
            TemplateType::BackendApi => "backend-api",
            TemplateType::DesktopApp => "desktop-app",
            TemplateType::ChromeExtension => "chrome-extension",
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Validate the legacy launch-readiness vault and CI gate.
    Validate {
        #[arg(long, conflicts_with = "interactive")]
        ci: bool,
        #[arg(long)]
        report_url: Option<String>,
        #[arg(long)]
        policy: Option<String>,
        #[arg(short, long, conflicts_with = "ci")]
        interactive: bool,
        #[arg(long)]
        json: bool,
    },
    /// Rebuild the legacy readiness evidence index.
    Index,
    /// Show a concise legacy launch-readiness summary.
    Summarize,
    /// Initialize Gantry in the current project.
    #[command(display_order = 3)]
    Init {
        #[arg(long)]
        force: bool,
        #[arg(long, conflicts_with_all = ["template", "interactive"])]
        auto: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
        #[arg(long, conflicts_with = "auto", value_enum)]
        template: Option<TemplateType>,
        #[arg(short, long, conflicts_with = "auto")]
        interactive: bool,
        /// Initialize the mobile release policy alongside the mobile-app vault.
        #[arg(long, conflicts_with_all = ["auto", "template"])]
        mobile: bool,
    },
    /// Inspect or export the legacy evidence graph.
    Audit {
        #[command(subcommand)]
        subcommand: Option<AuditSubcommands>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        report_url: Option<String>,
        #[arg(long)]
        format: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// List generated remediation tasks.
    Tasks {
        #[arg(long)]
        json: bool,
    },
    /// Explain the release impact of repository changes.
    Impact {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        markdown: bool,
        #[arg(long)]
        base: Option<String>,
    },
    /// Compare current readiness with a named specification.
    Diff {
        spec: String,
        #[arg(long)]
        json: bool,
    },
    /// Send configured release notifications.
    Notify {
        #[arg(long)]
        dry_run: bool,
    },
    /// Start the Gantry Model Context Protocol server.
    Mcp,
    /// Complete or ignore a generated remediation task.
    Task {
        #[command(subcommand)]
        command: TaskCommands,
    },
    /// Pull configured telemetry evidence.
    Telemetry {
        #[command(subcommand)]
        command: TelemetryCommands,
    },
    /// Manage legacy readiness waivers.
    Waiver {
        #[command(subcommand)]
        command: WaiverCommands,
    },
    /// Attach legacy release evidence.
    Evidence {
        #[command(subcommand)]
        command: EvidenceCommands,
    },
    /// Verify, recover, and migrate durable Gantry vault records.
    #[command(display_order = 4)]
    Vault {
        #[command(subcommand)]
        command: VaultCommands,
    },
    /// Track exact candidates, evidence, decisions, and release packets.
    #[command(display_order = 2)]
    Release {
        #[command(subcommand)]
        command: ReleaseCommands,
    },
    /// Show historical readiness trends.
    Trends {
        #[arg(long)]
        json: bool,
    },
    /// Watch the project and refresh readiness on changes.
    Watch,
    /// Verify the current readiness snapshot.
    Verify {
        #[arg(long)]
        json: bool,
    },
    /// Install Gantry validation Git hooks.
    #[command(name = "install-hooks")]
    InstallHooks {
        #[arg(long)]
        force: bool,
        #[arg(long, default_value = "push")]
        stage: String,
    },
    /// Remove Gantry validation Git hooks.
    #[command(name = "uninstall-hooks")]
    UninstallHooks {
        #[arg(long, default_value = "push")]
        stage: String,
    },
    /// Generate shell completion scripts.
    Completions { shell: String },
    /// Check for CLI updates.
    Update {
        #[command(subcommand)]
        command: UpdateCommands,
    },
    /// Detect and govern iOS/Android release candidates.
    #[command(display_order = 1)]
    Mobile {
        #[command(subcommand)]
        command: MobileCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum VaultCommands {
    /// Verify release records and report interrupted transactions.
    Check {
        /// Roll fully checksummed interrupted transactions forward before verification.
        #[arg(long)]
        repair: bool,
        #[arg(long)]
        json: bool,
    },
    /// Plan or apply registered schema migrations.
    Migrate {
        /// Verify and print the migration plan without changing files.
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Write a redacted diagnostic bundle without source, artifacts, or credentials.
    SupportBundle {
        /// Destination JSON path. Defaults under .gantry/support/.
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum AuditSubcommands {
    Export {
        #[arg(long)]
        format: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum TaskCommands {
    Complete { id: String },
    Ignore { id: String, reason: Vec<String> },
}

#[derive(Subcommand, Debug)]
pub enum TelemetryCommands {
    Pull,
}

#[derive(Subcommand, Debug)]
pub enum WaiverCommands {
    List {
        #[arg(long)]
        json: bool,
    },
    Add {
        #[arg(long)]
        target_type: Option<String>,
        #[arg(long)]
        target_id: Option<String>,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        approved_by: Option<String>,
        #[arg(long)]
        expires_at: Option<String>,
        #[arg(long)]
        target_path: Option<String>,
        #[arg(long)]
        target_code: Option<String>,
        #[arg(short, long)]
        interactive: bool,
    },
    Expire {
        id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum EvidenceCommands {
    Add {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        artifact_sha: Option<String>,
        /// Exact mobile evidence requirement this record satisfies.
        #[arg(long)]
        mobile_evidence_kind: Option<String>,
        /// Human or provider identity that issued the evidence.
        #[arg(long)]
        evidence_issuer: Option<String>,
        /// RFC 3339 expiry time for time-bounded evidence.
        #[arg(long)]
        expires_at: Option<String>,
        #[arg(long)]
        env: Option<String>,
        #[arg(long)]
        release: Option<String>,
        #[arg(short, long)]
        interactive: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ReleaseCommands {
    /// Create a durable workflow release.
    Create {
        /// App version or release label, such as 2.4.0.
        version: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        owner: Option<String>,
        /// Identity recorded in the append-only release event.
        #[arg(long, env = "GANTRY_ACTOR")]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Create a release once or return the existing verified release for this version.
    Ensure {
        version: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        owner: Option<String>,
        #[arg(long, env = "GANTRY_ACTOR")]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List durable workflow releases.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Summarize verified releases and the prioritized review queue.
    Portfolio {
        #[arg(long)]
        json: bool,
    },
    /// Show one workflow release, its candidates, and verified event history.
    Show {
        /// REL- identifier or unique version.
        release: String,
        #[arg(long)]
        json: bool,
    },
    /// Record target, submission, or rollout dates in the verified release history.
    Schedule {
        /// REL- identifier or unique version.
        release: String,
        #[arg(long)]
        target_date: Option<String>,
        #[arg(long)]
        submission_date: Option<String>,
        #[arg(long)]
        rollout_date: Option<String>,
        #[arg(long, env = "GANTRY_ACTOR")]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Bind and manage immutable release candidates.
    Candidate {
        #[command(subcommand)]
        command: ReleaseCandidateCommands,
    },
    /// Audit the active candidate and persist the canonical decision report.
    Audit {
        /// REL- identifier or unique version.
        release: String,
        /// Required when both iOS and Android candidates are active.
        #[arg(long, value_enum)]
        platform: Option<ReleasePlatformArg>,
        /// Identity recorded in the append-only release event.
        #[arg(long, env = "GANTRY_ACTOR")]
        actor: Option<String>,
        /// Return the active audit without adding an event when its deterministic inputs are unchanged.
        #[arg(long)]
        reuse_unchanged: bool,
        /// Emit stable GitHub workflow annotations for non-passing findings.
        #[arg(long)]
        ci: bool,
        #[arg(long)]
        json: bool,
    },
    /// Attach evidence to the active candidate and an exact audit finding.
    Evidence {
        #[command(subcommand)]
        command: ReleaseEvidenceCommands,
    },
    /// Record human workflow state for exact audit findings.
    Finding {
        #[command(subcommand)]
        command: ReleaseFindingCommands,
    },
    /// Approve the active audited candidate. No-Go audits cannot be approved.
    Approve {
        /// REL- identifier or unique version.
        release: String,
        #[arg(long, value_enum)]
        platform: Option<ReleasePlatformArg>,
        /// Accountable decision-maker identity.
        #[arg(long = "by", env = "GANTRY_ACTOR")]
        decided_by: String,
        /// Decision-maker role, such as release_manager.
        #[arg(long)]
        role: String,
        #[arg(long)]
        rationale: String,
        #[arg(long)]
        json: bool,
    },
    /// Reject the active audited candidate.
    Reject {
        /// REL- identifier or unique version.
        release: String,
        #[arg(long, value_enum)]
        platform: Option<ReleasePlatformArg>,
        /// Accountable decision-maker identity.
        #[arg(long = "by", env = "GANTRY_ACTOR")]
        decided_by: String,
        /// Decision-maker role, such as release_manager.
        #[arg(long)]
        role: String,
        #[arg(long)]
        rationale: String,
        #[arg(long)]
        json: bool,
    },
    /// Accept a time-bounded finding risk without changing the deterministic audit decision.
    Waive {
        /// REL- identifier or unique version.
        release: String,
        /// Exact finding_instance_id from the active audit.
        #[arg(long)]
        finding: String,
        #[arg(long, value_enum)]
        platform: Option<ReleasePlatformArg>,
        #[arg(long)]
        reason: String,
        /// Accountable waiver approver identity.
        #[arg(long, env = "GANTRY_ACTOR")]
        approved_by: String,
        /// Required RFC 3339 expiry time.
        #[arg(long)]
        expires_at: String,
        #[arg(long)]
        json: bool,
    },
    /// Write a deterministic verified release packet as JSON, Markdown, and HTML.
    Packet {
        /// REL- identifier or unique version.
        release: String,
        #[arg(long)]
        json: bool,
    },
    /// Compare two immutable candidates and their latest audit findings.
    Diff {
        /// REL- identifier or unique version.
        release: String,
        #[arg(long, value_enum)]
        platform: Option<ReleasePlatformArg>,
        /// Earlier CAN- identifier. Defaults to the candidate before --to.
        #[arg(long)]
        from: Option<String>,
        /// Later CAN- identifier. Defaults to the active platform candidate.
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Print the legacy readiness manifest generated by `gantry index`.
    Manifest {
        #[arg(long)]
        json: bool,
    },
    /// List legacy readiness manifests generated by `gantry index`.
    History {
        #[arg(long)]
        json: bool,
    },
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum ReleasePlatformArg {
    Ios,
    Android,
}

fn release_platform(platform: ReleasePlatformArg) -> gantry_core::release::MobilePlatform {
    match platform {
        ReleasePlatformArg::Ios => gantry_core::release::MobilePlatform::Ios,
        ReleasePlatformArg::Android => gantry_core::release::MobilePlatform::Android,
    }
}

#[derive(Subcommand, Debug)]
pub enum ReleaseCandidateCommands {
    /// Hash an IPA/AAB/APK and bind it to a workflow release.
    Add {
        /// REL- identifier or unique version.
        release: String,
        #[arg(long)]
        artifact: PathBuf,
        /// Normally inferred from the artifact extension.
        #[arg(long, value_enum)]
        platform: Option<ReleasePlatformArg>,
        /// Identity recorded in the append-only release event.
        #[arg(long, env = "GANTRY_ACTOR")]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Bind a candidate only when artifact bytes or Git identity changed.
    Ensure {
        release: String,
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long, value_enum)]
        platform: Option<ReleasePlatformArg>,
        #[arg(long, env = "GANTRY_ACTOR")]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ReleaseEvidenceCommands {
    /// Create artifact-bound mobile evidence and link it to an audit finding.
    Add {
        /// REL- identifier or unique version.
        release: String,
        #[arg(long, value_enum)]
        platform: Option<ReleasePlatformArg>,
        /// Exact finding_instance_id from the active audit.
        #[arg(long)]
        finding: String,
        /// Exact mobile evidence requirement, such as release_owner_approval.
        #[arg(long)]
        kind: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        issuer: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        url: Option<String>,
        /// RFC 3339 expiry time.
        #[arg(long)]
        expires_at: Option<String>,
        #[arg(long, env = "GANTRY_ACTOR")]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ReleaseFindingCommands {
    /// Mark a finding as addressed workflow work; the audit decision is unchanged.
    Resolve {
        /// REL- identifier or unique version.
        release: String,
        /// Exact finding_instance_id from the active audit.
        finding: String,
        #[arg(long, value_enum)]
        platform: Option<ReleasePlatformArg>,
        #[arg(long)]
        reason: String,
        /// Candidate-linked EVI identifier. Repeat for multiple records.
        #[arg(long = "evidence")]
        evidence_ids: Vec<String>,
        #[arg(long, env = "GANTRY_ACTOR")]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum UpdateCommands {
    Check {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum MobileCommands {
    /// Detect Flutter, React Native, native iOS, and native Android projects.
    Detect {
        #[arg(long)]
        json: bool,
    },
    /// Audit source metadata and optionally the exact IPA/AAB/APK candidate.
    Audit {
        #[arg(long)]
        artifact: Option<PathBuf>,
        #[arg(long)]
        format: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        write: bool,
        #[arg(long)]
        ci: bool,
        #[arg(long)]
        json: bool,
    },
    /// Print the verified derived-cache identity for an exact candidate.
    CacheKey {
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Print the built-in versioned mobile rule catalog.
    Rules {
        #[arg(long)]
        json: bool,
    },
    /// Create or refresh .gantry/mobile-policy.json.
    Init {
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let color_choice = if std::env::args().any(|x| x == "--no-color") {
        colored::control::set_override(false);
        ColorChoice::Never
    } else {
        ColorChoice::Auto
    };

    let mut cmd = Cli::command().color(color_choice);

    // With no arguments, show the same concise help as `gantry --help`.
    if std::env::args().len() == 1 {
        let _ = cmd.print_help();
        println!();
        process::exit(0);
    }

    let matches = match cmd.try_get_matches() {
        Ok(m) => m,
        Err(err) => {
            if err.use_stderr() {
                eprintln!("{}", err);
                process::exit(2);
            } else {
                print!("{}", err);
                process::exit(0);
            }
        }
    };

    let args = Cli::from_arg_matches(&matches).unwrap_or_else(|e| {
        eprintln!("{}", e);
        process::exit(2);
    });

    let json_mode = args
        .command
        .as_ref()
        .map(|c| match c {
            Commands::Validate { json, .. } => *json,
            Commands::Init { json, .. } => *json,
            Commands::Audit { json, .. } => *json,
            Commands::Tasks { json, .. } => *json,
            Commands::Impact { json, .. } => *json,
            Commands::Diff { json, .. } => *json,
            Commands::Waiver {
                command: WaiverCommands::List { json, .. },
            } => *json,
            Commands::Vault {
                command:
                    VaultCommands::Check { json, .. }
                    | VaultCommands::Migrate { json, .. }
                    | VaultCommands::SupportBundle { json, .. },
            } => *json,
            Commands::Release {
                command:
                    ReleaseCommands::Create { json, .. }
                    | ReleaseCommands::Ensure { json, .. }
                    | ReleaseCommands::List { json }
                    | ReleaseCommands::Portfolio { json }
                    | ReleaseCommands::Show { json, .. }
                    | ReleaseCommands::Schedule { json, .. }
                    | ReleaseCommands::Candidate {
                        command:
                            ReleaseCandidateCommands::Add { json, .. }
                            | ReleaseCandidateCommands::Ensure { json, .. },
                    }
                    | ReleaseCommands::Audit { json, .. }
                    | ReleaseCommands::Evidence {
                        command: ReleaseEvidenceCommands::Add { json, .. },
                    }
                    | ReleaseCommands::Finding {
                        command: ReleaseFindingCommands::Resolve { json, .. },
                    }
                    | ReleaseCommands::Approve { json, .. }
                    | ReleaseCommands::Reject { json, .. }
                    | ReleaseCommands::Waive { json, .. }
                    | ReleaseCommands::Packet { json, .. }
                    | ReleaseCommands::Diff { json, .. }
                    | ReleaseCommands::Manifest { json }
                    | ReleaseCommands::History { json },
            } => *json,
            Commands::Trends { json } => *json,
            Commands::Verify { json } => *json,
            Commands::Update {
                command: UpdateCommands::Check { json },
            } => *json,
            Commands::Mobile {
                command:
                    MobileCommands::Detect { json }
                    | MobileCommands::Audit { json, .. }
                    | MobileCommands::CacheKey { json, .. }
                    | MobileCommands::Rules { json }
                    | MobileCommands::Init { json, .. },
            } => *json,
            _ => false,
        })
        .unwrap_or(false);

    let output = Output::new(args.quiet, !args.no_color, json_mode);

    let interactive = args
        .command
        .as_ref()
        .map(|c| match c {
            Commands::Validate { interactive, .. } => *interactive,
            Commands::Init { interactive, .. } => *interactive,
            Commands::Waiver {
                command: WaiverCommands::Add { interactive, .. },
            } => *interactive,
            Commands::Evidence {
                command: EvidenceCommands::Add { interactive, .. },
            } => *interactive,
            _ => false,
        })
        .unwrap_or(false);

    if interactive {
        if json_mode || args.quiet {
            output.print_stderr("Error: --interactive cannot be used with --json or --quiet.");
            process::exit(2);
        }
        if !std::io::stdin().is_terminal() {
            output
                .print_stderr("Error: Input is not a TTY. Interactive prompts require a terminal.");
            process::exit(2);
        }
    }

    let prompter = prompter::TerminalPrompter;
    let explicit_vault_path = args.vault_path.as_deref();

    // Resolve context based on subcommand requirements
    let requires_vault = !matches!(
        args.command.as_ref(),
        Some(Commands::Init { .. })
            | Some(Commands::InstallHooks { .. })
            | Some(Commands::UninstallHooks { .. })
            | Some(Commands::Completions { .. })
            | Some(Commands::Update { .. })
            | Some(Commands::Mobile { .. })
            | None
    );

    let vault_ctx = if requires_vault {
        match gantry_core::resolve_existing_vault(explicit_vault_path) {
            Ok(ctx) => Some(ctx),
            Err(e) => {
                output.print_stderr(&format!("Error: {}", e));
                process::exit(2);
            }
        }
    } else if let Some(Commands::Init { .. }) = args.command.as_ref() {
        match gantry_core::resolve_init_target(explicit_vault_path) {
            Ok(ctx) => Some(ctx),
            Err(e) => {
                output.print_stderr(&format!("Error: {}", e));
                process::exit(2);
            }
        }
    } else {
        None
    };

    let project_root = if let Some(ref ctx) = vault_ctx {
        ctx.project_root.clone()
    } else {
        match gantry_core::resolve_existing_vault(explicit_vault_path) {
            Ok(ctx) => ctx.project_root,
            Err(_) => match std::env::current_dir() {
                Ok(cwd) => cwd,
                Err(e) => {
                    output.print_stderr(&format!("Failed to determine project root: {}", e));
                    process::exit(2);
                }
            },
        }
    };

    let eval_opts = vault_ctx
        .as_ref()
        .map(|ctx| gantry_core::EvaluationOptions {
            vault: ctx.clone(),
            environment: args.env.clone(),
            policy_overlay: None,
        });

    let exit_code = (|| -> Result<i32, String> {
        match args.command.unwrap_or(Commands::Summarize) {
            Commands::Validate {
                ci,
                report_url,
                policy,
                ..
            } => handlers::validate::handle(
                &output,
                ci,
                report_url,
                policy,
                require_evaluation_options(eval_opts.as_ref())?.clone(),
            ),
            Commands::Index => {
                handlers::index::handle(&output, require_evaluation_options(eval_opts.as_ref())?)
            }
            Commands::Summarize => handlers::summarize::handle(
                &output,
                require_evaluation_options(eval_opts.as_ref())?,
            ),
            Commands::Init {
                force,
                auto,
                dry_run,
                json,
                template,
                interactive,
                mobile,
            } => handlers::init::handle(
                &output,
                &prompter,
                force,
                auto,
                dry_run,
                json,
                if mobile {
                    Some("mobile-app".to_string())
                } else {
                    template.as_ref().map(|t| t.as_str().to_string())
                },
                interactive,
                mobile,
                require_vault_context(vault_ctx.as_ref())?,
            ),
            Commands::Audit {
                subcommand,
                dry_run,
                json,
                report_url,
                format,
                output: out_path,
            } => match subcommand {
                Some(AuditSubcommands::Export {
                    format: fmt,
                    output: out,
                }) => handlers::audit::handle_export(
                    &output,
                    fmt,
                    out,
                    require_evaluation_options(eval_opts.as_ref())?,
                ),
                None => handlers::audit::handle_audit(
                    &output,
                    dry_run,
                    json,
                    report_url,
                    format,
                    out_path,
                    require_evaluation_options(eval_opts.as_ref())?,
                ),
            },
            Commands::Tasks { json } => handlers::tasks::handle(
                &output,
                json,
                &require_vault_context(vault_ctx.as_ref())?.vault_path,
            ),
            Commands::Impact {
                json,
                markdown,
                base,
            } => handlers::impact::handle(
                &output,
                json,
                markdown,
                base,
                require_vault_context(vault_ctx.as_ref())?,
            ),
            Commands::Diff { spec, json } => handlers::diff::handle(
                &output,
                spec,
                json,
                &require_vault_context(vault_ctx.as_ref())?.vault_path,
            ),
            Commands::Notify { dry_run } => handlers::notify::handle(
                &output,
                dry_run,
                &require_vault_context(vault_ctx.as_ref())?.vault_path,
            ),
            Commands::Mcp => {
                handlers::mcp::handle(&output, require_evaluation_options(eval_opts.as_ref())?)
            }
            Commands::Task { command } => {
                let cmd = match command {
                    TaskCommands::Complete { id } => handlers::task::TaskCommand::Complete { id },
                    TaskCommands::Ignore { id, reason } => handlers::task::TaskCommand::Ignore {
                        id,
                        reason: reason.join(" "),
                    },
                };
                handlers::task::handle(
                    &output,
                    cmd,
                    &require_vault_context(vault_ctx.as_ref())?.vault_path,
                )
            }
            Commands::Telemetry { command } => match command {
                TelemetryCommands::Pull => handlers::telemetry::handle(
                    &output,
                    args.env,
                    &require_vault_context(vault_ctx.as_ref())?.vault_path,
                ),
            },
            Commands::Waiver { command } => {
                let cmd = match command {
                    WaiverCommands::List { json } => handlers::waiver::WaiverCommand::List { json },
                    WaiverCommands::Add {
                        target_type,
                        target_id,
                        reason,
                        approved_by,
                        expires_at,
                        target_path,
                        target_code,
                        interactive,
                    } => handlers::waiver::WaiverCommand::Add {
                        target_type,
                        target_id,
                        reason,
                        approved_by,
                        expires_at,
                        target_path,
                        target_code,
                        interactive,
                    },
                    WaiverCommands::Expire { id } => handlers::waiver::WaiverCommand::Expire { id },
                };
                handlers::waiver::handle(
                    &output,
                    &prompter,
                    cmd,
                    &require_vault_context(vault_ctx.as_ref())?.vault_path,
                )
            }
            Commands::Evidence { command } => {
                let cmd = match command {
                    EvidenceCommands::Add {
                        kind,
                        title,
                        path,
                        url,
                        artifact_sha,
                        mobile_evidence_kind,
                        evidence_issuer,
                        expires_at,
                        env,
                        release,
                        interactive,
                    } => handlers::evidence::EvidenceCommand::Add {
                        kind,
                        title,
                        path,
                        url,
                        artifact_sha,
                        mobile_evidence_kind,
                        evidence_issuer,
                        expires_at,
                        env,
                        release,
                        interactive,
                    },
                };
                handlers::evidence::handle(
                    &output,
                    &prompter,
                    cmd,
                    require_vault_context(vault_ctx.as_ref())?,
                )
            }
            Commands::Vault { command } => {
                let cmd = match command {
                    VaultCommands::Check { repair, json } => {
                        handlers::vault::VaultCommand::Check { repair, json }
                    }
                    VaultCommands::Migrate { dry_run, json } => {
                        handlers::vault::VaultCommand::Migrate { dry_run, json }
                    }
                    VaultCommands::SupportBundle { output, json } => {
                        handlers::vault::VaultCommand::SupportBundle { output, json }
                    }
                };
                handlers::vault::handle(&output, cmd, require_vault_context(vault_ctx.as_ref())?)
            }
            Commands::Release { command } => {
                let cmd = match command {
                    ReleaseCommands::Create {
                        version,
                        name,
                        owner,
                        actor,
                        json,
                    } => handlers::release::ReleaseCommand::Create {
                        version,
                        name,
                        owner,
                        actor,
                        json,
                    },
                    ReleaseCommands::Ensure {
                        version,
                        name,
                        owner,
                        actor,
                        json,
                    } => handlers::release::ReleaseCommand::Ensure {
                        version,
                        name,
                        owner,
                        actor,
                        json,
                    },
                    ReleaseCommands::List { json } => {
                        handlers::release::ReleaseCommand::List { json }
                    }
                    ReleaseCommands::Portfolio { json } => {
                        handlers::release::ReleaseCommand::Portfolio { json }
                    }
                    ReleaseCommands::Show { release, json } => {
                        handlers::release::ReleaseCommand::Show { release, json }
                    }
                    ReleaseCommands::Schedule {
                        release,
                        target_date,
                        submission_date,
                        rollout_date,
                        actor,
                        json,
                    } => handlers::release::ReleaseCommand::Schedule {
                        release,
                        target_date,
                        submission_date,
                        rollout_date,
                        actor,
                        json,
                    },
                    ReleaseCommands::Candidate { command } => match command {
                        ReleaseCandidateCommands::Add {
                            release,
                            artifact,
                            platform,
                            actor,
                            json,
                        } => handlers::release::ReleaseCommand::CandidateAdd {
                            release,
                            artifact,
                            platform: platform.map(|platform| match platform {
                                ReleasePlatformArg::Ios => {
                                    gantry_core::release::MobilePlatform::Ios
                                }
                                ReleasePlatformArg::Android => {
                                    gantry_core::release::MobilePlatform::Android
                                }
                            }),
                            actor,
                            json,
                        },
                        ReleaseCandidateCommands::Ensure {
                            release,
                            artifact,
                            platform,
                            actor,
                            json,
                        } => handlers::release::ReleaseCommand::CandidateEnsure {
                            release,
                            artifact,
                            platform: platform.map(release_platform),
                            actor,
                            json,
                        },
                    },
                    ReleaseCommands::Audit {
                        release,
                        platform,
                        actor,
                        reuse_unchanged,
                        ci,
                        json,
                    } => handlers::release::ReleaseCommand::Audit {
                        release,
                        platform: platform.map(|platform| match platform {
                            ReleasePlatformArg::Ios => gantry_core::release::MobilePlatform::Ios,
                            ReleasePlatformArg::Android => {
                                gantry_core::release::MobilePlatform::Android
                            }
                        }),
                        actor,
                        reuse_unchanged,
                        ci,
                        json,
                    },
                    ReleaseCommands::Evidence { command } => match command {
                        ReleaseEvidenceCommands::Add {
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
                        } => handlers::release::ReleaseCommand::EvidenceAdd {
                            release,
                            platform: platform.map(release_platform),
                            finding,
                            kind,
                            title,
                            issuer,
                            path,
                            url,
                            expires_at,
                            actor,
                            json,
                        },
                    },
                    ReleaseCommands::Finding { command } => match command {
                        ReleaseFindingCommands::Resolve {
                            release,
                            finding,
                            platform,
                            reason,
                            evidence_ids,
                            actor,
                            json,
                        } => handlers::release::ReleaseCommand::FindingResolve {
                            release,
                            platform: platform.map(release_platform),
                            finding,
                            reason,
                            evidence_ids,
                            actor,
                            json,
                        },
                    },
                    ReleaseCommands::Approve {
                        release,
                        platform,
                        decided_by,
                        role,
                        rationale,
                        json,
                    } => handlers::release::ReleaseCommand::Decide {
                        release,
                        platform: platform.map(release_platform),
                        decided_by,
                        role,
                        rationale,
                        decision: gantry_core::release::HumanDecisionKind::Approved,
                        json,
                    },
                    ReleaseCommands::Reject {
                        release,
                        platform,
                        decided_by,
                        role,
                        rationale,
                        json,
                    } => handlers::release::ReleaseCommand::Decide {
                        release,
                        platform: platform.map(release_platform),
                        decided_by,
                        role,
                        rationale,
                        decision: gantry_core::release::HumanDecisionKind::Rejected,
                        json,
                    },
                    ReleaseCommands::Waive {
                        release,
                        finding,
                        platform,
                        reason,
                        approved_by,
                        expires_at,
                        json,
                    } => handlers::release::ReleaseCommand::Waive {
                        release,
                        platform: platform.map(release_platform),
                        finding,
                        reason,
                        approved_by,
                        expires_at,
                        json,
                    },
                    ReleaseCommands::Packet { release, json } => {
                        handlers::release::ReleaseCommand::Packet { release, json }
                    }
                    ReleaseCommands::Diff {
                        release,
                        platform,
                        from,
                        to,
                        json,
                    } => handlers::release::ReleaseCommand::Diff {
                        release,
                        platform: platform.map(release_platform),
                        from,
                        to,
                        json,
                    },
                    ReleaseCommands::Manifest { json } => {
                        handlers::release::ReleaseCommand::Manifest { json }
                    }
                    ReleaseCommands::History { json } => {
                        handlers::release::ReleaseCommand::History { json }
                    }
                };
                handlers::release::handle(&output, cmd, require_vault_context(vault_ctx.as_ref())?)
            }
            Commands::Trends { json } => handlers::trends::handle(
                &output,
                json,
                &require_vault_context(vault_ctx.as_ref())?.vault_path,
            ),
            Commands::Watch => handlers::watch::handle(
                &output,
                &require_vault_context(vault_ctx.as_ref())?.vault_path,
                require_evaluation_options(eval_opts.as_ref())?,
            ),
            Commands::Verify { json } => handlers::verify::handle(
                &output,
                json,
                &require_vault_context(vault_ctx.as_ref())?.vault_path,
            ),
            Commands::InstallHooks { force, stage } => {
                let cmd = handlers::hooks::HooksCommand::Install { force, stage };
                handlers::hooks::handle(&output, cmd, &project_root)
            }
            Commands::UninstallHooks { stage } => {
                let cmd = handlers::hooks::HooksCommand::Uninstall { stage };
                handlers::hooks::handle(&output, cmd, &project_root)
            }
            Commands::Completions { shell } => {
                let mut completions_cmd = Cli::command();
                handlers::completions::handle(&output, shell, &mut completions_cmd)
            }
            Commands::Update { command } => match command {
                UpdateCommands::Check { json } => {
                    let fetcher = handlers::update::HttpReleaseFetcher;
                    handlers::update::handle(&output, json, &fetcher)
                }
            },
            Commands::Mobile { command } => {
                handlers::mobile::handle(&output, command, &project_root)
            }
        }
    })();

    match exit_code {
        Ok(code) => process::exit(code),
        Err(e) => {
            output.print_stderr(&format!("Error: {}", e));
            process::exit(2);
        }
    }
}
