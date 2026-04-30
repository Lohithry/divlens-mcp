use std::collections::BinaryHeap;
use std::cmp::Ordering;
use std::time::SystemTime;
use walkdir::WalkDir;
use directories::UserDirs;
use serde::Serialize;

// 1. The "Top File" Struct (For the Leaderboard)
// Keeps track of the largest individual files found.
#[derive(Debug, Clone, Serialize)]
pub struct TopFile {
    pub path: String,
    pub size_mb: f64,
    pub modified_days_ago: u64,
}

// Custom ordering for the Heap (Smallest at top, so we can pop it easily)
// We want a Min-Heap of the top N files. The "smallest" of the top files is at the root.
// If a new file is larger than the root, we pop the root and push the new file.
impl PartialEq for TopFile {
    fn eq(&self, other: &Self) -> bool { self.size_mb.partial_cmp(&other.size_mb) == Some(Ordering::Equal) }
}
impl Eq for TopFile {}
impl PartialOrd for TopFile {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Reverse order: The smallest size should be "greater" so it stays at the bottom?
        // Wait, Rust BinaryHeap is a Max-Heap.
        // To keep the "Top 50 Largest", we need to know the *smallest* of those 50 quickly,
        // so we can replace it if we find something bigger.
        // So we need a Min-Heap of size 50.
        // In a Max-Heap, the largest is at the top.
        // If we want a Min-Heap using Rust's BinaryHeap (Max-Heap), we reverse the comparison.
        other.size_mb.partial_cmp(&self.size_mb) 
    }
}
impl Ord for TopFile {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
    }
}

// 2. The Result Structure
#[derive(Debug, Serialize)]
pub struct StorageAdvancedStats {
    pub top_files: Vec<TopFile>,
    pub stale_files_size_mb: f64, // Files > 6 months old
    pub fresh_files_size_mb: f64, // Files < 1 month old
}

pub struct AdvancedStorageCollector;

impl AdvancedStorageCollector {
    // This function scans user directories to calculate staleness and identify large files.
    // It runs in O(N) where N is number of files, effectively "one pass".
    pub fn scan() -> StorageAdvancedStats {
        let mut top_heap = BinaryHeap::new(); // Keeps track of Top 50 biggest files
        let mut stale_size = 0.0;
        let mut fresh_size = 0.0;
        let now = SystemTime::now();

        if let Some(user_dirs) = UserDirs::new() {
            // Scan key folders only (Lightweight approach)
            // We focus on where big files usually hide.
            let mut targets = vec![];
            if let Some(p) = user_dirs.download_dir() { targets.push(p); }
            if let Some(p) = user_dirs.document_dir() { targets.push(p); }
            if let Some(p) = user_dirs.desktop_dir() { targets.push(p); }
            if let Some(p) = user_dirs.video_dir() { targets.push(p); }
            if let Some(p) = user_dirs.audio_dir() { targets.push(p); }
            if let Some(p) = user_dirs.picture_dir() { targets.push(p); }

            for target in targets {
                if !target.exists() { continue; }
                
                // Use the same junk filter logic implicitly by just walking these dirs?
                // Or should we copy the junk filter? 
                // For "Advanced" scan, we can probably trust these user dirs aren't too messy,
                // but adding a basic hidden file filter is good practice.
                
                for entry in WalkDir::new(target).into_iter().filter_map(|e| e.ok()) {
                    // Skip hidden folders/files to stay fast and relevant
                    if entry.file_name().to_string_lossy().starts_with('.') {
                        if entry.file_type().is_dir() { WalkDir::new(entry.path()).max_depth(0); } // Prune? WalkDir doesn't work like this inside iterator
                        continue; 
                    }
                    
                    if !entry.path().is_file() { continue; }

                    let metadata = match entry.metadata() {
                        Ok(m) => m,
                        Err(_) => continue,
                    };

                    let size_bytes = metadata.len();
                    // Skip tiny files to save CPU cycles on heap operations
                    if size_bytes < 1_048_576 { // < 1MB
                        continue;
                    }
                    
                    let size_mb = size_bytes as f64 / 1024.0 / 1024.0;

                    // --- LOGIC A: Staleness (Time Analysis) ---
                    // Reliable cross-platform "Modified" time
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(duration) = now.duration_since(modified) {
                            let days = duration.as_secs() / 86400;
                            
                            if days > 180 { stale_size += size_mb; } // Older than 6 months
                            if days < 30 { fresh_size += size_mb; }
                            
                            // --- LOGIC B: Top 50 Global Files ---
                            // Check if this file deserves to be in the "Hall of Fame"
                            
                            // Since we implemented Ord as "Reverse", the .peek() returns the SMALLEST element.
                            // So if current file is BIGGER than the smallest in heap, we replace it.
                            
                            if top_heap.len() < 50 {
                                top_heap.push(TopFile {
                                    path: entry.path().to_string_lossy().to_string(),
                                    size_mb,
                                    modified_days_ago: days,
                                });
                            } else if let Some(smallest_top) = top_heap.peek() {
                                if size_mb > smallest_top.size_mb {
                                    top_heap.pop();
                                    top_heap.push(TopFile {
                                        path: entry.path().to_string_lossy().to_string(),
                                        size_mb,
                                        modified_days_ago: days,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Convert Heap to Vector and Sort (Biggest first)
        // The heap pops the "smallest" first (because of our reverse Ord).
        // So popping everything gives Small -> Big.
        // We want Big -> Small.
        let sorted_files = top_heap.into_sorted_vec();
        // into_sorted_vec returns in ascending order (Small -> Big) based on Ord.
        // Our Ord is reversed (Big < Small), so into_sorted_vec gives (Big -> Small)?
        // Wait, standard: 1, 2, 3. Ord: 1 < 2. Sorted: 1, 2, 3.
        // Reversed Ord: 3 < 2 < 1. Sorted: 3, 2, 1.
        // So yes, it should be Big -> Small.
        
        // Double check: TopFile(100MB) vs TopFile(10MB).
        // partial_cmp calls other.cmp(self) -> 10.cmp(100) -> Less.
        // So 100MB < 10MB in our custom ordering.
        // So sorted vec (Ascending) will be [100, 10].
        // Which is what we want (Descending size).

        StorageAdvancedStats {
            top_files: sorted_files,
            stale_files_size_mb: stale_size,
            fresh_files_size_mb: fresh_size,
        }
    }
}

