Register-ArgumentCompleter -Native -CommandName gantry -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $opts = @(
        'validate', 'summarize', 'init', 'index', 'audit', 'verify',
        'install-hooks', 'uninstall-hooks', 'watch', 'impact', 'diff',
        'trends', 'waiver', 'evidence', 'release', 'tasks', 'notify',
        'telemetry', 'mcp', 'completions', '--help', '-h', '--version', '-V', '--no-color'
    )
    $opts | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object {
        [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)
    }
}
