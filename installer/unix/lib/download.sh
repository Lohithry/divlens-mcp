#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
#  DivLens MCP — Binary Download & Verification
#  Sourced by install.sh — not executed directly.
# ═══════════════════════════════════════════════════════════════════════════════

check_deps() {
    step "Checking dependencies"

    if command -v curl &>/dev/null; then
        DOWNLOADER="curl"
        ok "curl found"
    elif command -v wget &>/dev/null; then
        DOWNLOADER="wget"
        ok "wget found"
    else
        error_exit "Neither curl nor wget is installed."
    fi

    if command -v python3 &>/dev/null; then
        ok "Python 3 found (for config patching)"
        HAVE_PYTHON=true
    else
        warn "Python 3 not found — AI config auto-patching will be skipped."
        HAVE_PYTHON=false
    fi

    if command -v sha256sum &>/dev/null; then
        CHECKSUM_CMD="sha256sum"
    elif command -v shasum &>/dev/null; then
        CHECKSUM_CMD="shasum -a 256"
    else
        warn "No SHA-256 tool found. Checksum verification will be skipped."
        CHECKSUM_CMD=""
    fi
}

fetch_version() {
    step "Fetching latest version"
    start_spinner "Querying GitHub API…"

    local response
    if [[ "$DOWNLOADER" == "curl" ]]; then
        response=$(curl -fsSL --max-time 15 \
            -H "Accept: application/vnd.github+json" \
            "$GITHUB_API" 2>/dev/null) || {
            stop_spinner
            error_exit "Could not reach GitHub API. Check your internet connection."
        }
    else
        response=$(wget -qO- --timeout=15 \
            --header="Accept: application/vnd.github+json" \
            "$GITHUB_API" 2>/dev/null) || {
            stop_spinner
            error_exit "Could not reach GitHub API."
        }
    fi

    VERSION=$(printf '%s' "$response" | grep -o '"tag_name": *"[^"]*"' | head -1 | sed 's/.*"tag_name": *"\(.*\)"/\1/')

    stop_spinner
    if [[ -z "$VERSION" ]]; then
        error_exit "Could not parse latest release version."
    fi

    ok "Latest version: ${BOLD}${VERSION}${RESET}"

    # Check if already up to date
    if command -v "$BINARY_NAME" &>/dev/null; then
        local current_version
        current_version=$("$BINARY_NAME" --version 2>/dev/null | grep -o '[0-9]\+\.[0-9]\+\.[0-9]\+' | head -1) || true
        if [[ -n "$current_version" ]] && [[ "v${current_version}" == "$VERSION" ]]; then
            stop_spinner
            printf "\n"
            ok "DivLens MCP ${VERSION} is already installed and up to date."
            info "Run with --force to reinstall."
            exit 0
        elif [[ -n "$current_version" ]]; then
            info "Upgrading from v${current_version} → ${VERSION}"
        fi
    fi
}

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
                error_exit "Download failed. URL: ${binary_url}"
        else
            curl -fsSL "$binary_url" -o "$dest" 2>/dev/null || \
                error_exit "Download failed. URL: ${binary_url}"
            stop_spinner
        fi
    else
        wget -q --show-progress "$binary_url" -O "$dest" 2>&1 || {
            stop_spinner
            error_exit "Download failed. URL: ${binary_url}"
        }
        stop_spinner
    fi

    if [[ ! -f "$dest" ]] || [[ ! -s "$dest" ]]; then
        error_exit "Downloaded file is empty or missing."
    fi

    ok "Downloaded: ${ARTIFACT}"

    # ── Verify checksum ────────────────────────────────────────────────────────
    if [[ -n "$CHECKSUM_CMD" ]]; then
        start_spinner "Verifying checksum…"
        local sha_file="${TMP_DIR}/${BINARY_NAME}.sha256"
        if [[ "$DOWNLOADER" == "curl" ]]; then
            curl -fsSL "$sha_url" -o "$sha_file" 2>/dev/null || {
                stop_spinner; warn "Checksum file not found — skipping."; sha_file=""; }
        else
            wget -qO "$sha_file" "$sha_url" 2>/dev/null || {
                stop_spinner; warn "Checksum file not found — skipping."; sha_file=""; }
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
                error_exit "Checksum FAILED!\n  Expected: ${expected}\n  Got:      ${actual}"
            fi
            ok "Checksum verified ✓"
        fi
    fi

    chmod +x "$dest"
}
