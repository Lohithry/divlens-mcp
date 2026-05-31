#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
#  DivLens MCP — Service Registration
#  Sourced by install.sh — not executed directly.
#
#  macOS: ~/Library/LaunchAgents/io.divlens.mcp.plist
#  Linux: ~/.config/systemd/user/divlens-mcp.service
# ═══════════════════════════════════════════════════════════════════════════════

register_service() {
    step "Service registration (optional)"

    local os
    os=$(uname -s | tr '[:upper:]' '[:lower:]')

    case "$os" in
        darwin) _register_launchd ;;
        linux)  _register_systemd ;;
        *)      skip "Service registration not supported on ${os}" ;;
    esac
}

_register_launchd() {
    local plist_dir="$HOME/Library/LaunchAgents"
    local plist_path="$plist_dir/io.divlens.mcp.plist"

    mkdir -p "$plist_dir" 2>/dev/null || true

    cat > "$plist_path" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>io.divlens.mcp</string>
    <key>ProgramArguments</key>
    <array>
        <string>${INSTALLED_PATH}</string>
        <string>--mcp</string>
    </array>
    <key>KeepAlive</key>
    <false/>
    <key>RunAtLoad</key>
    <false/>
    <key>StandardErrorPath</key>
    <string>/tmp/divlens-mcp.err.log</string>
</dict>
</plist>
PLIST

    ok "launchd service registered: ${plist_path}"
    skip "Service set to on-demand (not auto-start) — AI clients spawn it automatically"
}

_register_systemd() {
    local service_dir="$HOME/.config/systemd/user"
    local service_path="$service_dir/divlens-mcp.service"

    mkdir -p "$service_dir" 2>/dev/null || true

    cat > "$service_path" << UNIT
[Unit]
Description=DivLens MCP Server — System Intelligence for AI
Documentation=https://github.com/Lohithry/divlens-mcp

[Service]
Type=simple
ExecStart=${INSTALLED_PATH} --mcp
Restart=on-failure
RestartSec=5
StandardOutput=null
StandardError=journal

[Install]
WantedBy=default.target
UNIT

    systemctl --user daemon-reload 2>/dev/null || true
    ok "systemd user service registered: ${service_path}"
    info "Enable auto-start: systemctl --user enable divlens-mcp"
}
