#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
#  DivLens MCP — Smart Installer for macOS and Linux
#  https://github.com/Lohithry/divlens-mcp
#
#  Usage:
#    curl -fsSL https://raw.githubusercontent.com/Lohithry/divlens-mcp/main/install.sh | bash
#
#  Supported platforms:
#    macOS Apple Silicon (arm64)  |  macOS Intel (x86_64)
#    Linux x86_64                 |  Linux arm64
#
#  Configures: Claude Desktop · Cursor · Windsurf · Antigravity
# ═══════════════════════════════════════════════════════════════════════════════

set -uo pipefail   # Do NOT use -e here — we handle errors explicitly

# ─── Constants ────────────────────────────────────────────────────────────────
readonly REPO="Lohithry/divlens-mcp"
readonly BINARY_NAME="divlens-core"
readonly DEFAULT_INSTALL_DIR="/usr/local/bin"
readonly LOCAL_INSTALL_DIR="$HOME/.local/bin"
readonly GITHUB_API="https://api.github.com/repos/${REPO}/releases/latest"

# ─── State ────────────────────────────────────────────────────────────────────
INSTALLED_PATH=""
VERSION=""
PLATFORM_LABEL=""
ARTIFACT=""
TMP_DIR=""
SPINNER_PID=""

# ─── Terminal colours (only when stdout is a real terminal) ───────────────────
setup_colors() {
    if [[ -t 1 ]] && [[ "${NO_COLOR:-}" == "" ]] && [[ "${TERM:-dumb}" != "dumb" ]]; then
        BOLD="\033[1m";     DIM="\033[2m";       RESET="\033[0m"
        RED="\033[91m";     GREEN="\033[92m";     YELLOW="\033[93m"
        CYAN="\033[96m";    WHITE="\033[97m";     ORANGE="\033[38;5;208m"
        BLUE="\033[94m";    MAGENTA="\033[95m"
    else
        BOLD=""; DIM=""; RESET=""; RED=""; GREEN=""
        YELLOW=""; CYAN=""; WHITE=""; ORANGE=""; BLUE=""; MAGENTA=""
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

# ─── Cleanup ──────────────────────────────────────────────────────────────────
cleanup() {
    stop_spinner
    if [[ -n "$TMP_DIR" ]] && [[ -d "$TMP_DIR" ]]; then
        rm -rf "$TMP_DIR"
    fi
}
trap 'cleanup' EXIT INT TERM

# ─── Spinner animation ────────────────────────────────────────────────────────
start_spinner() {
    local msg="${1:-Working…}"
    stop_spinner  # Stop any existing spinner
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
    tput civis 2>/dev/null || true  # Hide cursor
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
        printf "\r\033[K"  # Clear spinner line
        tput cnorm 2>/dev/null || true  # Restore cursor
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

# ─── OS + Architecture detection ─────────────────────────────────────────────
detect_platform() {
    step "Detecting platform"

    local os arch
    os=$(uname -s 2>/dev/null | tr '[:upper:]' '[:lower:]') || error_exit "Cannot detect OS."
    arch=$(uname -m 2>/dev/null) || error_exit "Cannot detect architecture."

    # Detect macOS running Intel binary on Apple Silicon via Rosetta 2
    if [[ "$os" == "darwin" ]] && [[ "$arch" == "x86_64" ]]; then
        if sysctl -n sysctl.proc_translated 2>/dev/null | grep -q '^1$'; then
            arch="arm64"  # Running under Rosetta — use native arm64 binary
        fi
    fi

    case "$os" in
        darwin)
            case "$arch" in
                arm64|aarch64)
                    ARTIFACT="divlens-core-aarch64-apple-darwin"
                    PLATFORM_LABEL="macOS Apple Silicon (arm64)" ;;
                x86_64|amd64)
                    ARTIFACT="divlens-core-x86_64-apple-darwin"
                    PLATFORM_LABEL="macOS Intel (x86_64)" ;;
                *)
                    error_exit "Unsupported macOS architecture: ${arch}. Please open an issue." ;;
            esac ;;
        linux)
            case "$arch" in
                x86_64|amd64)
                    ARTIFACT="divlens-core-x86_64-linux-gnu"
                    PLATFORM_LABEL="Linux x86_64" ;;
                arm64|aarch64)
                    # Note: Linux arm64 musl binary if available, else error gracefully
                    ARTIFACT="divlens-core-x86_64-linux-gnu"
                    PLATFORM_LABEL="Linux arm64 (using x86_64 build)"
                    warn "Native arm64 Linux binary not yet available — using x86_64 build via emulation." ;;
                *)
                    error_exit "Unsupported Linux architecture: ${arch}." ;;
            esac ;;
        *)
            error_exit "Unsupported OS: ${os}. Use the Windows installer (install.ps1) on Windows." ;;
    esac

    ok "Platform: ${PLATFORM_LABEL}"
}

