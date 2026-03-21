//! # GTaurus Server - FluidNC Machine Control WebSocket Server
//!
//! A standalone WebSocket server for remote monitoring and control of FluidNC CNC machines.
//! Provides real-time G-code streaming, job management, checkpoint/resume functionality,
//! camera integration, and machine file browsing over a single WebSocket connection.
//!
//! ## Security
//!
//! **⚠️ IMPORTANT**: This server has NO built-in authentication. Ensure only trusted clients
//! can access it via firewall rules. Access to this server means full control of your CNC machine.
//!
//! ## Features
//!
//! - Machine information and control (reset, status)
//! - G-code file streaming with pause/resume
//! - Job checkpoints for resuming after interruptions
//! - Toolpath preview and machine movements
//! - Local file browsing and management
//! - Camera integration for machine monitoring
//! - Surface generation (raster toolpaths)
//! - Real-time status broadcasting to all connected clients

mod camera;
mod checkpoint;
mod driver;
mod gcode;
mod surfacing;
mod ws_aux;
mod ws_checkpoint;
mod ws_connection;
mod ws_job_control;
mod ws_local_files;
mod ws_server;
mod ws_surfacing;
mod ws_gcode_streaming;

use driver::{FluidNCDriver, GCodeConnection};
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Arc, Mutex};

/// Tracks the state of the currently active G-code streaming job.
///
/// Shared state between the WebSocket handlers and the streaming thread.
/// Uses atomic types and mutexes for lock-free and thread-safe updates.
///
/// # Fields
///
/// * `status` - Current job status: "idle", "running", "paused", "cancelled", or "completed"
/// * `file_path` - Absolute path of the G-code file being streamed
/// * `total_lines` - Total number of active G-code lines in the job file
/// * `current_line` - Number of lines sent so far
/// * `cancel_flag` - Request to stop the job immediately
/// * `pause_flag` - Request to pause between lines
/// * `generation` - Job generation counter for preempting old streaming threads
/// * `subscribers` - WebSocket client channels for real-time status updates
pub struct JobState {
    /// Current job status: "idle", "running", "paused", "cancelled", or "completed".
    pub status: Mutex<String>,
    /// Absolute path of the file being streamed.
    pub file_path: Mutex<String>,
    /// Total number of active G-code lines (non-empty, non-comment) in the job file.
    pub total_lines: AtomicUsize,
    /// Number of active G-code lines that have been sent so far.
    pub current_line: AtomicUsize,
    /// Set to `true` to request the streaming thread to stop immediately.
    pub cancel_flag: AtomicBool,
    /// Set to `true` to request the streaming thread to pause between lines.
    pub pause_flag: AtomicBool,
    /// Incremented each time a new job starts. The streaming thread holds its own copy and
    /// exits if it finds the value has changed, providing clean preemption without a sleep.
    pub generation: AtomicUsize,
    /// WebSocket client channels that receive real-time `job://status` broadcast events.
    pub subscribers: Mutex<Vec<std::sync::mpsc::Sender<String>>>,
}

impl JobState {
    /// Create a new job state initialized to "idle" with no subscribers.
    pub fn new() -> Self {
        Self {
            status: Mutex::new("idle".to_string()),
            file_path: Mutex::new(String::new()),
            total_lines: AtomicUsize::new(0),
            current_line: AtomicUsize::new(0),
            cancel_flag: AtomicBool::new(false),
            pause_flag: AtomicBool::new(false),
            generation: AtomicUsize::new(0),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    /// Broadcast a JSON string to all subscribed WebSocket clients, pruning dead channels.
    ///
    /// Dead channels (where the client disconnected) are automatically removed.
    ///
    /// # Arguments
    ///
    /// * `msg` - A complete JSON message to send to all subscribers
    pub fn broadcast(&self, msg: &str) {
        if let Ok(mut subs) = self.subscribers.lock() {
            subs.retain(|tx| tx.send(msg.to_string()).is_ok());
        }
    }

    /// Convenience helper: broadcast a `cancelled` status event and update internal status.
    ///
    /// # Arguments
    ///
    /// * `total_lines` - Total lines being cancelled (included in the event)
    pub fn broadcast_cancelled(&self, total_lines: usize) {
        if let Ok(mut s) = self.status.lock() {
            *s = "cancelled".to_string();
        }
        self.broadcast(
            &serde_json::json!({
                "type": "event",
                "event": "job://status",
                "payload": {
                    "status": "cancelled",
                    "currentLine": self.current_line.load(std::sync::atomic::Ordering::SeqCst),
                    "totalLines": total_lines
                }
            })
            .to_string(),
        );
    }

    /// Register a new WebSocket client as a job-event subscriber.
    ///
    /// # Arguments
    ///
    /// * `tx` - The client's channel sender for receiving async status updates
    ///
    /// # Note
    ///
    /// When the client disconnects, the channel will fail on the next `broadcast()` call
    /// and the subscription will be automatically removed.
    pub fn add_subscriber(&self, tx: std::sync::mpsc::Sender<String>) {
        if let Ok(mut subs) = self.subscribers.lock() {
            subs.push(tx);
        }
    }
}

/// Global application state shared across WebSocket connections.
///
/// Contains references to the machine driver and the current job state.
pub struct AppState {
    /// Shared machine driver for G-code transmission and status updates
    pub driver: Arc<Mutex<Box<dyn GCodeConnection>>>,
    /// Shared job state for coordinate between server handlers and streaming thread
    pub job: Arc<JobState>,
}

/// Log a message to stdout and to `gtaurus_server.log` file.
///
/// # Arguments
///
/// * `msg` - Message to log (without timestamp or prefix)
pub fn log_msg(msg: &str) {
    println!("{}", msg);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("gtaurus_server.log")
    {
        let _ = writeln!(f, "{}", msg);
    }
}

/// Log an error message to stderr and to `gtaurus_server.log` file with "ERROR:" prefix.
///
/// # Arguments
///
/// * `msg` - Error message to log (without "ERROR:" prefix)
pub fn log_err(msg: &str) {
    eprintln!("{}", msg);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("gtaurus_server.log")
    {
        let _ = writeln!(f, "ERROR: {}", msg);
    }
}

#[tokio::main]
async fn main() {
    // Attempt auto-setup of Crowsnest if config is missing
    camera::init_camera_service();

    println!(
        r#"
    
     ____ _                                    ____                             
    / ___| |_ __ _ _   _ _ __ _   _ ___       / ___|  ___ _ ____   _____ _ __ 
   | |  _| __/ _` | | | | '__| | | / __| _____\___ \ / _ \ '__\ \ / / _ \ '__|
   | |_| | || (_| | |_| | |  | |_| \__ \_____|___) |  __/ |   \ V /  __/ |   
    \____|\__\__,_|\__,_|_|   \__,_|___/     |____/ \___|_|    \_/ \___|_|   

    SECURE YOUR MACHINE: Ensure a firewall rule is in place so your CNC 
    can only be accessed from your trusted local network.
                                                                             
    "#
    );

    let state = Arc::new(AppState {
        driver: Arc::new(Mutex::new(Box::new(FluidNCDriver::new()))),
        job: Arc::new(JobState::new()),
    });

    ws_server::start_server(state).await;
}
