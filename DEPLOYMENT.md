# DivLens MCP — Deployment Guide

> **DivLens System Intelligence** gives Claude Desktop, Cursor, Windsurf, and
> any other MCP-compatible AI client real-time access to your machine's
> hardware, network, storage, process, and development-stack data.
>
> **Zero cloud dependencies. Zero API keys. 100% local.**

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Build from Source](#2-build-from-source)
3. [Install the Binary](#3-install-the-binary)
4. [Configure Your MCP Client](#4-configure-your-mcp-client)
   - [Claude Desktop (macOS / Windows)](#claude-desktop-macos--windows)
   - [Cursor](#cursor)
   - [Windsurf](#windsurf)
5. [Environment Variables](#5-environment-variables)
6. [Available Tools](#6-available-tools)
7. [Verify the Server Works](#7-verify-the-server-works)
8. [Run Modes](#8-run-modes)
9. [Troubleshooting](#9-troubleshooting)

---

## 1. Prerequisites

| Requirement | Minimum version | Install |
|---|---|---|
| Rust toolchain | 1.82 (edition 2024) | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Git | any | `brew install git` / your system package manager |

> **macOS only** — Xcode Command Line Tools are required for the `io-kit-sys`
> and `mach2` native bindings:
> ```bash
> xcode-select --install
> ```

---

## 2. Build from Source

```bash
# Clone the repository
git clone https://github.com/your-org/divlens-mcp.git
cd divlens-mcp/apps/core

# Build the release binary (~2 min on first run)
cargo build --release
```

Binaries are output to:
- `./target/release/divlens-core` — macOS / Linux
- `.\target\release\divlens-core.exe` — Windows

> **Optional — vector memory** (semantic `recall_memory` feature):
> ```bash
> cargo build --release --features vector-memory
> ```
> This downloads ONNX embedding models (~100 MB) on first run.

---

## 3. Install the Binary

### macOS / Linux

```bash
# Copy to a directory on your PATH
sudo cp target/release/divlens-core /usr/local/bin/divlens-core

# Verify
divlens-core --version
```

### Windows (PowerShell)

```powershell
# Copy to a directory on your PATH, e.g. C:\bin
Copy-Item target\release\divlens-core.exe C:\bin\divlens-core.exe

# Verify
divlens-core --version
```

---

## 4. Configure Your MCP Client

The MCP server communicates over **stdio** (stdin/stdout). Add it to your
client's config file; the client spawns the process automatically.

### Claude Desktop (macOS / Windows)

Config file location:
- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

```jsonc
{
  "mcpServers": {
    "divlens": {
      "command": "/usr/local/bin/divlens-core",
      "args": ["--mcp"],
      "env": {}
    }
  }
}
```

> **Windows path example**:
> ```jsonc
> "command": "C:\\bin\\divlens-core.exe"
> ```

After editing, **quit and relaunch** Claude Desktop. A 🔌 icon in the
input bar confirms DivLens is connected.

---

### Cursor

Config file: `~/.cursor/mcp.json`

```jsonc
{
  "mcpServers": {
    "divlens": {
      "command": "/usr/local/bin/divlens-core",
      "args": ["--mcp"]
    }
  }
}
```

Reload Cursor (`Cmd+Shift+P` → *Reload Window*).

---

### Windsurf

Config file: `~/.codeium/windsurf/mcp_config.json`

```jsonc
{
  "mcpServers": {
    "divlens": {
      "command": "/usr/local/bin/divlens-core",
      "args": ["--mcp"]
    }
  }
}
```

---

## 5. Environment Variables

**MCP mode (`--mcp`) requires no environment variables** — all 17 tools work
fully offline using local OS APIs and SQLite.

The following are optional overrides, primarily useful for development:

```bash
# Enable verbose debug logging (output goes to stderr, never stdout)
# RUST_LOG=debug

# Override the data/database directory (default: OS-standard app data dir)
# DIVLENS_DATA_DIR=/path/to/custom/dir
```

Copy the example file:
```bash
cp apps/core/env.example apps/core/.env
```

---

## 6. Available Tools

DivLens registers **17 tools** with every MCP client:

| Category | Tool name | What it returns |
|---|---|---|
| **System** | `get_live_metrics` | CPU %, RAM, swap, uptime |
| **System** | `get_process_list` | Top processes by CPU/RAM |
| **Storage** | `get_storage_health` | Disk space per mount point |
| **Storage** | `scan_storage_inventory` | Full file-type inventory |
| **Storage** | `get_file_type_summary` | File counts/sizes by extension |
| **Storage** | `get_specific_file_type` | Files of a specific extension |
| **Storage** | `get_advanced_storage_stats` | Largest files, stale data |
| **Storage** | `get_storage_diagnostics` | IOPS, latency, health status |
| **Hardware** | `get_hardware_diagnostics` | CPU, GPU, battery, temperatures |
| **Network** | `get_network_diagnostics` | Latency, throughput, connections |
| **Network** | `get_network_config` | IP, DNS, interface config |
| **Identity** | `get_system_dna` | OS, hostname, machine fingerprint |
| **Dev** | `get_dev_stack` | Installed languages, runtimes, tools |
| **Dev** | `get_drivers` | Kernel modules / device drivers |
| **Utility** | `scan_directory` | Recursive directory listing |
| **Utility** | `recall_memory` | Semantic memory recall |
| **Logs** | `get_system_logs` | Recent OS/kernel log entries |

---

## 7. Verify the Server Works

Test the MCP wire protocol manually without any client:

```bash
# List all registered tools
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"test","version":"0.1"}}}' \
  | divlens-core --mcp

# Call a tool
echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_live_metrics","arguments":{}}}' \
  | divlens-core --mcp
```

A valid `tools/list` response looks like:
```jsonc
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "tools": [
      { "name": "get_live_metrics", "description": "…", "inputSchema": { … } },
      …  // 17 tools total
    ]
  }
}
```

---

## 8. Run Modes

```
divlens-core [OPTIONS]

Options:
  --mcp       Start as MCP server (stdio JSON-RPC transport)
  --debug     Enable verbose debug logging (output to stderr)
  --help      Print help
  --version   Print version
```

| Flag | Transport | Use case |
|---|---|---|
| `--mcp` | stdin / stdout (JSON-RPC 2.0) | Claude Desktop, Cursor, Windsurf |
| `--mcp --debug` | stdin / stdout + stderr logs | Development and troubleshooting |

---

## 9. Troubleshooting

### "Tool not found" or empty tool list

The MCP handshake requires an `initialize` message before `tools/list`. Most
clients handle this automatically. If testing manually, always send
`initialize` first.

### Claude Desktop shows no 🔌 icon

1. Confirm the path in `claude_desktop_config.json` is **absolute** and the
   binary is executable (`chmod +x /usr/local/bin/divlens-core`).
2. Check Claude's MCP logs:
   - macOS: `~/Library/Logs/Claude/mcp*.log`
   - Windows: `%APPDATA%\Claude\logs\mcp*.log`
3. Run the binary manually to confirm it starts without errors:
   ```bash
   divlens-core --mcp
   # Should print nothing to stdout; logs go to stderr
   ```

### "Database init failed"

DivLens stores its SQLite database in the OS data directory:
- **macOS**: `~/Library/Application Support/divlens/divlens.db`
- **Linux**: `~/.local/share/divlens/divlens.db`
- **Windows**: `%APPDATA%\divlens\divlens.db`

Ensure the directory is writable. The database is created automatically on
first run.

### Slow first tool call (storage / driver tools)

The driver-cache and storage-cache warm up in background tasks at startup.
The first call to `get_drivers` or storage tools may take a few seconds while
the cache populates; subsequent calls are instant.

### Compilation errors on macOS

Ensure Xcode Command Line Tools are installed:
```bash
xcode-select --install
```

---

*DivLens System Intelligence — real-time diagnostics for AI-native workflows.*