# ─── Dependency checks ────────────────────────────────────────────────────────
check_deps() {
    step "Checking dependencies"

    # Need curl or wget for downloading
    if command -v curl &>/dev/null; then
        DOWNLOADER="curl"
        ok "curl found"
    elif command -v wget &>/dev/null; then
        DOWNLOADER="wget"
        ok "wget found"
    else
        error_exit "Neither curl nor wget is installed. Please install curl:\n  macOS: brew install curl\n  Linux: apt/yum/dnf install curl"
    fi

    # Need python3 for safe JSON patching of AI config files
    if command -v python3 &>/dev/null; then
        ok "Python 3 found (for config patching)"
        HAVE_PYTHON=true
    else
        warn "Python 3 not found — AI config auto-patching will be skipped."
        warn "You will need to configure your AI client manually."
        HAVE_PYTHON=false
    fi

    # Need sha256sum or shasum for checksum verification
    if command -v sha256sum &>/dev/null; then
        CHECKSUM_CMD="sha256sum"
    elif command -v shasum &>/dev/null; then
        CHECKSUM_CMD="shasum -a 256"
    else
        warn "No SHA-256 tool found (sha256sum / shasum). Checksum verification will be skipped."
        CHECKSUM_CMD=""
    fi
}

# ─── Fetch latest version from GitHub API ────────────────────────────────────
fetch_version() {
    step "Fetching latest version"
    start_spinner "Querying GitHub API…"

    local response
    if [[ "$DOWNLOADER" == "curl" ]]; then
        response=$(curl -fsSL --max-time 15 \
            -H "Accept: application/vnd.github+json" \
            "$GITHUB_API" 2>/dev/null) || {
            stop_spinner
            error_exit "Could not reach GitHub API. Check your internet connection.\n  URL: ${GITHUB_API}"
        }
    else
        response=$(wget -qO- --timeout=15 \
            --header="Accept: application/vnd.github+json" \
            "$GITHUB_API" 2>/dev/null) || {
            stop_spinner
            error_exit "Could not reach GitHub API. Check your internet connection."
        }
    fi

    # Extract tag_name robustly (no jq dependency)
    VERSION=$(printf '%s' "$response" | grep -o '"tag_name": *"[^"]*"' | head -1 | sed 's/.*"tag_name": *"\(.*\)"/\1/')

    stop_spinner
    if [[ -z "$VERSION" ]]; then
        error_exit "Could not parse latest release version. GitHub API may be rate-limited.\n  Try again in a minute or download manually:\n  https://github.com/${REPO}/releases/latest"
    fi

    ok "Latest version: ${BOLD}${VERSION}${RESET}"

    # Check if already installed and up to date
    if command -v "$BINARY_NAME" &>/dev/null; then
        local current_version
        current_version=$("$BINARY_NAME" --version 2>/dev/null | grep -o '[0-9]\+\.[0-9]\+\.[0-9]\+' | head -1) || true
        if [[ -n "$current_version" ]] && [[ "v${current_version}" == "$VERSION" ]]; then
            stop_spinner
            printf "\n"
            ok "DivLens MCP ${VERSION} is already installed and up to date."
            info "Run with --force to reinstall: curl -fsSL ... | bash -s -- --force"
            exit 0
        elif [[ -n "$current_version" ]]; then
            info "Upgrading from v${current_version} → ${VERSION}"
        fi
    fi
}

