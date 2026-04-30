// ==================================================================================
// 🛡️ HARDWARE UTILITIES - Cross-Platform Helper Functions
// ==================================================================================
// Common utilities used across hardware collectors for safe and efficient
// hardware data collection and parsing.
// ==================================================================================

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use chrono;

/// Common SMART attribute IDs (SATA/ATA)
pub mod smart_attributes {
    pub const REALLOCATED_SECTORS: u8 = 5;
    pub const SPIN_UP_TIME: u8 = 3;
    pub const START_STOP_COUNT: u8 = 4;
    pub const POWER_ON_HOURS: u8 = 9;
    pub const POWER_CYCLE_COUNT: u8 = 12;
    pub const CURRENT_PENDING_SECTORS: u8 = 197;
    pub const OFFLINE_UNCORRECTABLE: u8 = 198;
    pub const TEMPERATURE: u8 = 194;
    pub const SSD_WEAR_LEVEL: u8 = 170;
    pub const SSD_PROGRAM_FAIL: u8 = 171;
    pub const SSD_ERASE_FAIL: u8 = 172;
    pub const SSD_WEAR_RANGE: u8 = 177;
}

/// NVMe log page IDs
pub mod nvme_log_pages {
    pub const SMART_HEALTH: u8 = 0x02;
    pub const ERROR_INFORMATION: u8 = 0x01;
    pub const FIRMWARE_SLOT: u8 = 0x03;
}

/// Parse a hex string to bytes
pub fn parse_hex_string(hex_str: &str) -> Result<Vec<u8>> {
    let clean_hex = hex_str.replace(" ", "").replace("0x", "");
    if clean_hex.len() % 2 != 0 {
        return Err(anyhow!("Invalid hex string length"));
    }
    
    let mut bytes = Vec::new();
    for chunk in clean_hex.as_bytes().chunks(2) {
        let hex_byte = std::str::from_utf8(chunk)?;
        let byte = u8::from_str_radix(hex_byte, 16)?;
        bytes.push(byte);
    }
    
    Ok(bytes)
}

/// Extract a little-endian integer from byte buffer
pub fn extract_le_u32(buffer: &[u8], offset: usize) -> Result<u32> {
    if buffer.len() < offset + 4 {
        return Err(anyhow!("Buffer too short for u32 at offset {}", offset));
    }
    
    Ok(u32::from_le_bytes([
        buffer[offset],
        buffer[offset + 1], 
        buffer[offset + 2],
        buffer[offset + 3]
    ]))
}

/// Extract a little-endian u64 from byte buffer
pub fn extract_le_u64(buffer: &[u8], offset: usize) -> Result<u64> {
    if buffer.len() < offset + 8 {
        return Err(anyhow!("Buffer too short for u64 at offset {}", offset));
    }
    
    Ok(u64::from_le_bytes([
        buffer[offset],
        buffer[offset + 1],
        buffer[offset + 2], 
        buffer[offset + 3],
        buffer[offset + 4],
        buffer[offset + 5],
        buffer[offset + 6],
        buffer[offset + 7]
    ]))
}

/// Convert raw SMART attribute to normalized value
pub fn normalize_smart_attribute(attribute_id: u8, raw_value: u64) -> Option<u64> {
    match attribute_id {
        smart_attributes::TEMPERATURE => {
            // Temperature is usually in the lower 16 bits
            Some(raw_value & 0xFFFF)
        },
        smart_attributes::POWER_ON_HOURS => {
            // Power-on hours, sometimes needs unit conversion
            Some(raw_value)
        },
        smart_attributes::REALLOCATED_SECTORS |
        smart_attributes::CURRENT_PENDING_SECTORS |
        smart_attributes::OFFLINE_UNCORRECTABLE => {
            // Error counts - raw value is usually correct
            Some(raw_value)
        },
        _ => Some(raw_value)
    }
}

/// Calculate health percentage from wear level attribute
pub fn calculate_health_percentage(protocol: &crate::models::hardware_diagnostics::StorageProtocol, 
                                 percentage_value: Option<u8>, 
                                 spare_value: Option<u8>) -> Option<u8> {
    match protocol {
        crate::models::hardware_diagnostics::StorageProtocol::NVMe => {
            // NVMe reports percentage used (0-100, higher is worse)
            percentage_value.map(|p| 100 - p)
        },
        crate::models::hardware_diagnostics::StorageProtocol::SATA => {
            // SATA reports available spare (0-100, lower is worse)
            spare_value
        },
        _ => None
    }
}

/// Parse timestamp from various formats
pub fn parse_timestamp(timestamp_str: &str) -> Result<String> {
    // Try to parse common timestamp formats and convert to ISO 8601
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp_str) {
        return Ok(dt.to_utc().to_rfc3339());
    }
    
    // Try Unix timestamp
    if let Ok(timestamp) = timestamp_str.parse::<i64>() {
        if let Some(dt) = chrono::DateTime::from_timestamp(timestamp, 0) {
            return Ok(dt.to_rfc3339());
        }
    }
    
    // Default to current time if parsing fails
    Ok(chrono::Utc::now().to_rfc3339())
}

/// Validate buffer size for safe parsing
pub fn validate_buffer_size(buffer: &[u8], expected_min_size: usize, context: &str) -> Result<()> {
    if buffer.len() < expected_min_size {
        return Err(anyhow!(
            "{}: Buffer too small (got {} bytes, expected at least {})",
            context, buffer.len(), expected_min_size
        ));
    }
    Ok(())
}

/// Safe string extraction from buffer with null termination
pub fn extract_string(buffer: &[u8], offset: usize, max_len: usize) -> String {
    let end_offset = std::cmp::min(offset + max_len, buffer.len());
    if offset >= buffer.len() {
        return String::new();
    }
    
    let slice = &buffer[offset..end_offset];
    
    // Find null terminator
    let null_pos = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    let string_slice = &slice[..null_pos];
    
    // Convert to string, replacing invalid UTF-8 with replacement chars
    String::from_utf8_lossy(string_slice).trim().to_string()
}

/// LRU Cache implementation for hardware data
pub struct HardwareCache<K, V> {
    cache: std::collections::HashMap<K, (std::time::Instant, V)>,
    max_size: usize,
    ttl: std::time::Duration,
}

impl<K: Clone + Eq + std::hash::Hash, V: Clone> HardwareCache<K, V> {
    pub fn new(max_size: usize, ttl: std::time::Duration) -> Self {
        Self {
            cache: HashMap::new(),
            max_size,
            ttl,
        }
    }
    
    pub fn get(&mut self, key: &K) -> Option<V> {
        if let Some((timestamp, value)) = self.cache.get(key) {
            if timestamp.elapsed() < self.ttl {
                return Some(value.clone());
            } else {
                self.cache.remove(key);
            }
        }
        None
    }
    
    pub fn insert(&mut self, key: K, value: V) {
        // Remove expired entries
        let now = std::time::Instant::now();
        self.cache.retain(|_, (timestamp, _)| now.duration_since(*timestamp) < self.ttl);
        
        // Make room if needed
        if self.cache.len() >= self.max_size {
            if let Some(oldest_key) = self.cache.iter()
                .min_by_key(|(_, (timestamp, _))| *timestamp)
                .map(|(k, _)| k.clone()) {
                self.cache.remove(&oldest_key);
            }
        }
        
        self.cache.insert(key, (now, value));
    }
}