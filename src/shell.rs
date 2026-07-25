use crate::model::Shell;

pub fn init_script(shell: Shell) -> &'static str {
    match shell {
        Shell::Fish => FISH_INIT,
        Shell::Zsh => ZSH_INIT,
        Shell::Bash => BASH_INIT,
    }
}

const FISH_INIT: &str = r#"# fmn shell integration
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
        set -l __fmn_result "$(command fmn search --query (commandline) --cwd "$PWD")"
        set -l __fmn_search_status $status
        if test $__fmn_search_status -eq 0
            commandline --replace -- "$__fmn_result"
            commandline --cursor (string length -- "$__fmn_result")
        end
        commandline -f repaint
    end
    bind \cr __fmn_search
    bind -M insert \cr __fmn_search
end
"#;

const ZSH_INIT: &str = r#"# fmn shell integration
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
        zle -I
        __fmn_result="$(command fmn search --query "$BUFFER" --cwd "$PWD" \
            --shell-integration 3>&1 1>&2 2>&3 0<&1 3>&-)"
        local __fmn_search_status=$?
        if (( __fmn_search_status == 0 )); then
            BUFFER="$__fmn_result"
            CURSOR=${#BUFFER}
        elif [[ -n "$__fmn_result" ]]; then
            print -r -- "$__fmn_result" >&2
        fi
        zle reset-prompt
    }
    zle -N __fmn_search_widget
    bindkey '^R' __fmn_search_widget
    bindkey -M viins '^R' __fmn_search_widget
fi
"#;

const BASH_INIT: &str = r#"# fmn shell integration
if [[ $- == *i* && -z ${__FMN_INITIALIZED-} ]]; then
    __FMN_INITIALIZED=1
    export FMN_SESSION_ID="$(command fmn _session)"
    __fmn_ready=0
    __fmn_command_active=0
    __fmn_in_hook=0

    __fmn_preexec() {
        [[ $__fmn_in_hook -eq 0 && $__fmn_ready -eq 1 && $__fmn_command_active -eq 0 ]] || return 0
        __fmn_in_hook=1
        local __fmn_history_line
        local __fmn_command_text
        local HISTTIMEFORMAT=
        __fmn_history_line="$(builtin history 1)"
        if [[ $__fmn_history_line =~ ^[[:space:]]*[0-9]+[[:space:]]+(.*)$ ]]; then
            __fmn_command_text="${BASH_REMATCH[1]}"
        else
            __fmn_command_text="$BASH_COMMAND"
        fi
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
        __fmn_in_hook=1
        if [[ $__fmn_ready -eq 1 && $__fmn_command_active -eq 1 && -n $FMN_SESSION_ID ]]; then
            command fmn _record-finish \
                --session "$FMN_SESSION_ID" --exit-code "$__fmn_exit_code" \
                >/dev/null 2>/dev/null || true
        fi
        __fmn_command_active=0
        __fmn_ready=1
        __fmn_in_hook=0
        return "$__fmn_exit_code"
    }

    __fmn_search_widget() {
        local __fmn_result
        __fmn_result="$(command fmn search --query "$READLINE_LINE" --cwd "$PWD")"
        local __fmn_search_status=$?
        if [[ $__fmn_search_status -eq 0 ]]; then
            READLINE_LINE="$__fmn_result"
            READLINE_POINT=${#READLINE_LINE}
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

    if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]]; then
        PROMPT_COMMAND=(__fmn_precmd "${PROMPT_COMMAND[@]}")
    elif [[ -n ${PROMPT_COMMAND-} ]]; then
        PROMPT_COMMAND="__fmn_precmd; $PROMPT_COMMAND"
    else
        PROMPT_COMMAND=__fmn_precmd
    fi
    bind -x '"\C-r":__fmn_search_widget'
    bind -m vi-insertion -x '"\C-r":__fmn_search_widget'
fi
"#;

#[cfg(test)]
mod tests {
    use super::init_script;
    use crate::model::Shell;

    #[test]
    fn every_script_is_guarded_and_creates_a_session() {
        for shell in [Shell::Fish, Shell::Zsh, Shell::Bash] {
            let script = init_script(shell);
            assert!(script.contains("fmn _session"));
            assert!(script.contains("FMN_SESSION_ID"));
        }
        assert!(init_script(Shell::Fish).contains("status is-interactive"));
        assert!(init_script(Shell::Zsh).contains("-o interactive"));
        assert!(init_script(Shell::Bash).contains("$- == *i*"));
    }

    #[test]
    fn every_script_installs_start_and_finish_hooks() {
        for shell in [Shell::Fish, Shell::Zsh, Shell::Bash] {
            let script = init_script(shell);
            assert!(script.contains("fmn _record-start"));
            assert!(script.contains("fmn _record-finish"));
            assert!(script.contains("--exit-code"));
        }
    }

    #[test]
    fn capture_writes_commands_through_stdin() {
        assert!(init_script(Shell::Fish).contains("printf %s"));
        assert!(init_script(Shell::Zsh).contains("print -rn"));
        assert!(init_script(Shell::Bash).contains("printf %s"));
        for shell in [Shell::Fish, Shell::Zsh, Shell::Bash] {
            assert!(init_script(shell).contains("| command fmn _record-start"));
        }
    }

    #[test]
    fn bash_preserves_existing_prompt_and_debug_hooks() {
        let script = init_script(Shell::Bash);
        assert!(script.contains("trap -p DEBUG"));
        assert!(script.contains("$__fmn_saved_debug_handler"));
        assert!(script.contains("\"${PROMPT_COMMAND[@]}\""));
    }

    #[test]
    fn every_script_binds_ctrl_r_without_executing_selection() {
        for shell in [Shell::Fish, Shell::Zsh, Shell::Bash] {
            let script = init_script(shell);
            assert!(script.contains("fmn search --query"));
            assert!(script.contains("__fmn_search"));
        }
        assert!(init_script(Shell::Fish).contains("commandline --replace"));
        assert!(init_script(Shell::Zsh).contains("BUFFER=\"$__fmn_result\""));
        assert!(init_script(Shell::Bash).contains("READLINE_LINE=\"$__fmn_result\""));
    }
}