# ─── Download binary ──────────────────────────────────────────────────────────
download_binary() {
    step "Downloading DivLens MCP"

    TMP_DIR=$(mktemp -d) || error_exit "Could not create temporary directory."

    local base_url="https://github.com/${REPO}/releases/download/${VERSION}"
    local binary_url="${base_url}/${ARTIFACT}"
    local sha_url="${binary_url}.sha256"
    local dest="${TMP_DIR}/${BINARY_NAME}"

    info "Source: ${binary_url}"

    start_spinner "Downloading ${ARTIFACT}…"

    if [[ "$DOWNLOADER" == "curl" ]]; then
        if [[ -t 1 ]]; then
            stop_spinner
            curl -fL --progress-bar "$binary_url" -o "$dest" 2>&1 || \
                error_exit "Download failed. Check your internet connection.\n  URL: ${binary_url}"
        else
            curl -fsSL "$binary_url" -o "$dest" 2>/dev/null || \
                error_exit "Download failed.\n  URL: ${binary_url}"
            stop_spinner
        fi
    else
        wget -q --show-progress "$binary_url" -O "$dest" 2>&1 || {
            stop_spinner
            error_exit "Download failed.\n  URL: ${binary_url}"
        }
        stop_spinner
    fi

    # Verify the file was actually downloaded and is non-empty
    if [[ ! -f "$dest" ]] || [[ ! -s "$dest" ]]; then
        error_exit "Downloaded file is empty or missing. The release asset may not exist yet."
    fi

    ok "Downloaded: ${ARTIFACT}"

    # ── Verify checksum ────────────────────────────────────────────────────────
    if [[ -n "$CHECKSUM_CMD" ]]; then
        start_spinner "Verifying checksum…"

        local sha_file="${TMP_DIR}/${BINARY_NAME}.sha256"
        if [[ "$DOWNLOADER" == "curl" ]]; then
            curl -fsSL "$sha_url" -o "$sha_file" 2>/dev/null || {
                stop_spinner
                warn "Checksum file not found at ${sha_url} — skipping verification."
                sha_file=""
            }
        else
            wget -qO "$sha_file" "$sha_url" 2>/dev/null || {
                stop_spinner
                warn "Checksum file not found — skipping verification."
                sha_file=""
            }
        fi

        if [[ -n "$sha_file" ]] && [[ -f "$sha_file" ]]; then
            cd "$TMP_DIR"
            local expected actual
            expected=$(awk '{print $1}' "$sha_file" | tr '[:upper:]' '[:lower:]')
            if [[ "$CHECKSUM_CMD" == "sha256sum" ]]; then
                actual=$(sha256sum "$BINARY_NAME" | awk '{print $1}' | tr '[:upper:]' '[:lower:]')
            else
                actual=$(shasum -a 256 "$BINARY_NAME" | awk '{print $1}' | tr '[:upper:]' '[:lower:]')
            fi
            cd - &>/dev/null

            stop_spinner
            if [[ "$expected" != "$actual" ]]; then
                error_exit "Checksum verification FAILED.\n  Expected: ${expected}\n  Got:      ${actual}\n  The download may be corrupted. Please try again."
            fi
            ok "Checksum verified ✓"
        fi
    fi

    chmod +x "$dest"
}

