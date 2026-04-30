<p align="center">
  <img src="divlensmainlogo.png" width="120" alt="DivLens Logo" />
</p>

<h1 align="center">DivLens MCP</h1>

<p align="center">
  <strong>Real-time system intelligence for AI agents.</strong><br/>
  Give Claude, Cursor, and Windsurf eyes into your machine — CPU, RAM, disk, network, processes, hardware health, and more.
</p>

<p align="center">
  <a href="https://opensource.org/licenses/Apache-2.0">
    <img src="https://img.shields.io/badge/License-Apache%202.0-orange.svg" alt="License: Apache 2.0" />
  </a>
  <a href="https://www.rust-lang.org">
    <img src="https://img.shields.io/badge/Built%20with-Rust-orange.svg?logo=rust" alt="Built with Rust" />
  </a>
  <img src="https://img.shields.io/badge/MCP-Compatible-orange.svg" alt="MCP Compatible" />
  <img src="https://img.shields.io/badge/Platform-macOS%20%7C%20Windows%20%7C%20Linux-orange.svg" alt="Platform" />
  <img src="https://img.shields.io/badge/Version-0.1.0-orange.svg" alt="Version" />
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Claude-Compatible-blueviolet?logo=anthropic" alt="Claude" />
  <img src="https://img.shields.io/badge/Cursor-Compatible-blue?logo=cursor" alt="Cursor" />
  <img src="https://img.shields.io/badge/Windsurf-Compatible-teal" alt="Windsurf" />
  <img src="https://img.shields.io/badge/Zero%20Cloud-100%25%20Local-brightgreen" alt="Zero Cloud" />
</p>

---

## What is DivLens MCP?

