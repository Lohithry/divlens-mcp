#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
#  DivLens MCP — Platform Detection
#  Sourced by install.sh — not executed directly.
# ═══════════════════════════════════════════════════════════════════════════════

detect_platform() {
    step "Detecting platform"

    local os arch
    os=$(uname -s 2>/dev/null | tr '[:upper:]' '[:lower:]') || error_exit "Cannot detect OS."
    arch=$(uname -m 2>/dev/null) || error_exit "Cannot detect architecture."

    # Detect macOS running Intel binary on Apple Silicon via Rosetta 2
    if [[ "$os" == "darwin" ]] && [[ "$arch" == "x86_64" ]]; then
        if sysctl -n sysctl.proc_translated 2>/dev/null | grep -q '^1$'; then
            arch="arm64"
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
                    error_exit "Unsupported macOS architecture: ${arch}." ;;
            esac ;;
        linux)
            case "$arch" in
                x86_64|amd64)
                    ARTIFACT="divlens-core-x86_64-linux-gnu"
                    PLATFORM_LABEL="Linux x86_64" ;;
                arm64|aarch64)
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
