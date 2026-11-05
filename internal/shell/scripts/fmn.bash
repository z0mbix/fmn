# fmn shell integration
if [[ $- == *i* && -z ${__FMN_INITIALIZED-} ]]; then
    __FMN_INITIALIZED=1
    FMN_SESSION_ID="$(command fmn _session)" || FMN_SESSION_ID=
    export FMN_SESSION_ID
    __fmn_ready=0
    __fmn_command_active=0
    __fmn_in_hook=0
    __fmn_last_history_line=

    __fmn_preexec() {
        [[ $__fmn_in_hook -eq 0 && $__fmn_ready -eq 1 && $__fmn_command_active -eq 0 ]] || return 0
        __fmn_in_hook=1
        local __fmn_history_line
        local __fmn_command_text
        local __fmn_history_pattern='^[[:space:]]*([0-9]+)[* ] (.*)$'
        local HISTTIMEFORMAT=
        __fmn_history_line="$(builtin history 1)"
        # An unchanged entry means Bash omitted this command from its history.
        # Consume only the history number's two-character separator, preserving
        # leading whitespace for fmn's privacy filter.
        if [[ $__fmn_history_line == "$__fmn_last_history_line" || ! $__fmn_history_line =~ $__fmn_history_pattern ]]; then
            __fmn_in_hook=0
            return 0
        fi
        __fmn_command_text="${BASH_REMATCH[2]}"
        __fmn_command_active=1
        if [[ -n $FMN_SESSION_ID ]]; then
            printf %s "$__fmn_command_text" | command fmn _record-start \
                --shell bash --session "$FMN_SESSION_ID" --cwd "$PWD" \
                >/dev/null 2>/dev/null || true
        fi
        __fmn_in_hook=0
        return 0
    }

    __fmn_precmd() {
        local __fmn_exit_code=$?
        __fmn_prompt_exit_code=$__fmn_exit_code
        __fmn_in_hook=1
        if [[ $__fmn_ready -eq 1 && $__fmn_command_active -eq 1 && -n $FMN_SESSION_ID ]]; then
            command fmn _record-finish \
                --session "$FMN_SESSION_ID" --exit-code "$__fmn_exit_code" \
                >/dev/null 2>/dev/null || true
        fi
        __fmn_command_active=0
        __fmn_ready=0
        return "$__fmn_exit_code"
    }

    __fmn_prompt_end() {
        local HISTTIMEFORMAT=
        __fmn_last_history_line="$(builtin history 1)"
        __fmn_ready=1
        __fmn_in_hook=0
        return "$__fmn_prompt_exit_code"
    }

    __fmn_search_widget() {
        local __fmn_result
        local __fmn_search_status=0
        # Reset queued input and acceptance before every search, including
        # cancellation and config errors after an earlier auto-execution.
        bind -m emacs-standard '"\C-x\C-g":""'
        bind -m emacs-standard '"\C-x\C-h":""'
        bind -m vi-insert '"\C-x\C-h":""'
        __fmn_result="$(command fmn search --query "$READLINE_LINE" --cwd "$PWD" \
            --shell-integration 3>&1 1>&2 2>&3 0<&1 3>&-)" || __fmn_search_status=$?
        if [[ $__fmn_search_status -eq 0 || $__fmn_search_status -eq 10 ]]; then
            if (( BASH_VERSINFO[0] >= 4 )); then
                READLINE_LINE="$__fmn_result"
                READLINE_POINT=${#READLINE_LINE}
            else
                # Bash 3 has no writable READLINE_LINE. Queue a literal-input
                # macro after this widget, quoting control bytes so selection
                # cannot accept a line or invoke another Readline binding.
                local LC_ALL=C
                local __fmn_macro='\C-e\C-u' __fmn_byte __fmn_octal __fmn_index
                for ((__fmn_index=0; __fmn_index<${#__fmn_result}; __fmn_index++)); do
                    printf -v __fmn_byte '%d' "'${__fmn_result:__fmn_index:1}"
                    printf -v __fmn_octal '\\%03o' "$__fmn_byte"
                    if (( __fmn_byte < 32 || __fmn_byte == 127 )); then
                        __fmn_macro+='\C-v'
                    fi
                    __fmn_macro+="$__fmn_octal"
                done
                bind -m emacs-standard '"\C-x\C-g":"'"$__fmn_macro"'"'
            fi
            if [[ $__fmn_search_status -eq 10 ]]; then
                bind -m emacs-standard '"\C-x\C-h":"\C-m"'
                bind -m vi-insert '"\C-x\C-h":"\C-m"'
            fi
        elif [[ -n $__fmn_result ]]; then
            printf '%s\n' "$__fmn_result" >&2
        fi
    }

    __fmn_saved_debug_handler=
    __fmn_debug_declaration="$(trap -p DEBUG)"
    if [[ -n $__fmn_debug_declaration ]]; then
        __fmn_saved_debug_handler="${__fmn_debug_declaration#trap -- \'}"
        __fmn_saved_debug_handler="${__fmn_saved_debug_handler%\' DEBUG}"
        eval "__fmn_saved_debug_handler='$__fmn_saved_debug_handler'"
    fi
    if [[ -n $__fmn_saved_debug_handler ]]; then
        # Compose the handler captured at initialization time.
        # shellcheck disable=SC2064
        trap "__fmn_preexec; $__fmn_saved_debug_handler" DEBUG
    else
        trap '__fmn_preexec' DEBUG
    fi
    unset __fmn_debug_declaration

    if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]] &&
        (( BASH_VERSINFO[0] > 5 || (BASH_VERSINFO[0] == 5 && BASH_VERSINFO[1] >= 1) )); then
        PROMPT_COMMAND=(__fmn_precmd "${PROMPT_COMMAND[@]}" __fmn_prompt_end)
    elif [[ -n ${PROMPT_COMMAND-} ]]; then
        PROMPT_COMMAND="__fmn_precmd
$PROMPT_COMMAND
__fmn_prompt_end"
    else
        PROMPT_COMMAND='__fmn_precmd; __fmn_prompt_end'
    fi
    # Accept only after the widget returns and the original editing mode is
    # restored. Bash 3 also inserts through a literal-input macro here.
    bind -m emacs-standard -x '"\C-x\C-r":__fmn_search_widget'
    bind -m emacs-standard '"\C-x\C-g":""'
    bind -m emacs-standard '"\C-x\C-h":""'
    bind -m vi-insert '"\C-x\C-h":""'
    bind -m emacs-standard '"\C-r":"\C-x\C-r\C-x\C-g\C-x\C-h"'
    bind -m vi-insert '"\C-x\C-]":emacs-editing-mode'
    bind -m emacs-standard '"\C-x\C-]":vi-editing-mode'
    bind -m vi-insert '"\C-r":"\C-x\C-]\C-x\C-r\C-x\C-g\C-x\C-]\C-x\C-h"'
fi
