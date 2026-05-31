#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
#  DivLens MCP — AI Client Configuration
#  Sourced by install.sh — not executed directly.
#
#  Uses embedded Python 3 for safe JSON patching. Never corrupts existing
#  config files — creates backups before every modification.
# ═══════════════════════════════════════════════════════════════════════════════

_patch_config() {
    local client_name="$1"
    local config_path="$2"
    local config_dir

    config_dir=$(dirname "$config_path")

    if [[ ! -d "$config_dir" ]]; then
        skip "${client_name}: not detected (${config_dir} not found)"
        return 0
    fi

    if [[ ! "$HAVE_PYTHON" == "true" ]]; then
        warn "${client_name}: skipped (Python 3 required for safe JSON patching)"
        return 0
    fi

    local result
    result=$(python3 - "$config_path" "$INSTALLED_PATH" "$client_name" <<'PYEOF'
import json, sys, os, shutil

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
    # Strip BOM if present
    if raw.startswith('\ufeff'):
        raw = raw[1:]
    try:
        return json.loads(raw)
    except (json.JSONDecodeError, ValueError):
        return None

def write_config(path, config):
    ensure_dir(path)
    with open(path, 'w', encoding='utf-8') as f:
        json.dump(config, f, indent=2, ensure_ascii=False)
        f.write('\n')

ensure_dir(config_path)
config = load_config(config_path)

if config is None:
    backup_path = config_path + '.divlens.bak'
    shutil.copy2(config_path, backup_path)
    print(f'WARN:Backed up malformed config to {backup_path}')
    config = {}
elif os.path.exists(config_path):
    shutil.copy2(config_path, config_path + '.divlens.bak')

if 'mcpServers' not in config or not isinstance(config.get('mcpServers'), dict):
    config['mcpServers'] = {}

existing = config['mcpServers'].get('divlens', {})
if existing.get('command') == binary_path and existing.get('args') == ['--mcp']:
    print('SKIP:already configured correctly')
    sys.exit(0)

config['mcpServers']['divlens'] = {
    'command': binary_path,
    'args': ['--mcp']
}

write_config(config_path, config)
print('OK')
PYEOF
    2>&1) || {
        warn "${client_name}: config patch failed"
        return 0
    }

    while IFS= read -r res_line; do
        case "$res_line" in
            OK)     ok "${client_name}: config updated (${config_path})" ;;
            SKIP:*) ok "${client_name}: already configured ✓" ;;
            WARN:*) warn "${client_name}: ${res_line#WARN:}" ;;
        esac
    done <<< "$result"
}

configure_ai_clients() {
    step "Connecting to AI clients"

    local os
    os=$(uname -s | tr '[:upper:]' '[:lower:]')

    if [[ "$os" == "darwin" ]]; then
        _patch_config "Claude Desktop"  "$HOME/Library/Application Support/Claude/claude_desktop_config.json"
        _patch_config "Cursor"          "$HOME/.cursor/mcp.json"
        _patch_config "Windsurf"        "$HOME/.codeium/windsurf/mcp_config.json"
        _patch_config "Antigravity"     "$HOME/.gemini/mcp_config.json"
    else
        local claude_config="$HOME/.config/Claude/claude_desktop_config.json"
        if [[ ! -d "$HOME/.config/Claude" ]] && [[ -d "$HOME/.var/app/com.anthropic.Claude" ]]; then
            claude_config="$HOME/.var/app/com.anthropic.Claude/config/Claude/claude_desktop_config.json"
        fi
        _patch_config "Claude Desktop"  "$claude_config"
        _patch_config "Cursor"          "$HOME/.cursor/mcp.json"
        _patch_config "Windsurf"        "$HOME/.codeium/windsurf/mcp_config.json"
        _patch_config "Antigravity"     "$HOME/.gemini/mcp_config.json"
    fi
}