# ─── Install binary ───────────────────────────────────────────────────────────
install_binary() {
    step "Installing binary"

    local src="${TMP_DIR}/${BINARY_NAME}"
    local dest=""

    # Strategy 1: Write directly to /usr/local/bin (most common case)
    if [[ -w "$DEFAULT_INSTALL_DIR" ]]; then
        dest="${DEFAULT_INSTALL_DIR}/${BINARY_NAME}"
        cp "$src" "$dest" || error_exit "Failed to copy binary to ${dest}"
        INSTALLED_PATH="$dest"
        ok "Installed to ${dest}"

    # Strategy 2: Use sudo (passwordless or with prompt)
    elif command -v sudo &>/dev/null; then
        info "Writing to ${DEFAULT_INSTALL_DIR} requires sudo…"
        sudo mkdir -p "$DEFAULT_INSTALL_DIR" 2>/dev/null || true
        if sudo cp "$src" "${DEFAULT_INSTALL_DIR}/${BINARY_NAME}" 2>/dev/null; then
            INSTALLED_PATH="${DEFAULT_INSTALL_DIR}/${BINARY_NAME}"
            ok "Installed to ${INSTALLED_PATH} (via sudo)"
        else
            # Strategy 3: Fallback to ~/.local/bin (no root needed)
            warn "sudo failed — falling back to ${LOCAL_INSTALL_DIR}"
            _install_to_local "$src"
        fi

    # Strategy 3: Fallback to ~/.local/bin
    else
        _install_to_local "$src"
    fi
}

_install_to_local() {
    local src="$1"
    mkdir -p "$LOCAL_INSTALL_DIR" || error_exit "Cannot create ${LOCAL_INSTALL_DIR}"
    cp "$src" "${LOCAL_INSTALL_DIR}/${BINARY_NAME}" || error_exit "Failed to install binary."
    INSTALLED_PATH="${LOCAL_INSTALL_DIR}/${BINARY_NAME}"
    ok "Installed to ${INSTALLED_PATH}"

    # Check if LOCAL_INSTALL_DIR is in PATH
    if ! echo ":${PATH}:" | grep -q ":${LOCAL_INSTALL_DIR}:"; then
        warn "${LOCAL_INSTALL_DIR} is not in your PATH."
        echo ""
        info "Add this to your shell config (~/.zshrc or ~/.bashrc):"
        printf "  ${CYAN}export PATH=\"\$HOME/.local/bin:\$PATH\"${RESET}\n"
        echo ""
    fi
}

# ─── AI client config patching ────────────────────────────────────────────────
#
#  Uses Python 3 to safely parse and patch JSON.
#  NEVER modifies configs with broken JSON without creating a backup first.
#  Creates config files from scratch if they don't exist.
# ─────────────────────────────────────────────────────────────────────────────

_patch_config() {
    local client_name="$1"
    local config_path="$2"
    local config_dir

    config_dir=$(dirname "$config_path")

    # Check if the client is installed by looking for its config directory parent
    # (We check the parent of the dir, not the config file itself, since it may not exist yet)
    # We rely on heuristics: if the app data folder exists, client is likely installed
    if [[ ! -d "$config_dir" ]]; then
        skip "${client_name}: not detected (${config_dir} not found)"
        return 0
    fi

    if [[ ! "$HAVE_PYTHON" == "true" ]]; then
        warn "${client_name}: skipped (Python 3 required for safe JSON patching)"
        return 0
    fi

    # Patch the config using an embedded Python script
    local result
    result=$(python3 - "$config_path" "$INSTALLED_PATH" "$client_name" <<'PYEOF'
import json, sys, os, shutil, re

config_path  = sys.argv[1]
binary_path  = sys.argv[2]
client_name  = sys.argv[3]

def ensure_dir(path):
    os.makedirs(os.path.dirname(path), exist_ok=True)

def load_config(path):
    if not os.path.exists(path):
        return {}
    with open(path, 'r', encoding='utf-8') as f:
        raw = f.read().strip()
    if not raw:
        return {}
    try:
        return json.loads(raw)
    except (json.JSONDecodeError, ValueError):
        return None  # Signal: file exists but is malformed

def write_config(path, config):
    ensure_dir(path)
    with open(path, 'w', encoding='utf-8') as f:
        json.dump(config, f, indent=2, ensure_ascii=False)
        f.write('\n')

ensure_dir(config_path)
config = load_config(config_path)

# Handle malformed JSON — backup and start fresh
if config is None:
    backup_path = config_path + '.divlens.bak'
    shutil.copy2(config_path, backup_path)
    print(f'WARN:Backed up malformed config to {backup_path}')
    config = {}

# Backup healthy config before modifying
elif os.path.exists(config_path):
    shutil.copy2(config_path, config_path + '.divlens.bak')

# Ensure mcpServers key exists
if 'mcpServers' not in config or not isinstance(config.get('mcpServers'), dict):
    config['mcpServers'] = {}

# Check if already configured correctly
existing = config['mcpServers'].get('divlens', {})
if existing.get('command') == binary_path and existing.get('args') == ['--mcp']:
    print('SKIP:already configured correctly')
    sys.exit(0)

# Add / update divlens entry
config['mcpServers']['divlens'] = {
    'command': binary_path,
    'args': ['--mcp']
}

write_config(config_path, config)
print('OK')
PYEOF
    2>&1) || {
        warn "${client_name}: config patch failed — see error above"
        return 0
    }

    # Parse result lines from Python script
    while IFS= read -r res_line; do
        case "$res_line" in
            OK)
                ok "${client_name}: config updated (${config_path})" ;;
            SKIP:*)
                ok "${client_name}: already configured ✓" ;;
            WARN:*)
                warn "${client_name}: ${res_line#WARN:}" ;;
            *)
                ;;
        esac
    done <<< "$result"
}

