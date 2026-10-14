# Disable file completions for gantry commands
complete -c gantry -f

# Main commands
complete -c gantry -a validate -d 'Gate a release — exits 0 (Go), 1 (No-Go), 2 (error)'
complete -c gantry -a summarize -d 'Print a plain-language launch summary'
complete -c gantry -a init -d 'Scaffold a .gantry/ vault in this project'
complete -c gantry -a index -d 'Rebuild index.json and write a snapshot'
complete -c gantry -a audit -d 'Score readiness and generate repair tasks'
complete -c gantry -a verify -d 'Verify the tamper-evident snapshot chain'
complete -c gantry -a install-hooks -d 'Block pushes/commits when readiness is No-Go'
complete -c gantry -a uninstall-hooks -d 'Remove the Gantry gate block from git hooks'
complete -c gantry -a watch -d 'Live re-validation daemon'
complete -c gantry -a impact -d 'Show which changed files map to blockers and QA suites'
complete -c gantry -a diff -d 'Compare two readiness snapshots by git refs'
complete -c gantry -a trends -d 'Summarize readiness score over time'
complete -c gantry -a waiver -d 'Manage accepted-risk waivers'
complete -c gantry -a evidence -d 'Attach structured evidence records'
complete -c gantry -a release -d 'Inspect release manifests'
complete -c gantry -a tasks -d 'List and complete repair tasks'
complete -c gantry -a notify -d 'Send a Slack digest for the latest snapshot transition'
complete -c gantry -a telemetry -d 'Pull Sentry crash-free stats into observed.json'
complete -c gantry -a mcp -d 'Launch the stdio MCP server for AI agent access'
complete -c gantry -a completions -d 'Generate shell completion scripts'

# Global flags
complete -c gantry -s h -l help -d 'Print help'
complete -c gantry -s V -l version -d 'Print version'
complete -c gantry -l no-color -d 'Disable colored output'
