#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
#  DivLens MCP — Modular Installer for macOS and Linux
#  https://github.com/Lohithry/divlens-mcp
#
#  This is the modular version of the installer. The root install.sh is
#  assembled from these modules for the one-liner install experience.
#
#  Usage (development):
#    cd installer/unix && bash install.sh
# ═══════════════════════════════════════════════════════════════════════════════

set -uo pipefail

# ─── Constants ────────────────────────────────────────────────────────────────
readonly REPO="Lohithry/divlens-mcp"
readonly BINARY_NAME="divlens-core"
readonly GITHUB_API="https://api.github.com/repos/${REPO}/releases/latest"

# ─── State ────────────────────────────────────────────────────────────────────
INSTALLED_PATH=""
VERSION=""
PLATFORM_LABEL=""
ARTIFACT=""
TMP_DIR=""

# ─── Resolve script directory ─────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ─── Source modules ───────────────────────────────────────────────────────────
source "${SCRIPT_DIR}/lib/ui.sh"
source "${SCRIPT_DIR}/lib/platform.sh"
source "${SCRIPT_DIR}/lib/download.sh"
source "${SCRIPT_DIR}/lib/install.sh"
source "${SCRIPT_DIR}/lib/configure.sh"
source "${SCRIPT_DIR}/lib/service.sh"

# ─── Cleanup ──────────────────────────────────────────────────────────────────
cleanup() {
    stop_spinner
    if [[ -n "${TMP_DIR:-}" ]] && [[ -d "$TMP_DIR" ]]; then
        rm -rf "$TMP_DIR"
    fi
}
trap 'cleanup' EXIT INT TERM

# ─── Verify server ────────────────────────────────────────────────────────────
verify_server() {
    step "Verifying installation"
    start_spinner "Testing MCP server…"

    local test_payload='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"installer-test","version":"1.0"}}}'
    local response

    response=$(echo "$test_payload" | \
        timeout 10 "$INSTALLED_PATH" --mcp 2>/dev/null | head -1) || {
        stop_spinner
        warn "Server test timed out — restart your AI client to activate."
        return 0
    }

    stop_spinner
    if echo "$response" | grep -q '"result"'; then
        ok "Server test passed — MCP protocol confirmed working"
    else
        warn "Server responded but response was unexpected."
    fi
}

# ─── Main ─────────────────────────────────────────────────────────────────────
main() {
    print_banner
    detect_platform
    check_deps
    fetch_version
    download_binary
    install_binary
    configure_ai_clients
    register_service
    verify_server
    print_success
}

main "$@"