configure_ai_clients() {
    step "Connecting to AI clients"

    local os
    os=$(uname -s | tr '[:upper:]' '[:lower:]')

    if [[ "$os" == "darwin" ]]; then
        # ── macOS paths ──────────────────────────────────────────────────────
        local claude_config="$HOME/Library/Application Support/Claude/claude_desktop_config.json"
        local cursor_config="$HOME/.cursor/mcp.json"
        local windsurf_config="$HOME/.codeium/windsurf/mcp_config.json"
        local antigravity_config="$HOME/.gemini/mcp_config.json"
    else
        # ── Linux paths ──────────────────────────────────────────────────────
        local claude_config="$HOME/.config/Claude/claude_desktop_config.json"
        # Some Linux Claude Desktop installs use this path too
        if [[ ! -d "$HOME/.config/Claude" ]] && [[ -d "$HOME/.var/app/com.anthropic.Claude" ]]; then
            claude_config="$HOME/.var/app/com.anthropic.Claude/config/Claude/claude_desktop_config.json"
        fi
        local cursor_config="$HOME/.cursor/mcp.json"
        local windsurf_config="$HOME/.codeium/windsurf/mcp_config.json"
        local antigravity_config="$HOME/.gemini/mcp_config.json"
    fi

    _patch_config "Claude Desktop"  "$claude_config"
    _patch_config "Cursor"          "$cursor_config"
    _patch_config "Windsurf"        "$windsurf_config"
    _patch_config "Antigravity"     "$antigravity_config"
}

# ─── Verify the server actually works ────────────────────────────────────────
verify_server() {
    step "Verifying installation"
    start_spinner "Testing MCP server…"

    local test_payload='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"installer-test","version":"1.0"}}}'
    local response

    response=$(echo "$test_payload" | \
        timeout 10 "$INSTALLED_PATH" --mcp 2>/dev/null | head -1) || {
        stop_spinner
        warn "Server test timed out. The binary installed correctly — it may need a restart."
        return 0
    }

    stop_spinner

    if echo "$response" | grep -q '"result"'; then
        ok "Server test passed — MCP protocol confirmed working"
    else
        warn "Server responded but response was unexpected. The binary is installed."
        info "Response: ${response:0:120}…"
    fi
}

# ─── Print success banner ─────────────────────────────────────────────────────
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

    printf "  ${BOLD}${WHITE}Management:${RESET}\n\n"
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

# ─── Main ─────────────────────────────────────────────────────────────────────
main() {
    print_banner
    detect_platform
    check_deps
    fetch_version
    download_binary
    install_binary
    configure_ai_clients
    verify_server
    print_success
}

main "$@"
