#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════════
#  DivLens MCP — Uninstaller for macOS and Linux
#  https://github.com/Lohithry/divlens-mcp
#
#  Usage:
#    divlens-core uninstall          # Interactive (recommended)
#    bash uninstall.sh               # Standalone script
#    bash uninstall.sh --force       # Skip confirmation
# ═══════════════════════════════════════════════════════════════════════════════

set -uo pipefail

FORCE=false
[[ "${1:-}" == "--force" ]] && FORCE=true

# ─── Colors ───────────────────────────────────────────────────────────────────
if [[ -t 1 ]] && [[ "${NO_COLOR:-}" == "" ]]; then
    G="\033[92m"; Y="\033[93m"; R="\033[91m"; C="\033[96m"
    B="\033[1m"; D="\033[2m"; RST="\033[0m"
else
    G=""; Y=""; R=""; C=""; B=""; D=""; RST=""
fi

ok()   { printf "  ${G}✓${RST}  %s\n" "$*"; }
warn() { printf "  ${Y}⚠${RST}  ${Y}%s${RST}\n" "$*"; }
fail() { printf "  ${R}✗${RST}  ${R}%s${RST}\n" "$*"; }
info() { printf "  ${C}→${RST}  %s\n" "$*"; }

echo ""
printf "  ${B}🗑️  DivLens Uninstaller${RST}\n\n"

# ─── Find installed binary ────────────────────────────────────────────────────
BINARY=$(command -v divlens-core 2>/dev/null || echo "")
if [[ -z "$BINARY" ]]; then
    for p in /usr/local/bin/divlens-core "$HOME/.local/bin/divlens-core"; do
        [[ -f "$p" ]] && BINARY="$p" && break
    done
fi

# ─── Data directory ───────────────────────────────────────────────────────────
os=$(uname -s | tr '[:upper:]' '[:lower:]')
if [[ "$os" == "darwin" ]]; then
    DATA_DIR="$HOME/Library/Application Support/com.divlens.divlens"
    SERVICE="$HOME/Library/LaunchAgents/io.divlens.mcp.plist"
else
    DATA_DIR="$HOME/.local/share/divlens"
    SERVICE="$HOME/.config/systemd/user/divlens-mcp.service"
fi

# ─── Show what will be removed ────────────────────────────────────────────────
printf "  ${B}This will remove:${RST}\n"
[[ -n "$BINARY" ]]                && info "Binary:    $BINARY"
[[ -d "$DATA_DIR" ]]              && info "Database:  $DATA_DIR"
[[ -f "$SERVICE" ]]               && info "Service:   $SERVICE"
info "Configs:   divlens entries from Claude, Cursor, Windsurf, Antigravity"

# ─── Confirm ──────────────────────────────────────────────────────────────────
if [[ "$FORCE" != "true" ]]; then
    echo ""
    printf "  Proceed? [y/N] "
    read -r answer
    [[ "$answer" != "y" && "$answer" != "Y" ]] && { info "Cancelled."; exit 0; }
fi
echo ""

ERRORS=0

# ─── Remove binary ───────────────────────────────────────────────────────────
if [[ -n "$BINARY" ]]; then
    if rm "$BINARY" 2>/dev/null || sudo rm "$BINARY" 2>/dev/null; then
        ok "Removed binary"
    else
        fail "Could not remove $BINARY"; ((ERRORS++))
    fi
fi

# ─── Remove data ─────────────────────────────────────────────────────────────
if [[ -d "$DATA_DIR" ]]; then
    rm -rf "$DATA_DIR" && ok "Removed database" || { fail "Could not remove data"; ((ERRORS++)); }
fi

# ─── Remove service ──────────────────────────────────────────────────────────
if [[ -f "$SERVICE" ]]; then
    if [[ "$os" == "darwin" ]]; then
        launchctl unload "$SERVICE" 2>/dev/null || true
    else
        systemctl --user stop divlens-mcp 2>/dev/null || true
        systemctl --user disable divlens-mcp 2>/dev/null || true
    fi
    rm -f "$SERVICE" && ok "Removed service" || { fail "Could not remove service"; ((ERRORS++)); }
fi

# ─── Clean AI client configs ─────────────────────────────────────────────────
if command -v python3 &>/dev/null; then
    _clean_config() {
        local name="$1" path="$2"
        [[ ! -f "$path" ]] && return
        python3 -c "
import json, sys
path = sys.argv[1]
with open(path, 'r') as f:
    raw = f.read()
if raw.startswith('\ufeff'):
    raw = raw[1:]
try:
    c = json.loads(raw)
except:
    sys.exit(0)
if 'mcpServers' in c and 'divlens' in c['mcpServers']:
    del c['mcpServers']['divlens']
    with open(path, 'w') as f:
        json.dump(c, f, indent=2)
        f.write('\n')
    print('OK')
" "$path" 2>/dev/null && ok "Cleaned $name config"
    }

    if [[ "$os" == "darwin" ]]; then
        _clean_config "Claude"       "$HOME/Library/Application Support/Claude/claude_desktop_config.json"
        _clean_config "Cursor"       "$HOME/.cursor/mcp.json"
        _clean_config "Windsurf"     "$HOME/.codeium/windsurf/mcp_config.json"
        _clean_config "Antigravity"  "$HOME/.gemini/mcp_config.json"
    else
        _clean_config "Claude"       "$HOME/.config/Claude/claude_desktop_config.json"
        _clean_config "Cursor"       "$HOME/.cursor/mcp.json"
        _clean_config "Windsurf"     "$HOME/.codeium/windsurf/mcp_config.json"
        _clean_config "Antigravity"  "$HOME/.gemini/mcp_config.json"
    fi
fi

# ─── Summary ──────────────────────────────────────────────────────────────────
echo ""
if [[ $ERRORS -eq 0 ]]; then
    printf "  ${G}${B}DivLens has been completely removed. 👋${RST}\n"
else
    printf "  ${Y}Uninstall completed with $ERRORS error(s).${RST}\n"
fi
echo ""
