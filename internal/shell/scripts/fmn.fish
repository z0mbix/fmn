# fmn shell integration
if status is-interactive; and not set -q __fmn_initialized
    set -g __fmn_initialized 1
    set -gx FMN_SESSION_ID (command fmn _session)

    function __fmn_preexec --on-event fish_preexec
        if test -n "$FMN_SESSION_ID"
            printf %s "$argv[1]" | command fmn _record-start \
                --shell fish --session "$FMN_SESSION_ID" --cwd "$PWD" \
                >/dev/null 2>/dev/null
        end
    end

    function __fmn_postexec --on-event fish_postexec
        set -l __fmn_exit_code $status
        if test -n "$FMN_SESSION_ID"
            command fmn _record-finish \
                --session "$FMN_SESSION_ID" --exit-code "$__fmn_exit_code" \
                >/dev/null 2>/dev/null
        end
    end

    function __fmn_search
        set -l __fmn_result "$(command fmn search --query (commandline) --cwd "$PWD" \
            --shell-integration 3>&1 1>&2 2>&3 0<&1 3>&-)"
        set -l __fmn_search_status $status
        if test $__fmn_search_status -eq 0; or test $__fmn_search_status -eq 10
            commandline --replace -- "$__fmn_result"
            commandline --cursor (string length -- "$__fmn_result")
        else if test -n "$__fmn_result"
            printf '%s\n' "$__fmn_result" >&2
        end
        commandline -f repaint
        if test $__fmn_search_status -eq 10
            commandline -f execute
        end
    end
    bind \cr __fmn_search
    bind -M insert \cr __fmn_search
end
