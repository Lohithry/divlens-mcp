#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
#  DivLens MCP — Terminal UI Library
#  Sourced by install.sh — not executed directly.
#
#  Provides: colors, spinners, banners, print helpers
# ═══════════════════════════════════════════════════════════════════════════════

setup_colors() {
    if [[ -t 1 ]] && [[ "${NO_COLOR:-}" == "" ]] && [[ "${TERM:-dumb}" != "dumb" ]]; then
        BOLD="\033[1m";     DIM="\033[2m";       RESET="\033[0m"
        RED="\033[91m";     GREEN="\033[92m";     YELLOW="\033[93m"
        CYAN="\033[96m";    WHITE="\033[97m";     ORANGE="\033[38;5;208m"
    else
        BOLD=""; DIM=""; RESET=""; RED=""; GREEN=""
        YELLOW=""; CYAN=""; WHITE=""; ORANGE=""
    fi
}
setup_colors

# ─── Print helpers ────────────────────────────────────────────────────────────
line()    { printf "  ${DIM}%s${RESET}\n" "──────────────────────────────────────────────────"; }
ok()      { printf "  ${GREEN}✓${RESET}  %s\n" "$*"; }
warn()    { printf "  ${YELLOW}⚠${RESET}  ${YELLOW}%s${RESET}\n" "$*"; }
skip()    { printf "  ${DIM}○  %s${RESET}\n" "$*"; }
info()    { printf "  ${CYAN}→${RESET}  %s\n" "$*"; }
step()    { printf "\n  ${BOLD}${WHITE}%s${RESET}\n" "$*"; }

error_exit() {
    stop_spinner
    printf "\n  ${RED}${BOLD}✗ Error:${RESET} ${RED}%s${RESET}\n\n" "$*" >&2
    printf "  ${DIM}For help, visit: https://github.com/${REPO}/issues${RESET}\n\n" >&2
    cleanup
    exit 1
}

# ─── Spinner animation ────────────────────────────────────────────────────────
SPINNER_PID=""

start_spinner() {
    local msg="${1:-Working…}"
    stop_spinner
    if [[ -t 1 ]]; then
        _spinner_loop "$msg" &
        SPINNER_PID=$!
        disown "$SPINNER_PID" 2>/dev/null || true
    fi
}

_spinner_loop() {
    local msg="$1"
    local frames=('⠋' '⠙' '⠹' '⠸' '⠼' '⠴' '⠦' '⠧' '⠇' '⠏')
    local i=0
    tput civis 2>/dev/null || true
    while true; do
        printf "\r  ${CYAN}${frames[$i]}${RESET}  ${DIM}%s${RESET}   " "$msg"
        i=$(( (i + 1) % ${#frames[@]} ))
        sleep 0.08
    done
}

stop_spinner() {
    if [[ -n "$SPINNER_PID" ]] && kill -0 "$SPINNER_PID" 2>/dev/null; then
        kill "$SPINNER_PID" 2>/dev/null
        wait "$SPINNER_PID" 2>/dev/null || true
        SPINNER_PID=""
        printf "\r\033[K"
        tput cnorm 2>/dev/null || true
    fi
}

# ─── Banner ───────────────────────────────────────────────────────────────────
print_banner() {
    echo ""
    printf "${ORANGE}${BOLD}"
    printf "  ┌─────────────────────────────────────────┐\n"
    printf "  │                                         │\n"
    printf "  │   🔍  DivLens MCP  ·  Installer        │\n"
    printf "  │   Real-time system intelligence for AI │\n"
    printf "  │                                         │\n"
    printf "  └─────────────────────────────────────────┘\n"
    printf "${RESET}\n"
}

# ─── Success banner ──────────────────────────────────────────────────────────
print_success() {
    echo ""
    printf "  ${GREEN}${BOLD}"
    printf "  ┌─────────────────────────────────────────┐\n"
    printf "  │   ✅  DivLens MCP is ready!             │\n"
    printf "  └─────────────────────────────────────────┘\n"
    printf "${RESET}\n"

    info "Binary:    ${INSTALLED_PATH}"
    info "Version:   ${VERSION}"
    echo ""
    line

    printf "\n  ${BOLD}${WHITE}Next steps:${RESET}\n\n"
    printf "  1. ${BOLD}Restart${RESET} Claude Desktop or Cursor\n"
    printf "  2. ${BOLD}Ask:${RESET} ${CYAN}\"What's using my CPU right now?\"${RESET}\n"
    printf "       ${CYAN}\"Is my SSD healthy?\"${RESET}\n"
    printf "       ${CYAN}\"What's eating my disk space?\"${RESET}\n"
    echo ""

    printf "\n  ${BOLD}${WHITE}Management commands:${RESET}\n\n"
    printf "  ${CYAN}divlens-core status${RESET}           Show installation status\n"
    printf "  ${CYAN}divlens-core doctor${RESET}           Run health checks\n"
    printf "  ${CYAN}divlens-core config --show${RESET}    Show AI client configs\n"
    printf "  ${CYAN}divlens-core uninstall${RESET}        Remove DivLens completely\n"
    echo ""
    line
    echo ""
    info "Docs:   https://github.com/${REPO}"
    info "Issues: https://github.com/${REPO}/issues"
    echo ""
}
