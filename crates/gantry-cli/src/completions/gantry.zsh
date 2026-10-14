#compdef gantry

_gantry() {
    local line

    _arguments -C \
        '-V[Print version]' \
        '--version[Print version]' \
        '-h[Print help]' \
        '--help[Print help]' \
        '--no-color[Disable colored output]' \
        '--env[Evaluate against a named environment policy]:environment:_environments' \
        '--ci[CI mode: structured output]' \
        '--json[Output results as JSON]' \
        '*::track key:_gantry_cmds'

    return 0
}

_gantry_cmds() {
    local -a commands
    commands=(
        'validate:Gate a release — exits 0 (Go), 1 (No-Go), 2 (error)'
        'summarize:Print a plain-language launch summary'
        'init:Scaffold a .gantry/ vault in this project'
        'index:Rebuild index.json and write a snapshot'
        'audit:Score readiness and generate repair tasks'
        'verify:Verify the tamper-evident snapshot chain'
        'install-hooks:Block pushes/commits when readiness is No-Go'
        'uninstall-hooks:Remove the Gantry gate block from git hooks'
        'watch:Live re-validation daemon'
        'impact:Show which changed files map to blockers and QA suites'
        'diff:Compare two readiness snapshots by git refs'
        'trends:Summarize readiness score over time'
        'waiver:Manage accepted-risk waivers'
        'evidence:Attach structured evidence records'
        'release:Inspect release manifests'
        'tasks:List and complete repair tasks'
        'notify:Send a Slack digest for the latest snapshot transition'
        'telemetry:Pull Sentry crash-free stats into observed.json'
        'mcp:Launch the stdio MCP server for AI agent access'
        'completions:Generate shell completion scripts'
    )
    _describe -t commands 'gantry commands' commands
}
