#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
#  DivLens MCP — Binary Installation
#  Sourced by install.sh — not executed directly.
# ═══════════════════════════════════════════════════════════════════════════════

readonly DEFAULT_INSTALL_DIR="/usr/local/bin"
readonly LOCAL_INSTALL_DIR="$HOME/.local/bin"

install_binary() {
    step "Installing binary"

    local src="${TMP_DIR}/${BINARY_NAME}"

    # Strategy 1: Write directly to /usr/local/bin
    if [[ -w "$DEFAULT_INSTALL_DIR" ]]; then
        cp "$src" "${DEFAULT_INSTALL_DIR}/${BINARY_NAME}" || error_exit "Failed to copy binary."
        INSTALLED_PATH="${DEFAULT_INSTALL_DIR}/${BINARY_NAME}"
        ok "Installed to ${INSTALLED_PATH}"

    # Strategy 2: Use sudo
    elif command -v sudo &>/dev/null; then
        info "Writing to ${DEFAULT_INSTALL_DIR} requires sudo…"
        sudo mkdir -p "$DEFAULT_INSTALL_DIR" 2>/dev/null || true
        if sudo cp "$src" "${DEFAULT_INSTALL_DIR}/${BINARY_NAME}" 2>/dev/null; then
            INSTALLED_PATH="${DEFAULT_INSTALL_DIR}/${BINARY_NAME}"
            ok "Installed to ${INSTALLED_PATH} (via sudo)"
        else
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
    cp "$src" "${LOCAL_INSTALL_DIR}/${BINARY_NAME}" || error_exit "Failed to install."
    INSTALLED_PATH="${LOCAL_INSTALL_DIR}/${BINARY_NAME}"
    ok "Installed to ${INSTALLED_PATH}"

    if ! echo ":${PATH}:" | grep -q ":${LOCAL_INSTALL_DIR}:"; then
        warn "${LOCAL_INSTALL_DIR} is not in your PATH."
        info "Add this to your shell config (~/.zshrc or ~/.bashrc):"
        printf "  ${CYAN}export PATH=\"\$HOME/.local/bin:\$PATH\"${RESET}\n"
    fi
}
