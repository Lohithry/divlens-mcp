/// DivLens MCP Server — stdio transport
///
/// Exposes all DivLens diagnostic tools to any MCP-compatible AI client
/// (Claude Desktop, Cursor, Windsurf, etc.) over the stdio JSON-RPC channel.
///
/// # Invocation
///
/// ```bash
/// divlens-core --mcp
/// ```
///
/// # Transport
///
/// ```text
/// MCP Client ──► stdin  (JSON-RPC 2.0 lines) ──► divlens-core --mcp
/// MCP Client ◄── stdout (JSON-RPC 2.0 lines) ◄── divlens-core --mcp
///                stderr (tracing logs only)
/// ```
///
/// stdout is exclusively reserved for the MCP wire protocol.
/// All tracing output is directed to stderr.

use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;
use tracing::{info, error};

use crate::mcp::McpServer;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;
use crate::collectors::volatile::VolatileCollector;
use crate::collectors::persistent::storage::StorageCollector;
use crate::db::DatabaseManager;
use crate::utils::data_dir;

use crate::tools::{
    live_metrics::GetLiveMetricsTool,
    processes::GetProcessListTool,
    storage::{
        GetStorageHealthTool,
        ScanStorageInventoryTool,
        GetFileTypeSummaryTool,
        GetSpecificFileTypeTool,
        GetAdvancedStorageStatsTool,
    },
    storage_diagnostics::GetStorageDiagnosticsTool,
    hardware_diagnostics::GetHardwareDiagnosticsTool,
    filesystem::ScanDirectoryTool,
    memory::RecallMemoryTool,
    system_dna::GetSystemDnaTool,
    network_config::GetNetworkConfigTool,
    network_diagnostics::GetNetworkDiagnosticsTool,
    dev_stack::GetDevStackTool,
    drivers::GetDriversTool,
    system_logs::GetSystemLogsTool,
};

