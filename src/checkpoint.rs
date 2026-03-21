//! Checkpoint persistence and recovery utilities for resumable jobs.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::hash::{BuildHasher, RandomState};

/// A snapshot of machine state at job interruption for recovery and resume.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JobCheckpoint {
    /// Unique identifier for this checkpoint
    pub checkpoint_id: String,

    /// Original G-code file metadata
    pub file_hash: String,
    pub file_path: String,
    pub file_name: String,

    /// Job position tracking
    pub current_line: usize,
    pub total_lines: usize,
    pub line_content: String,

    /// Machine state at interruption
    pub machine_state: MachineState,

    /// G-code modal state
    pub modal_state: ModalState,

    /// Work/Tool state
    pub work_offset_system: String,
    pub tool_number: Option<u32>,
    pub tool_length_offset: f64,

    /// Spindle state
    pub spindle: SpindleState,

    /// Feed rate (F value)
    pub feed_rate: f64,

    /// Reason for interruption
    pub interruption_reason: String,

    /// Timestamp of checkpoint
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
/// Machine status snapshot stored in a checkpoint.
pub struct MachineState {
    pub status: String,
    pub mpos: Position,
    pub wpos: Position,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
/// Cartesian position in machine or work coordinates.
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
/// Relevant G-code modal state required for safe resume.
pub struct ModalState {
    pub units: String,           // G20 or G21
    pub distance_mode: String,   // G90 or G91
    pub plane: String,           // G17, G18, or G19
    pub motion_mode: String,     // G0, G1, G2, or G3
    pub feed_mode: String,       // G93, G94, or G95
}

#[derive(Serialize, Deserialize, Clone, Debug)]
/// Spindle runtime state at interruption time.
pub struct SpindleState {
    pub is_active: bool,
    pub rpm: f64,
    pub direction: Option<String>,
}

impl JobCheckpoint {
}

/// Save a checkpoint to disk.
///
/// If `save_path` is empty, the checkpoint is written next to the G-code file
/// using `{stem}.resume.json` naming.
///
/// # Errors
/// Returns an error if serialization or file writing fails.
pub fn save_checkpoint(checkpoint: &JobCheckpoint, save_path: &str) -> Result<(), String> {
    // If no explicit save path provided, derive it from the job file
    let checkpoint_path = if save_path.is_empty() {
        let base = Path::new(&checkpoint.file_path);
        let stem = base
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("checkpoint");
        let parent = base.parent().unwrap_or_else(|| Path::new("."));
        parent.join(format!("{}.resume.json", stem))
    } else {
        PathBuf::from(save_path)
    };

    // Serialize checkpoint to JSON
    let json = serde_json::to_string_pretty(checkpoint).map_err(|e| e.to_string())?;

    // Write to file
    fs::write(&checkpoint_path, json)
        .map_err(|e| format!("Failed to write checkpoint: {}", e))?;

    println!(
        "[Checkpoint] Saved checkpoint to {}",
        checkpoint_path.display()
    );

    Ok(())
}

/// Load a checkpoint from disk.
///
/// # Errors
/// Returns an error if file reading or JSON parsing fails.
pub fn load_checkpoint(checkpoint_path: &str) -> Result<JobCheckpoint, String> {
    let contents = fs::read_to_string(checkpoint_path)
        .map_err(|e| format!("Failed to read checkpoint file: {}", e))?;

    let checkpoint: JobCheckpoint = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse checkpoint JSON: {}", e))?;

    println!(
        "[Checkpoint] Loaded checkpoint from {}: line {} of {}",
        checkpoint_path, checkpoint.current_line, checkpoint.total_lines
    );

    Ok(checkpoint)
}

/// Build the default resume checkpoint path for a G-code file.
pub fn get_resume_checkpoint_path(gcode_path: &str) -> PathBuf {
    let base = Path::new(gcode_path);
    let stem = base
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("checkpoint");
    let parent = base.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{}.resume.json", stem))
}

/// Compute a simple file hash for checkpoint validation.
///
/// # Errors
/// Returns an error if the file cannot be opened or read.
///
/// # Note
/// This uses a non-cryptographic hash. Prefer SHA-256 for stronger guarantees.
pub fn compute_file_hash(file_path: &str) -> Result<String, String> {
    use std::io::Read;

    let mut file = fs::File::open(file_path)
        .map_err(|e| format!("Failed to open file for hashing: {}", e))?;

    let mut contents = Vec::new();
    file.read_to_end(&mut contents)
        .map_err(|e| format!("Failed to read file for hashing: {}", e))?;

    // Simple hash: convert first 16 bytes as hex (in production, use sha256)
    let hash = RandomState::new().hash_one(&contents[..]);
    Ok(format!("{:x}", hash))
}
