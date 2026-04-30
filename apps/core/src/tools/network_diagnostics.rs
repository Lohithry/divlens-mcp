// ==================================================================================
// 🧠 DIVLENS NETWORK DIAGNOSTICS TOOL (MCP Integration)
// ==================================================================================
// This tool provides AI-friendly network diagnostics that answer:
// "Why is my network slow?" or "What's using my bandwidth?"
//
// DESIGN PRINCIPLES:
// 1. Pre-Analysis: Does the heavy lifting so AI gets clean, actionable data
// 2. Human-Friendly: Returns summaries like "Signal is weak" not raw dBm values
// 3. Performance: Combines volatile, lazy, and static collectors efficiently
// ==================================================================================

use async_trait::async_trait;
use serde_json::Value;
use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;
use super::Tool;
use std::sync::{Arc, Mutex};
use crate::collectors::volatile::{VolatileCollector, network_metrics};
use crate::collectors::ondemand::{net_connections, net_config};

pub struct GetNetworkDiagnosticsTool {
    collector: Arc<Mutex<VolatileCollector>>,
}

impl GetNetworkDiagnosticsTool {
    pub fn new(collector: Arc<Mutex<VolatileCollector>>) -> Self {
        Self { collector }
    }
}

#[async_trait]
impl Tool for GetNetworkDiagnosticsTool {
    fn name(&self) -> &str {
        "get_network_diagnostics"
    }

