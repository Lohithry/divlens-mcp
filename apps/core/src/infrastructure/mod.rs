/// Infrastructure Layer
///
/// This module is intentionally minimal for the MCP server binary.
/// The MCP server reads data directly from the OS via native Rust APIs —
/// no cloud providers, no HTTP clients, no LLM adapters are needed here.

