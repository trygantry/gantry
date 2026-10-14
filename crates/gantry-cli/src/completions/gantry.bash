_gantry_completions() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    opts="validate summarize init index audit verify install-hooks uninstall-hooks watch impact diff trends waiver evidence release tasks notify telemetry mcp completions --help -h --version -V --no-color"

    if [[ ${COMP_CWORD} -eq 1 ]] ; then
        COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
        return 0
    fi
}
complete -F _gantry_completions gantry