    fn description(&self) -> &str {
        "Retrieves comprehensive network diagnostics including interface status, signal strength, \
        active connections with process attribution, DNS configuration, and routing table. \
        Provides pre-analyzed summaries to help diagnose network performance issues. \
        Returns:\n\
        - Primary connection: Interface name, type, signal strength (dBm), and speed (Mbps)\n\
        - Active connections: Count and top applications using network\n\
        - DNS servers: Configured DNS resolver addresses\n\
        - Signal warnings: Alerts if Wi-Fi signal is weak\n\
        Use this to answer questions like 'Why is my network slow?' or 'What's using my bandwidth?'"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "include_connections": {
                    "type": "boolean",
                    "description": "Whether to include detailed connection list"
                }
            }
        })
    }

    async fn call(
        &self,
        args: &Value,
        _legacy_col: &mut SystemCollector,
        _dh: &DataHub,
        _mem: &MemoryManager,
    ) -> anyhow::Result<Value> {
        // Parse optional arguments
        let include_connections = args
            .get("include_connections")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Lock the collector to get exclusive access
        let mut col = self.collector.lock().unwrap();
        
        // Refresh network data
        col.get_networks_mut().refresh(true);
        
        // Refresh processes if we need connections (must be done before getting immutable refs)
        if include_connections {
            col.get_sys_mut().refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        }
        
        // Get immutable references for reading data (after all mutable operations)
        let sys = col.get_sys();
        let networks = col.get_networks();

        // 1. Fetch Volatile Data (Interfaces with traffic & signal)
        let interfaces = network_metrics::scan_interfaces(sys, networks);

        // 2. Fetch Lazy Data (Connections - only if requested)
        let connections = if include_connections {
            net_connections::scan_connections(sys)
        } else {
            Vec::new()
        };

        // 3. Fetch Static Data (DNS & Routing)
        let dns_servers = net_config::get_dns_servers();
        let routes = net_config::get_routing_table();

        // 4. AI Intelligence Layer (Pre-Analysis)
        // Find the primary (most active) interface
        let active_iface = interfaces
            .iter()
            .max_by_key(|i| i.rx_bytes_sec + i.tx_bytes_sec)
            .or_else(|| interfaces.first());

        let (primary_connection, _download_mbps, _upload_mbps, signal_warn, signal_message) = 
            if let Some(iface) = active_iface {
                let download_mbps = (iface.rx_bytes_sec as f64 * 8.0) / 1_000_000.0; // Convert bytes to Mbps
                let upload_mbps = (iface.tx_bytes_sec as f64 * 8.0) / 1_000_000.0;
                
                // Signal strength warning logic
                let signal_warn = iface.signal_strength_dbm.map(|s| s < -75).unwrap_or(false);
                let signal_message = if signal_warn {
                    format!("Signal is very weak ({} dBm). Speed may suffer. Consider moving closer to the router.", 
                            iface.signal_strength_dbm.unwrap_or(0))
                } else if let Some(dbm) = iface.signal_strength_dbm {
                    format!("Signal is healthy ({} dBm).", dbm)
                } else {
                    "Signal strength not applicable (Ethernet connection).".to_string()
                };

                (
                    serde_json::json!({
                        "name": iface.name,
                        "type": iface.connection_type,
                        "mac_address": iface.mac_address,
                        "ipv4_addresses": iface.ipv4_addresses,
                        "ipv6_addresses": iface.ipv6_addresses,
                        "signal": {
                            "strength_dbm": iface.signal_strength_dbm,
                            "rating": iface.signal_quality,
                            "warning": signal_message
                        },
                        "speed": {
                            "download_mbps": format!("{:.2}", download_mbps),
                            "upload_mbps": format!("{:.2}", upload_mbps),
                            "download_bytes_sec": iface.rx_bytes_sec,
                            "upload_bytes_sec": iface.tx_bytes_sec
                        }
                    }),
                    download_mbps,
                    upload_mbps,
                    signal_warn,
                    signal_message,
                )
            } else {
                (
                    serde_json::json!({
                        "name": "No active interface",
                        "type": "Unknown",
                        "signal": { "rating": "N/A" },
                        "speed": { "download_mbps": "0", "upload_mbps": "0" }
                    }),
                    0.0,
                    0.0,
                    false,
                    "No network interfaces detected.".to_string(),
                )
            };

        // Count active connections by state
        // State format is now cleaned (e.g., "Established" not "TcpState::Established")
        let established_count = connections.iter()
            .filter(|c| {
                let state_upper = c.state.to_uppercase();
                state_upper == "ESTABLISHED" || state_upper.contains("ESTABLISHED")
            })
            .count();
        
        let listen_count = connections.iter()
            .filter(|c| {
                let state_upper = c.state.to_uppercase();
                state_upper == "LISTEN" || state_upper.contains("LISTEN")
            })
            .count();
        
        // Debug logging to help diagnose connection issues
        if connections.is_empty() {
            tracing::warn!("No network connections found. This might indicate a permissions issue or no active connections.");
        } else {
            tracing::debug!("Found {} total connections: {} established, {} listening", 
                connections.len(), established_count, listen_count);
        }

        // Aggregate top apps by connection count
        use std::collections::HashMap;
        let mut app_counts: HashMap<String, usize> = HashMap::new();
        for conn in &connections {
            if !conn.process_name.is_empty() && conn.process_name != "System/Kernel" {
                *app_counts.entry(conn.process_name.clone()).or_insert(0) += 1;
            }
        }
        
        let mut top_apps: Vec<_> = app_counts.into_iter().collect();
        top_apps.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by count descending
        top_apps.truncate(10); // Top 10 apps
        
        let top_apps_json: Vec<Value> = top_apps
            .into_iter()
            .map(|(name, count)| {
                serde_json::json!({
                    "name": name,
                    "connections": count
                })
            })
            .collect();

        // 5. Return JSON Optimized for LLM
        Ok(serde_json::json!({
            "status": if interfaces.is_empty() { "No Network" } else { "Active" },
            "primary_connection": primary_connection,
            "usage_analysis": {
                "active_connections_count": established_count,
                "listening_ports_count": listen_count,
                "total_connections": connections.len(),
                "top_apps": top_apps_json,
                "dns_servers": dns_servers,
                "routing_table_entries": routes.len()
            },
            "interfaces": interfaces.iter().map(|i| serde_json::json!({
                "name": i.name,
                "type": i.connection_type,
                "signal_dbm": i.signal_strength_dbm,
                "signal_quality": i.signal_quality,
                "download_mbps": format!("{:.2}", (i.rx_bytes_sec as f64 * 8.0) / 1_000_000.0),
                "upload_mbps": format!("{:.2}", (i.tx_bytes_sec as f64 * 8.0) / 1_000_000.0),
                "download_bytes_sec": i.rx_bytes_sec,
                "upload_bytes_sec": i.tx_bytes_sec
            })).collect::<Vec<_>>(),
            "connections": if include_connections {
                connections.iter().take(100).map(|c| serde_json::json!({
                    "protocol": c.protocol,
                    "local": format!("{}:{}", c.local_ip, c.local_port),
                    "local_ip": c.local_ip,
                    "local_port": c.local_port,
                    "remote": format!("{}:{}", c.remote_ip, c.remote_port),
                    "state": c.state,
                    "process": c.process_name,
                    "category": c.app_category,
                    "pid": c.pid
                })).collect::<Vec<_>>()
            } else {
                Vec::<Value>::new()
            },
            "routing_table": routes.iter().take(20).map(|r| serde_json::json!({
                "destination": r.destination,
                "gateway": r.gateway,
                "interface": r.interface
            })).collect::<Vec<_>>(),
            "warnings": {
                "weak_signal": signal_warn,
                "message": if signal_warn { signal_message } else { "No network issues detected.".to_string() }
            }
        }))
    }
}

