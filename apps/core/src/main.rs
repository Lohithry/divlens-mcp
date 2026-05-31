/// DivLens Core — System Intelligence for AI
///
/// # Run Modes
///
/// ```bash
/// divlens-core --mcp         # Start MCP server (backward compat)
/// divlens-core mcp           # Start MCP server (new style)
/// divlens-core status        # Show installation status
/// divlens-core doctor        # Run health checks
/// divlens-core config --show # Show AI client configurations
/// divlens-core uninstall     # Completely remove DivLens
/// divlens-core --help        # Show help
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
mod cli;

use clap::{Parser, Subcommand};
use dotenvy::dotenv;

/// DivLens Core — System Intelligence for AI Agents
///
/// Exposes 17 real-time diagnostic tools (CPU, RAM, disk, network, processes,
/// hardware health, developer stack, system logs) to any MCP-compatible
/// AI client via the stdio JSON-RPC 2.0 transport.
///
/// Lifecycle commands (status, doctor, config, uninstall) provide terminal-based
/// management of the DivLens installation.
#[derive(Parser, Debug)]
#[command(
    name        = "divlens-core",
    version,
    author,
    about       = "DivLens — Real-time system intelligence for AI agents",
    long_about  = None,
)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// [DEPRECATED] Start as MCP server. Use `divlens-core mcp` instead.
    /// Kept for backward compatibility with existing AI client configurations.
    #[arg(long, hide = true)]
    mcp: bool,

    /// Enable verbose debug logging (logs go to stderr — stdout stays clean).
    #[arg(short, long, global = true)]
    debug: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start MCP server (stdio JSON-RPC transport).
    /// This is the primary run mode — AI clients spawn this automatically.
    Mcp,

    /// Show installation status and connected AI clients.
    Status,

    /// Run diagnostic health checks on the installation.
    /// Validates binary, PATH, database, MCP handshake, and AI client configs.
    Doctor,

    /// Show or manage AI client configurations.
    Config {
        /// Display current configuration for all AI clients.
        #[arg(long)]
        show: bool,
    },

    /// Completely remove DivLens from this machine.
    /// Removes binary, database, config entries, and service registration.
    Uninstall {
        /// Skip confirmation prompt.
        #[arg(long)]
        force: bool,
    },
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

    // ─── Route: --mcp flag (backward compat) or `mcp` subcommand ─────────────
    if args.mcp {
        init_tracing(args.debug);
        return mcp_server::start_mcp_server().await;
    }

    match args.command {
        Some(Commands::Mcp) => {
            init_tracing(args.debug);
            mcp_server::start_mcp_server().await
        }
        Some(Commands::Status) => {
            cli::status::run();
            Ok(())
        }
        Some(Commands::Doctor) => {
            cli::doctor::run();
            Ok(())
        }
        Some(Commands::Config { show }) => {
            if show {
                cli::config::run_show();
            } else {
                // Default behavior: show config
                cli::config::run_show();
            }
            Ok(())
        }
        Some(Commands::Uninstall { force }) => {
            cli::uninstall::run(force);
            Ok(())
        }
        None => {
            // No subcommand — print beautiful usage
            print_usage();
            Ok(())
        }
    }
}

/// Print a helpful usage message when no subcommand is given.
fn print_usage() {
    let version = env!("CARGO_PKG_VERSION");
    eprintln!();
    eprintln!("  \x1b[1m\x1b[38;5;208m🔍 DivLens v{}\x1b[0m — System Intelligence for AI", version);
    eprintln!();
    eprintln!("  \x1b[1mUsage:\x1b[0m");
    eprintln!("    divlens-core \x1b[96mmcp\x1b[0m              Start MCP server (stdio JSON-RPC)");
    eprintln!("    divlens-core \x1b[96mstatus\x1b[0m           Show installation status");
    eprintln!("    divlens-core \x1b[96mdoctor\x1b[0m           Run health checks");
    eprintln!("    divlens-core \x1b[96mconfig --show\x1b[0m    Show AI client configs");
    eprintln!("    divlens-core \x1b[96muninstall\x1b[0m        Remove DivLens completely");
    eprintln!();
    eprintln!("  \x1b[1mOptions:\x1b[0m");
    eprintln!("    \x1b[96m-d, --debug\x1b[0m              Enable verbose logging");
    eprintln!("    \x1b[96m-h, --help\x1b[0m               Show help");
    eprintln!("    \x1b[96m-V, --version\x1b[0m            Show version");
    eprintln!();
    eprintln!("  \x1b[2mDocs:   https://github.com/Lohithry/divlens-mcp\x1b[0m");
    eprintln!("  \x1b[2mIssues: https://github.com/Lohithry/divlens-mcp/issues\x1b[0m");
    eprintln!();
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
