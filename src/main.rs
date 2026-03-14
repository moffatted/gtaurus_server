/*
 * @file main.rs
 * @purpose Entry point for the standalone server, initializing the application state and starting the WebSocket server.
 */
mod camera;
mod driver;
mod ws_server;

use driver::{FluidNCDriver, GCodeConnection};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Arc, Mutex};

/// Tracks the state of the currently active G-code streaming job.
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
    pub fn broadcast(&self, msg: &str) {
        if let Ok(mut subs) = self.subscribers.lock() {
            subs.retain(|tx| tx.send(msg.to_string()).is_ok());
        }
    }

    /// Convenience helper: broadcast a `cancelled` status event and update internal status.
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
    pub fn add_subscriber(&self, tx: std::sync::mpsc::Sender<String>) {
        if let Ok(mut subs) = self.subscribers.lock() {
            subs.push(tx);
        }
    }
}

pub struct AppState {
    pub driver: Arc<Mutex<Box<dyn GCodeConnection>>>,
    pub job: Arc<JobState>,
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
