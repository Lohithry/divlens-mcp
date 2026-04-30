/// DivLens Core — MCP Server Library
///
/// This library exposes the MCP server modules and all diagnostic tools
/// for use in integration tests and external consumers.
///
/// # Architecture
///
/// DivLens MCP is a **Zero-AI tool server**. It exposes 17 real-time
/// diagnostic tools over the Model Context Protocol (MCP) stdio transport.
/// The AI reasoning happens entirely in the MCP client (Claude, Cursor, etc.).
///
/// ```text
/// ┌──────────────────────────────────┐
/// │     AI Client (Claude / Cursor)  │  ← LLM reasoning lives here
/// └─────────────────┬────────────────┘
///                   │  JSON-RPC over stdio (MCP)
///                   ▼
/// ┌──────────────────────────────────┐
/// │     divlens-core  (this binary)  │
/// │  ┌────────┐  ┌────────────────┐  │
/// │  │  MCP   │  │  17 Tools      │  │  ← Only real-time OS data here
/// │  │  Layer │  │  (Rust + OS)   │  │
/// │  └────────┘  └────────────────┘  │
/// └──────────────────────────────────┘
/// ```
///
/// # Modules
///
/// - `mcp`: JSON-RPC 2.0 protocol handler and McpServer
/// - `mcp_server`: stdio transport loop and tool registration
/// - `tools`: All 17 diagnostic tool implementations
/// - `modules`: Core business logic (metrics, datahub, memory)
/// - `collectors`: Native OS data collection (volatile + persistent)
/// - `db`: SQLite caching layer
/// - `models`: Shared data types
/// - `utils`: Utilities (env_fixer for shell environment rehydration)

// ─── MCP Protocol & Transport ────────────────────────────────────────────────
pub mod mcp;
pub mod mcp_server;

// ─── Diagnostic Tools (the 17 tools) ────────────────────────────────────────
pub mod tools;

// ─── Data & Storage ──────────────────────────────────────────────────────────
pub mod modules;
pub mod models;
pub mod db;

// ─── OS Data Collectors ──────────────────────────────────────────────────────
pub mod collectors;

// ─── Utilities ───────────────────────────────────────────────────────────────
pub mod utils;