/// Start the DivLens MCP server.
///
/// Initialises all tool dependencies, registers every tool, then enters the
/// stdio read-loop. Each newline-delimited JSON-RPC message is dispatched and
/// the response is written back to stdout, flushed immediately.
pub async fn start_mcp_server() -> anyhow::Result<()> {
    info!("DivLens MCP Server starting…");

    // ─── Background cache warm-up ─────────────────────────────────────────────
    // Pre-fetch slow platform data so the first tool call is instant.
    crate::collectors::ondemand::drivers::init_driver_cache_background();
    crate::collectors::persistent::storage::init_app_cache_background();

    // ─── Data paths (cross-platform) ──────────────────────────────────────────
    let data_dir  = data_dir::get_data_dir();
    let db_path   = data_dir::get_db_path();
    let lance_path = data_dir::get_lancedb_path();

    info!("Data directory : {:?}", data_dir);
    info!("Database       : {:?}", db_path);

    // ─── Database ─────────────────────────────────────────────────────────────
    let db_path_str = db_path.to_string_lossy().to_string();
    let db = match DatabaseManager::new(&db_path_str) {
        Ok(mgr) => {
            mgr.init_schema()
                .map_err(|e| anyhow::anyhow!("Database schema init failed: {}", e))?;
            info!("Database initialised");
            Arc::new(mgr)
        }
        Err(e) => return Err(anyhow::anyhow!("Database init failed: {}", e)),
    };

    // Populate system data (non-fatal — first run may still be empty)
    if let Err(e) = crate::collectors::SystemScanner::run_all(&db).await {
        info!("Scanner warning (non-fatal): {}", e);
    }

    // ─── Collectors ───────────────────────────────────────────────────────────
    let volatile  = Arc::new(Mutex::new(VolatileCollector::new()));
    let storage   = Arc::new(TokioMutex::new(StorageCollector::new()));

    // ─── DataHub ──────────────────────────────────────────────────────────────
    let datahub = match DataHub::new(&db_path_str) {
        Ok(dh) => { info!("DataHub initialised"); Arc::new(dh) }
        Err(e) => return Err(anyhow::anyhow!("DataHub init failed: {}", e)),
    };

    // ─── Memory Manager ───────────────────────────────────────────────────────
    let lance_path_str = lance_path.to_string_lossy().to_string();
    let memory = match MemoryManager::new(&lance_path_str).await {
        Ok(mem) => {
            if let Err(e) = mem.init_tables().await {
                info!("Memory tables warning (non-fatal): {}", e);
            }
            info!("Memory manager initialised");
            Arc::new(mem)
        }
        Err(e) => {
            // vector-memory feature may be disabled — use a no-op fallback
            info!("Memory manager unavailable (non-fatal): {}", e);
            Arc::new(
                MemoryManager::new(".").await
                    .expect("MemoryManager fallback must not fail"),
            )
        }
    };

    // ─── Metrics collector (used internally by handle_message) ────────────────
    let collector = Arc::new(TokioMutex::new(
        crate::modules::metrics::SystemCollector::new(),
    ));

    // ─── Tool registration ────────────────────────────────────────────────────
    let mut mcp = McpServer::new();

    // System & Performance
    mcp.register_tool(Box::new(GetLiveMetricsTool::new(volatile.clone())));
    mcp.register_tool(Box::new(GetProcessListTool::new(volatile.clone())));

    // Storage
    mcp.register_tool(Box::new(GetStorageHealthTool { collector: storage.clone() }));
    mcp.register_tool(Box::new(ScanStorageInventoryTool {
        collector: storage.clone(),
        db: db.clone(),
    }));
    mcp.register_tool(Box::new(GetFileTypeSummaryTool { db: db.clone() }));
    mcp.register_tool(Box::new(GetSpecificFileTypeTool { db: db.clone() }));
    mcp.register_tool(Box::new(GetAdvancedStorageStatsTool));
    mcp.register_tool(Box::new(GetStorageDiagnosticsTool));

    // Hardware
    mcp.register_tool(Box::new(GetHardwareDiagnosticsTool::new()));

    // Network
    mcp.register_tool(Box::new(GetNetworkDiagnosticsTool::new(volatile.clone())));
    mcp.register_tool(Box::new(GetNetworkConfigTool::new(db.clone())));

    // Development & Identity
    mcp.register_tool(Box::new(GetSystemDnaTool::new(db.clone(), volatile.clone())));
    mcp.register_tool(Box::new(GetDevStackTool::new(db.clone())));
    mcp.register_tool(Box::new(GetDriversTool));

    // Utility
    mcp.register_tool(Box::new(ScanDirectoryTool));
    mcp.register_tool(Box::new(RecallMemoryTool));
    mcp.register_tool(Box::new(GetSystemLogsTool::new()));

    info!(
        "MCP Server ready — {} tools registered: {:?}",
        mcp.tool_count(),
        mcp.get_tool_names()
    );
    info!("Listening on stdio (JSON-RPC 2.0)…");

    // ─── Stdio transport loop ─────────────────────────────────────────────────
    // Read newline-delimited JSON from stdin, write responses to stdout.
    // stdout must never contain anything other than JSON-RPC messages.
    let stdin  = io::stdin();
    let reader = io::BufReader::new(stdin.lock());

    for line in reader.lines() {
        let line = match line {
            Ok(l)  => l,
            Err(e) => { error!("stdin read error: {}", e); break; }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut col = collector.lock().await;
        let response = mcp
            .handle_message(trimmed, &mut *col, &datahub, &memory)
            .await;

        // Notifications produce no response — don't write an empty line.
        if response.is_empty() {
            continue;
        }

        println!("{}", response);
        if let Err(e) = io::stdout().flush() {
            error!("stdout flush error: {}", e);
            break;
        }
    }

    info!("DivLens MCP Server stopped.");
    Ok(())
}