**DivLens MCP** is a high-performance [Model Context Protocol (MCP)](https://modelcontextprotocol.io) server written in Rust.

It bridges the gap between AI assistants and your machine — giving Claude, Cursor, Windsurf, and any other MCP-compatible agent **live, structured access** to hardware sensors, storage metrics, network diagnostics, process trees, developer runtimes, system logs, and more.

No cloud. No API keys. No configuration required. Just build and run.

```
"Why is my Mac slow?" → Claude calls get_live_metrics() → Instant answer.
"Is my SSD healthy?"  → Claude calls get_hardware_diagnostics() → SMART data returned.
"What's eating disk?"  → Claude calls get_advanced_storage_stats() → Largest files listed.
```

---

## ✦ 17 Diagnostic Tools

| Category | Tool | What it returns |
| :--- | :--- | :--- |
| ⚡ **Performance** | `get_live_metrics` | CPU %, RAM, swap, blocked processes, uptime |
| ⚡ **Performance** | `get_process_list` | Top processes by CPU / RAM with PID |
| 💾 **Storage** | `get_storage_health` | Free/used/total per mount point |
| 💾 **Storage** | `scan_storage_inventory` | Full file-type inventory with sizes |
| 💾 **Storage** | `get_file_type_summary` | File counts and sizes by extension |
| 💾 **Storage** | `get_specific_file_type` | All files matching a specific extension |
| 💾 **Storage** | `get_advanced_storage_stats` | Top 50 largest files + stale data analysis |
| 💾 **Storage** | `get_storage_diagnostics` | IOPS, read/write latency, SMART status |
| 🖥️ **Hardware** | `get_hardware_diagnostics` | CPU/GPU specs, battery %, temps, SMART |
| 🌐 **Network** | `get_network_diagnostics` | Throughput, active connections, signal |
| 🌐 **Network** | `get_network_config` | IP, DNS, interface config per adapter |
| 🔬 **Identity** | `get_system_dna` | OS, hostname, uptime, machine fingerprint |
| 🛠️ **Dev Stack** | `get_dev_stack` | Node, Python, Rust, Go, Java runtimes + packages |
| 🛠️ **Dev Stack** | `get_drivers` | Kernel modules and device drivers |
| 📂 **Utility** | `scan_directory` | Recursive directory listing with sizes |
| 🧠 **Memory** | `recall_memory` | Semantic search over past AI diagnoses |
| 📋 **Logs** | `get_system_logs` | Recent OS/kernel errors clustered by pattern |

---

## Quick Start

### 1 — Prerequisites

| Requirement | Minimum | Install |
| :--- | :--- | :--- |
| **Rust** | 1.82 (edition 2024) | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| **Git** | any | `brew install git` or your package manager |

> **macOS** requires Xcode Command Line Tools for native hardware bindings:
> ```bash
> xcode-select --install
> ```

### 2 — Build

```bash
git clone https://github.com/Lohithry/divlens-mcp.git
cd divlens-mcp/apps/core

cargo build --release
```

Binary output:
- `target/release/divlens-core` — macOS / Linux
- `target\release\divlens-core.exe` — Windows

> **Optional — semantic memory** (downloads ~100 MB ONNX model on first run):
> ```bash
> cargo build --release --features vector-memory
> ```

### 3 — Install

```bash
# macOS / Linux — copy to PATH
sudo cp target/release/divlens-core /usr/local/bin/divlens-core

# Windows (PowerShell)
Copy-Item target\release\divlens-core.exe C:\bin\divlens-core.exe
```

---

## Connect to Your AI

### Claude Desktop

> Config file: `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS)  
> or `%APPDATA%\Claude\claude_desktop_config.json` (Windows)

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

Quit and relaunch Claude Desktop. A 🔌 plug icon confirms the connection.

### Cursor

> Config file: `~/.cursor/mcp.json`

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

`Cmd+Shift+P` → *Reload Window*

### Windsurf

> Config file: `~/.codeium/windsurf/mcp_config.json`

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

For complete setup details, see **[DEPLOYMENT.md](DEPLOYMENT.md)**.

---

## How It Works

```
  ┌─────────────────────────────────────────┐
  │   AI Client  (Claude / Cursor / etc.)   │
  │         LLM reasoning lives here        │
  └──────────────────┬──────────────────────┘
                     │  JSON-RPC 2.0  (stdio)
                     ▼
  ┌─────────────────────────────────────────┐
  │          divlens-core  (Rust)           │
  │                                         │
  │  ┌───────────────┐  ┌───────────────┐   │
  │  │  MCP Layer    │  │  17 Tools     │   │
  │  │  (JSON-RPC)   │  │  (Rust + OS)  │   │
  │  └───────────────┘  └───────────────┘   │
  │  ┌───────────────┐  ┌───────────────┐   │
  │  │  SQLite Cache │  │  Native APIs  │   │
  │  │  (sysinfo/OS) │  │  (IOKit/WMI)  │   │
  │  └───────────────┘  └───────────────┘   │
  └─────────────────────────────────────────┘

      Zero cloud.  Zero API keys.  100% local.
```

**Transport:** Every MCP message is a newline-delimited JSON-RPC 2.0 object over stdio.  
**AI logic:** DivLens never runs LLM inference — it only collects and returns raw system data.  
**Privacy:** All data stays on your machine. Nothing is sent anywhere.

---

## Project Structure

```
divlens-mcp/
└── apps/
    └── core/                      # Rust MCP engine
        ├── src/
        │   ├── tools/             # 17 tool implementations
        │   ├── mcp/               # JSON-RPC 2.0 protocol handler
        │   ├── mcp_server.rs      # stdio transport loop
        │   ├── collectors/        # Native OS data collectors
        │   │   ├── volatile/      # CPU, RAM, network (live)
        │   │   ├── persistent/    # Storage, hardware (cached)
        │   │   └── ondemand/      # Drivers, logs, packages
        │   ├── modules/           # Core business logic
        │   ├── db/                # SQLite caching layer
        │   ├── models/            # Shared data types
        │   └── utils/             # Shell env rehydration
        ├── Cargo.toml
        └── env.example
```

---

## Optional: Semantic Memory

Enable the `vector-memory` feature to give `recall_memory` true semantic search using a local ONNX embedding model (no cloud, no API key):

```bash
cargo build --release --features vector-memory
```

When enabled, DivLens creates a local [LanceDB](https://lancedb.github.io/lancedb/) vector store and uses [fastembed](https://github.com/Anush008/fastembed-rs) to embed and recall past diagnoses semantically.

When disabled (default), `recall_memory` returns an empty list — no functionality is broken.

---

## Verify the Server

Test the MCP wire protocol without a client:

```bash
# Initialize handshake
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"test","version":"0.1"}}}' \
  | divlens-core --mcp

# Call a tool directly
echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_live_metrics","arguments":{}}}' \
  | divlens-core --mcp
```

---

## License

Licensed under the **Apache License, Version 2.0**.  
See [LICENSE](LICENSE) for the full text.

Copyright © 2024 DivLens Contributors.

---

<p align="center">
  <img src="divlensmainlogo.png" width="48" alt="DivLens" /><br/>
  <sub>Built with ❤️ in Rust · Zero cloud · AI-native diagnostics</sub>
</p>
