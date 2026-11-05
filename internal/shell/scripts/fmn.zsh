# fmn shell integration
if [[ -o interactive && -z ${__FMN_INITIALIZED-} ]]; then
    typeset -g __FMN_INITIALIZED=1
    typeset -gx FMN_SESSION_ID="$(command fmn _session)"
    autoload -Uz add-zsh-hook

    __fmn_preexec() {
        local __fmn_command_text="$1"
        if [[ -n "$FMN_SESSION_ID" ]]; then
            print -rn -- "$__fmn_command_text" | command fmn _record-start \
                --shell zsh --session "$FMN_SESSION_ID" --cwd "$PWD" \
                >/dev/null 2>/dev/null
        fi
        return 0
    }

    __fmn_precmd() {
        local __fmn_exit_code=$?
        if [[ -n "$FMN_SESSION_ID" ]]; then
            command fmn _record-finish \
                --session "$FMN_SESSION_ID" --exit-code "$__fmn_exit_code" \
                >/dev/null 2>/dev/null
        fi
        return 0
    }

    add-zsh-hook preexec __fmn_preexec
    add-zsh-hook precmd __fmn_precmd

    __fmn_search_widget() {
        local __fmn_result
        local __fmn_search_status=0
        zle -I
        __fmn_result="$(command fmn search --query "$BUFFER" --cwd "$PWD" \
            --shell-integration 3>&1 1>&2 2>&3 0<&1 3>&-)" || __fmn_search_status=$?
        if (( __fmn_search_status == 0 || __fmn_search_status == 10 )); then
            BUFFER="$__fmn_result"
            CURSOR=${#BUFFER}
        elif [[ -n "$__fmn_result" ]]; then
            print -r -- "$__fmn_result" >&2
        fi
        zle reset-prompt
        if (( __fmn_search_status == 10 )); then
            zle accept-line
        fi
    }
    zle -N __fmn_search_widget
    bindkey '^R' __fmn_search_widget
    bindkey -M viins '^R' __fmn_search_widget
fi
