/// DivLens Core — MCP Server Entry Point
///
/// # Run Modes
///
/// ```bash
/// divlens-core --mcp     # Start MCP server (stdio JSON-RPC transport)
/// divlens-core --help    # Show help
/// ```
///
/// # MCP Transport
///
/// ```text
/// MCP Client ──► stdin  (JSON-RPC 2.0 newline-delimited)  ──► divlens-core
/// MCP Client ◄── stdout (JSON-RPC 2.0 newline-delimited)  ◄── divlens-core
///                stderr (tracing logs only — never pollutes the wire)
/// ```

mod modules;
mod models;
mod db;
mod collectors;
mod mcp;
mod tools;
mod utils;
mod mcp_server;

use clap::Parser;
use dotenvy::dotenv;
use tracing::info;

/// DivLens Core — System Intelligence MCP Server
///
/// Exposes 17 real-time diagnostic tools (CPU, RAM, disk, network, processes,
/// hardware health, developer stack, system logs) to any MCP-compatible
/// AI client via the stdio JSON-RPC 2.0 transport.
#[derive(Parser, Debug)]
#[command(
    name        = "divlens-core",
    version,
    author,
    about       = "DivLens MCP — Real-time system intelligence for AI agents",
    long_about  = None,
)]
struct Args {
    /// Start as MCP server (stdio JSON-RPC transport).
    /// This is the only supported run mode for the MCP product.
    #[arg(long)]
    mcp: bool,

    /// Enable verbose debug logging (logs go to stderr — stdout stays clean).
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // =========================================================================
    // CRITICAL: Rehydrate shell environment BEFORE anything else.
    //
    // When launched by an MCP host (Claude Desktop, Cursor, Windsurf), the
    // process inherits a minimal PATH (/usr/bin:/bin). env_fixer spawns the
    // user's login shell, captures the full environment (PATH, CONDA_PREFIX,
    // NVM_DIR, CARGO_HOME, etc.), and injects it into the current process so
    // that all downstream tool invocations can find pip, cargo, npm, etc.
    // =========================================================================
    utils::env_fixer::rehydrate();

    // Load .env (optional — harmless if not present)
    dotenv().ok();

    let args = Args::parse();

    // Route to the appropriate run mode.
    // All tracing goes to stderr so stdout remains clean for MCP JSON-RPC.
    init_tracing(args.debug);

    if args.mcp {
        return mcp_server::start_mcp_server().await;
    }

    // No flag supplied — print usage and exit cleanly.
    info!("DivLens MCP — no run mode specified.");
    eprintln!(
        "\nUsage:\n  \
         divlens-core --mcp      Start MCP server (stdio JSON-RPC transport)\n  \
         divlens-core --help     Show full help\n"
    );
    Ok(())
}

/// Configure `tracing` to write all output to stderr.
///
/// stdout must remain exclusively reserved for the MCP JSON-RPC protocol.
/// Any tracing output written to stdout would corrupt the message stream.
fn init_tracing(debug: bool) {
    use tracing::Level;
    use tracing_subscriber::fmt;

    let level = if debug { Level::DEBUG } else { Level::INFO };

    fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .init();
}
