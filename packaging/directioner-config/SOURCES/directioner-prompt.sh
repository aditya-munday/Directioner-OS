#!/bin/bash
# Directioner OS — Bash prompt (TODO 064)
# Minimal branded prompt. No heavy frameworks (oh-my-zsh, starship etc. not forced).
# Users may override by setting PS1 in ~/.bashrc.

# Only set if interactive and not already set by user
if [[ -z "${DIRECTIONER_PROMPT_SET:-}" && $- == *i* ]]; then
    export DIRECTIONER_PROMPT_SET=1

    # Color codes
    _DOS_CYAN='\[\033[0;36m\]'
    _DOS_BLUE='\[\033[0;34m\]'
    _DOS_GREEN='\[\033[0;32m\]'
    _DOS_RED='\[\033[0;31m\]'
    _DOS_BOLD='\[\033[1m\]'
    _DOS_RESET='\[\033[0m\]'

    # Git branch indicator (only if git available and in a repo)
    _dos_git_branch() {
        local branch
        branch=$(git symbolic-ref --short HEAD 2>/dev/null) || \
        branch=$(git describe --tags --exact-match 2>/dev/null) || \
        branch=$(git rev-parse --short HEAD 2>/dev/null)
        [[ -n "$branch" ]] && echo " ${_DOS_BLUE}(${branch})${_DOS_RESET}"
    }

    # Exit code indicator
    _dos_exit_indicator() {
        local code=$?
        if [[ $code -eq 0 ]]; then
            echo "${_DOS_GREEN}❯${_DOS_RESET}"
        else
            echo "${_DOS_RED}❯${_DOS_RESET}"
        fi
    }

    # Prompt: [dos] user@host:~/path (branch) ❯
    PROMPT_COMMAND='_DOS_EXIT=$?'
    PS1="${_DOS_CYAN}${_DOS_BOLD}[dos]${_DOS_RESET} "
    PS1+="${_DOS_GREEN}\u@\h${_DOS_RESET}:${_DOS_CYAN}\w${_DOS_RESET}"
    PS1+="\$(_dos_git_branch) "
    PS1+="\$([ \$_DOS_EXIT -eq 0 ] && echo '${_DOS_GREEN}❯${_DOS_RESET}' || echo '${_DOS_RED}❯${_DOS_RESET}') "

    export PS1
fi
