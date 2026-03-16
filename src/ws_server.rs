/*
 * @file ws_server.rs
 * @purpose WebSocket server implementation handling client connections, command invocation, and file system management for the CNC bridge.
 */
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use tungstenite::Message;

use std::fs::{self, OpenOptions};
use std::io::Write;

fn log_msg(msg: &str) {
    println!("{}", msg);
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("gtaurus_server.log")
    {
        let _ = writeln!(f, "{}", msg);
    }
}

fn log_err(msg: &str) {
    eprintln!("{}", msg);
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("gtaurus_server.log")
    {
        let _ = writeln!(f, "ERROR: {}", msg);
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
#[serde(default)]
struct ServerConfig {
    port: u16,
    auto_connect: bool,
    default_serial_port: Option<String>,
    default_baud_rate: Option<u32>,
    http_port: Option<u16>,
    web_root: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 9001,
            auto_connect: true,
            default_serial_port: None,
            default_baud_rate: Some(115200),
            http_port: Some(1420),
            web_root: Some("./public".to_string()),
        }
    }
}

async fn attempt_auto_connect(state: Arc<crate::AppState>, config: &ServerConfig) {
    let (is_disconnected, is_suspended) = if let Ok(lock) = state.driver.lock() {
        (
            lock.get_status() == "Disconnected",
            lock.is_auto_connect_suspended(),
        )
    } else {
        (false, false)
    };

    if !is_disconnected || is_suspended {
        return;
    }

    let port_to_use = if let Some(p) = config.default_serial_port.clone() {
        Some(p)
    } else {
        match serialport::available_ports() {
            Ok(ports) => {
                if ports.is_empty() {
                    None
                } else {
                    // Grab the first available port
                    Some(ports[0].port_name.clone())
                }
            }
            Err(e) => {
                log_err(&format!(
                    "[WS] Auto-connect failed: Could not enumerate serial ports: {}",
                    e
                ));
                None
            }
        }
    };

    if let Some(port) = port_to_use {
        let baud = config.default_baud_rate.unwrap_or(115200);
        log_msg(&format!(
            "[WS] Auto-connecting to {} at {} baud...",
            port, baud
        ));
        if let Ok(mut lock) = state.driver.lock() {
            if let Err(e) = lock.connect_serial(&port, baud) {
                log_err(&format!("[WS] Auto-connect failed on port {}: {}", port, e));
            } else {
                log_msg(&format!("[WS] Successfully auto-connected to {}", port));
                // Send a status query to verify the connection is alive and
                // to immediately populate the app with the controller's state.
                // This is especially important after a power cycle where the
                // controller may be in alarm state.
                if let Err(e) = lock.send_realtime(0x3F) {
                    log_err(&format!(
                        "[WS] Post-connect status query failed: {}. Connection may be stale.",
                        e
                    ));
                }
            }
        }
    }
}

pub async fn start_server(state: Arc<crate::AppState>) {
    let config_path = "server_config.json";

    let config = if let Ok(data) = fs::read_to_string(config_path) {
        if let Ok(parsed) = serde_json::from_str::<ServerConfig>(&data) {
            parsed
        } else {
            log_err(&format!(
                "[WS] Failed to parse {}, using defaults.",
                config_path
            ));
            ServerConfig::default()
        }
    } else {
        let def = ServerConfig::default();
        if let Ok(json) = serde_json::to_string_pretty(&def) {
            let _ = fs::write(config_path, json);
        }
        def
    };

    if config.auto_connect {
        let state_clone = state.clone();
        let config_clone = config.clone();
        tokio::spawn(async move {
            loop {
                attempt_auto_connect(state_clone.clone(), &config_clone).await;
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });
    }

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            log_err(&format!("[WS] Failed to bind to {}: {}", addr, e));
            return;
        }
    };
    log_msg(&format!("[WS] WebSocket Bridge listening on ws://{}", addr));

    // Spin up HTTP static file server if a root is defined
    let web_root = config
        .web_root
        .clone()
        .unwrap_or_else(|| "./public".to_string());
    let http_port = config.http_port.unwrap_or(1420);

    log_msg(&format!(
        "[HTTP] Web Dashboard: http://0.0.0.0:{}",
        http_port
    ));

    // Check if the directory exists, otherwise create it so the server doesn't panic
    if !std::path::Path::new(&web_root).exists() {
        let _ = std::fs::create_dir_all(&web_root);
        log_msg(&format!(
            "[HTTP] Created empty web root directory: {}",
            web_root
        ));
    }

    tokio::spawn(async move {
        use warp::Filter;
        let routes = warp::fs::dir(web_root.clone()).with(warp::cors().allow_any_origin());

        warp::serve(routes).run(([0, 0, 0, 0], http_port)).await;
    });

    while let Ok((stream, _)) = listener.accept().await {
        let state_clone = state.clone();
        tokio::spawn(async move {
            let ws_stream = match accept_async(stream).await {
                Ok(ws) => ws,
                Err(e) => {
                    log_err(&format!("[WS] Handshake failed: {}", e));
                    return;
                }
            };

            log_msg("[WS] New client connected");
            let (mut ws_tx, mut ws_rx) = ws_stream.split();

            let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<Value>(32);

            let (driver_rx_tx, driver_rx_rx) = std::sync::mpsc::channel::<String>();
            {
                if let Ok(mut lock) = state_clone.driver.lock() {
                    lock.add_rx_subscriber(driver_rx_tx);
                };
            }

            let event_tx_clone = event_tx.clone();
            tokio::task::spawn_blocking(move || {
                while let Ok(line) = driver_rx_rx.recv() {
                    let _ = event_tx_clone.blocking_send(serde_json::json!({
                        "type": "event",
                        "event": "fluidnc://rx",
                        "payload": line
                    }));
                }
            });

            // Subscribe this client to real-time job progress broadcasts.
            let (job_rx_tx, job_rx_rx) = std::sync::mpsc::channel::<String>();
            state_clone.job.add_subscriber(job_rx_tx);

            let event_tx_job = event_tx.clone();
            tokio::task::spawn_blocking(move || {
                while let Ok(msg) = job_rx_rx.recv() {
                    let _ = event_tx_job.blocking_send(
                        serde_json::from_str::<Value>(&msg).unwrap_or(serde_json::json!({
                            "type": "event",
                            "event": "job://status",
                            "payload": msg
                        })),
                    );
                }
            });

            let mut write_task = tokio::spawn(async move {
                while let Some(msg) = event_rx.recv().await {
                    if ws_tx
                        .send(Message::Text(msg.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });

            let read_state = state_clone.clone();
            let event_tx_read = event_tx.clone();
            let mut read_task = tokio::spawn(async move {
                while let Some(msg) = ws_rx.next().await {
                    if let Ok(Message::Text(txt)) = msg {
                        if let Ok(req) = serde_json::from_str::<Value>(&txt) {
                            if req["type"] == "invoke" {
                                let id = req["id"].as_str().unwrap_or("").to_string();
                                let cmd = req["cmd"].as_str().unwrap_or("");
                                let args = req["args"].clone();

                                let response = handle_invoke(&read_state, cmd, args).await;

                                let resp_msg = match response {
                                    Ok(payload) => serde_json::json!({
                                        "type": "response",
                                        "id": id,
                                        "payload": payload
                                    }),
                                    Err(err) => serde_json::json!({
                                        "type": "response",
                                        "id": id,
                                        "error": err
                                    }),
                                };
                                let _ = event_tx_read.send(resp_msg).await;
                            }
                        }
                    }
                }
            });

            tokio::select! {
                _ = &mut write_task => {},
                _ = &mut read_task => {},
            };

            log_msg("[WS] Client disconnected");
        });
    }
}

fn get_resolved_path(path_arg: &str) -> std::path::PathBuf {
    let pb = std::path::PathBuf::from(path_arg);

    // Check if the path is "dirty" (e.g. Windows path on Linux or vice-versa)
    let is_windows_path = path_arg.contains(':') || path_arg.contains('\\');
    let is_host_windows = cfg!(windows);

    // If path is empty, or clearly from the wrong OS, default to $HOME/gcode_files
    if path_arg.is_empty()
        || (is_windows_path && !is_host_windows)
        || (!is_windows_path && is_host_windows && !path_arg.starts_with('\\'))
    {
        let home = if is_host_windows {
            std::env::var("USERPROFILE").unwrap_or_else(|_| "C:/".to_string())
        } else {
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
        };
        let mut base = std::path::PathBuf::from(home);
        base.push("gcode_files");
        return base;
    }

    pb
}

async fn handle_invoke(state: &crate::AppState, cmd: &str, args: Value) -> Result<Value, String> {
    match cmd {
        "list_serial_ports" => {
            let ports = match serialport::available_ports() {
                Ok(ports) => ports
                    .into_iter()
                    .map(|p| p.port_name)
                    .collect::<Vec<String>>(),
                Err(_) => vec![],
            };
            Ok(serde_json::to_value(ports).unwrap())
        }
        "get_connection_status" => {
            let driver = state.driver.lock().map_err(|_| "Lock failed")?;
            Ok(serde_json::Value::String(driver.get_status()))
        }
        "send_gcode" => {
            let code = args["cmd"].as_str().unwrap_or("");
            let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
            driver.send_command(code.to_string())?;
            Ok(serde_json::Value::Null)
        }
        "send_realtime" => {
            let byte_num = args["byte"].as_u64().unwrap_or(0) as u8;
            let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
            driver.send_realtime(byte_num)?;
            Ok(serde_json::Value::Null)
        }
        "connect_serial" => {
            let port = args["port_name"]
                .as_str()
                .or(args["portName"].as_str())
                .unwrap_or("");
            let baud = args["baud_rate"]
                .as_u64()
                .or(args["baudRate"].as_u64())
                .unwrap_or(115200) as u32;
            let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
            driver.set_auto_connect_suspended(false);
            driver.connect_serial(port, baud)?;
            Ok(serde_json::Value::String(format!("Connected to {}", port)))
        }
        "connect_telnet" => {
            let host = args["host"].as_str().unwrap_or("");
            let port = args["ws_port"]
                .as_u64()
                .or(args["wsPort"].as_u64())
                .map(|p| p as u16)
                .unwrap_or(23);
            let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
            driver.set_auto_connect_suspended(false);
            driver.connect_telnet(host, port)?;
            Ok(serde_json::Value::String(format!(
                "Connected to {}:{}",
                host, port
            )))
        }
        "disconnect" => {
            let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
            driver.disconnect();
            driver.set_auto_connect_suspended(true);
            Ok(serde_json::Value::Null)
        }
        "resume_auto_connect" => {
            let config_path = "server_config.json";
            let config = std::fs::read_to_string(config_path)
                .map(|data| serde_json::from_str::<ServerConfig>(&data).unwrap_or_default())
                .unwrap_or_default();

            let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
            driver.set_auto_connect_suspended(false);

            if driver.get_status() == "Disconnected" {
                let port_to_use = if let Some(p) = config.default_serial_port.clone() {
                    Some(p)
                } else {
                    match serialport::available_ports() {
                        Ok(ports) if !ports.is_empty() => Some(ports[0].port_name.clone()),
                        _ => None,
                    }
                };

                if let Some(port) = port_to_use {
                    let baud = config.default_baud_rate.unwrap_or(115200);
                    let _ = driver.connect_serial(&port, baud);
                }
            }

            Ok(serde_json::Value::String(
                "Auto-connect resumed".to_string(),
            ))
        }
        "ensure_dir_exists" => {
            let path_arg = args["path"].as_str().unwrap_or("");
            let path = get_resolved_path(path_arg);
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            Ok(serde_json::Value::Null)
        }
        "list_local_files" => {
            let path_arg = args["path"].as_str().unwrap_or("");
            let path = get_resolved_path(path_arg);
            println!(
                "[GTaurus Server] list_local_files: resolved_path={:?}",
                path
            );

            // No longer need to manually check for ':' as get_resolved_path handles it
            let mut files = Vec::new();
            let entries = std::fs::read_dir(&path).map_err(|e| {
                println!("[GTaurus Server] Failed to read dir: {:?} - {}", path, e);
                e.to_string()
            })?;
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        let modified = metadata
                            .modified()
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        files.push(serde_json::json!({
                            "name": entry.file_name().to_string_lossy().to_string(),
                            "size": metadata.len(),
                            "modified": modified
                        }));
                    }
                }
            }
            Ok(serde_json::Value::Array(files))
        }
        "read_local_file" => {
            let path_arg = args["path"].as_str().unwrap_or("");
            let filename = args["filename"].as_str().unwrap_or("");
            let mut full_path = get_resolved_path(path_arg);
            full_path.push(filename);
            let content = std::fs::read_to_string(full_path).map_err(|e| e.to_string())?;
            Ok(serde_json::Value::String(content))
        }
        "save_local_file" => {
            let path_arg = args["path"].as_str().unwrap_or("");
            let filename = args["filename"].as_str().unwrap_or("");
            let content = args["content"].as_str().unwrap_or("");

            let path = get_resolved_path(path_arg);

            println!(
                "[GTaurus Server] save_local_file: path={:?}, filename={:?}, size={}",
                path,
                filename,
                content.len()
            );
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            let mut full_path = path;
            full_path.push(filename);
            std::fs::write(&full_path, content).map_err(|e| e.to_string())?;
            println!("[GTaurus Server] Successfully saved: {:?}", full_path);
            Ok(serde_json::Value::Null)
        }
        "delete_local_file" => {
            let path_arg = args["path"].as_str().unwrap_or("");
            let filename = args["filename"].as_str().unwrap_or("");
            let mut full_path = get_resolved_path(path_arg);
            full_path.push(filename);
            std::fs::remove_file(full_path).map_err(|e| e.to_string())?;
            Ok(serde_json::Value::Null)
        }
        "copy_to_storage" => {
            let source_path_arg = args["sourcePath"].as_str().unwrap_or("");
            let dest_dir_arg = args["destDir"].as_str().unwrap_or("");
            let source = std::path::PathBuf::from(source_path_arg); // Source path is absolute, not resolved
            if let Some(filename) = source.file_name() {
                let dest_path = get_resolved_path(dest_dir_arg);
                std::fs::create_dir_all(&dest_path).map_err(|e| e.to_string())?;
                let mut dest = dest_path;
                dest.push(filename);
                std::fs::copy(source, dest).map_err(|e| e.to_string())?;
                Ok(serde_json::Value::Null)
            } else {
                Err("Invalid filename".to_string())
            }
        }
        "get_home_dir" => {
            let home = if cfg!(windows) {
                std::env::var("USERPROFILE").unwrap_or_else(|_| "C:/".to_string())
            } else {
                std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
            };
            Ok(serde_json::Value::String(home))
        }
        "validate_gcode_file" => {
            let path_arg = args["path"].as_str().unwrap_or("");
            let path = get_resolved_path(path_arg);
            let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            let is_gcode = content.lines().any(|line| {
                let l = line.trim();
                if l.is_empty() || l.starts_with(';') || l.starts_with('(') {
                    return false;
                }
                l.starts_with('G')
                    || l.starts_with('M')
                    || l.starts_with('X')
                    || l.starts_with('Y')
                    || l.starts_with('Z')
                    || l.starts_with('$')
                    || l.starts_with('F')
                    || l.starts_with('S')
                    || l.starts_with('T')
            });
            Ok(serde_json::Value::Bool(is_gcode))
        }
        "stream_local_gcode" => {
            let path = args["path"].as_str().unwrap_or("").to_string();
            let feed_override: Option<f64> = args["feedRateOverride"].as_f64();
            // 1-indexed file line to start from. 0 or 1 means start from the beginning.
            let start_line: usize = args["startLine"].as_u64().unwrap_or(0) as usize;

            let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;

            // Count the active G-code lines so the frontend can display progress.
            let total_active: usize = content
                .lines()
                .filter(|l| {
                    let t = l.trim();
                    !t.is_empty() && !t.starts_with(';') && !t.starts_with('(')
                })
                .count();

            let driver_clone = state.driver.clone();
            let job = state.job.clone();

            // Signal any currently running job to stop before starting a new one.
            // Incrementing the generation causes the old streaming thread to detect the
            // mismatch and exit without needing a sleep-based race.
            let my_gen = job.generation.fetch_add(1, Ordering::SeqCst) + 1;

            // Reset job state for the new run.
            job.cancel_flag.store(false, Ordering::SeqCst);
            job.pause_flag.store(false, Ordering::SeqCst);
            job.total_lines.store(total_active, Ordering::SeqCst);
            job.current_line.store(0, Ordering::SeqCst);
            if let Ok(mut s) = job.status.lock() {
                *s = "running".to_string();
            }
            if let Ok(mut fp) = job.file_path.lock() {
                *fp = path.clone();
            }

            // Broadcast the initial "running" status to all connected clients.
            job.broadcast(
                &serde_json::json!({
                    "type": "event",
                    "event": "job://status",
                    "payload": {
                        "status": "running",
                        "filePath": path,
                        "currentLine": 0,
                        "totalLines": total_active,
                        "startLine": start_line
                    }
                })
                .to_string(),
            );

            std::thread::spawn(move || {
                log_msg(&format!(
                    "[GTaurus Server] Starting G-code stream job (startLine={}, totalLines={})...",
                    start_line, total_active
                ));

                let mut has_sent_initial_f = false;
                let mut active_line_count: usize = 0;

                for (file_line_idx, line) in content.lines().enumerate() {
                    // Exit if a new job has superseded this one or if explicitly cancelled.
                    if job.generation.load(Ordering::SeqCst) != my_gen
                        || job.cancel_flag.load(Ordering::SeqCst)
                    {
                        log_msg(&format!(
                            "[GTaurus Server] Job cancelled at file line {}.",
                            file_line_idx + 1
                        ));
                        job.broadcast_cancelled(total_active);
                        return;
                    }

                    // Skip lines before the requested start position (1-indexed; 0 means from
                    // the beginning, equivalent to 1).
                    let effective_start = if start_line == 0 { 1 } else { start_line };
                    if (file_line_idx + 1) < effective_start {
                        continue;
                    }

                    let l = line.trim();
                    if l.is_empty() || l.starts_with(';') || l.starts_with('(') {
                        continue;
                    }

                    // Wait while the job is paused, checking for cancellation each tick.
                    while job.pause_flag.load(Ordering::SeqCst) {
                        if job.generation.load(Ordering::SeqCst) != my_gen
                            || job.cancel_flag.load(Ordering::SeqCst)
                        {
                            job.broadcast_cancelled(total_active);
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }

                    // Apply optional feed-rate override.
                    let mut final_line = l.to_string();
                    if let Some(target_f) = feed_override {
                        if l.contains('F') || l.contains('f') {
                            let parts: Vec<&str> = l.split_whitespace().collect();
                            let mut new_parts = Vec::new();
                            for p in parts {
                                if p.starts_with('F') || p.starts_with('f') {
                                    new_parts.push(format!("F{:.1}", target_f));
                                } else {
                                    new_parts.push(p.to_string());
                                }
                            }
                            final_line = new_parts.join(" ");
                            has_sent_initial_f = true;
                        } else if (l.contains("G1") || l.contains("G2") || l.contains("G3"))
                            && !has_sent_initial_f
                        {
                            final_line = format!("{} F{:.1}", l, target_f);
                            has_sent_initial_f = true;
                        }
                    }

                    if let Ok(mut driver) = driver_clone.lock() {
                        let _ = driver.send_command(final_line);
                    }

                    active_line_count += 1;
                    job.current_line.store(active_line_count, Ordering::SeqCst);

                    // Throttle progress broadcasts to every 10 lines to avoid flooding clients
                    // on large files, while still providing responsive feedback.
                    if active_line_count % 10 == 0 {
                        job.broadcast(
                            &serde_json::json!({
                                "type": "event",
                                "event": "job://status",
                                "payload": {
                                    "status": "running",
                                    "currentLine": active_line_count,
                                    "totalLines": total_active
                                }
                            })
                            .to_string(),
                        );
                    }
                }

                if let Ok(mut s) = job.status.lock() {
                    *s = "completed".to_string();
                }
                job.broadcast(
                    &serde_json::json!({
                        "type": "event",
                        "event": "job://status",
                        "payload": {
                            "status": "completed",
                            "currentLine": active_line_count,
                            "totalLines": total_active
                        }
                    })
                    .to_string(),
                );
                log_msg("[GTaurus Server] Finished streaming G-code job.");
            });
            Ok(serde_json::Value::String("Streaming started".to_string()))
        }
        "get_job_status" => {
            let status = state
                .job
                .status
                .lock()
                .map(|s| s.clone())
                .unwrap_or_else(|_| "unknown".to_string());
            let file_path = state
                .job
                .file_path
                .lock()
                .map(|s| s.clone())
                .unwrap_or_default();
            let current_line = state.job.current_line.load(Ordering::SeqCst);
            let total_lines = state.job.total_lines.load(Ordering::SeqCst);
            Ok(serde_json::json!({
                "status": status,
                "filePath": file_path,
                "currentLine": current_line,
                "totalLines": total_lines
            }))
        }
        "pause_job" => {
            // Software: stop streaming thread from sending more commands.
            state.job.pause_flag.store(true, Ordering::SeqCst);
            if let Ok(mut s) = state.job.status.lock() {
                *s = "paused".to_string();
            }
            // Hardware (FLUIDNC / GRBL 1.1): send Feed Hold realtime command.
            if let Ok(mut driver) = state.driver.lock() {
                let _ = driver.send_realtime(0x21); // '!'
            }
            state.job.broadcast(
                &serde_json::json!({
                    "type": "event",
                    "event": "job://status",
                    "payload": {
                        "status": "paused",
                        "currentLine": state.job.current_line.load(Ordering::SeqCst),
                        "totalLines": state.job.total_lines.load(Ordering::SeqCst)
                    }
                })
                .to_string(),
            );
            Ok(serde_json::Value::String("Job paused".to_string()))
        }
        "resume_job" => {
            // Software: allow the streaming thread to continue sending commands.
            state.job.pause_flag.store(false, Ordering::SeqCst);
            if let Ok(mut s) = state.job.status.lock() {
                *s = "running".to_string();
            }
            // Hardware (FLUIDNC / GRBL 1.1): send Cycle Start realtime command.
            if let Ok(mut driver) = state.driver.lock() {
                let _ = driver.send_realtime(0x7E); // '~'
            }
            state.job.broadcast(
                &serde_json::json!({
                    "type": "event",
                    "event": "job://status",
                    "payload": {
                        "status": "running",
                        "currentLine": state.job.current_line.load(Ordering::SeqCst),
                        "totalLines": state.job.total_lines.load(Ordering::SeqCst)
                    }
                })
                .to_string(),
            );
            Ok(serde_json::Value::String("Job resumed".to_string()))
        }
        "cancel_job" => {
            // Software: signal the streaming thread to exit on its next iteration.
            // Clearing pause_flag first ensures a paused thread wakes up and sees cancel_flag.
            state.job.pause_flag.store(false, Ordering::SeqCst);
            state.job.cancel_flag.store(true, Ordering::SeqCst);
            // Immediately broadcast the cancelled status so clients don't wait for the
            // streaming thread to observe the flag on its next iteration.
            let total = state.job.total_lines.load(Ordering::SeqCst);
            state.job.broadcast_cancelled(total);
            // Hardware (FLUIDNC / GRBL 1.1): Soft Reset clears the motion buffer immediately.
            if let Ok(mut driver) = state.driver.lock() {
                let _ = driver.send_realtime(0x18); // Ctrl-X
            }
            Ok(serde_json::Value::String("Job cancelled".to_string()))
        }
        "get_camera_settings" => {
            let config_path = args["configPath"].as_str().unwrap_or("");
            crate::camera::get_camera_settings(config_path)
        }
        "set_camera_settings" => {
            let config_path = args["configPath"].as_str().unwrap_or("");
            let updates = args["updates"].clone();
            crate::camera::set_camera_settings(config_path, updates)
        }
        "parse_gcode_file" => {
            let path = args["path"]
                .as_str()
                .or(args["filePath"].as_str())
                .unwrap_or("")
                .to_string();
            let analysis = crate::gcode::parse_gcode_file_impl(path)?;
            serde_json::to_value(analysis).map_err(|e| e.to_string())
        }
        "generate_surfacing_toolpath" => {
            let width = args["width"].as_f64().unwrap_or(0.0) as f32;
            let height = args["height"].as_f64().unwrap_or(0.0) as f32;
            let total_depth = args["totalDepth"]
                .as_f64()
                .or(args["total_depth"].as_f64())
                .unwrap_or(0.0) as f32;
            let depth_per_pass = args["depthPerPass"]
                .as_f64()
                .or(args["depth_per_pass"].as_f64())
                .unwrap_or(0.0) as f32;
            let stepover = args["stepover"].as_f64().unwrap_or(0.0) as f32;
            let angle_deg = args["angleDeg"]
                .as_f64()
                .or(args["angle_deg"].as_f64())
                .unwrap_or(0.0) as f32;
            let overtravel = args["overtravel"].as_f64().unwrap_or(0.0) as f32;
            let bidirectional = args["bidirectional"].as_bool().unwrap_or(true);
            let safe_z = args["safeZ"]
                .as_f64()
                .or(args["safe_z"].as_f64())
                .unwrap_or(5.0) as f32;
            let feedrate = args["feedrate"].as_f64().unwrap_or(0.0) as f32;
            let plunge_rate = args["plungeRate"]
                .as_f64()
                .or(args["plunge_rate"].as_f64())
                .unwrap_or(0.0) as f32;
            let spindle_rpm = args["spindleRpm"]
                .as_u64()
                .or(args["spindle_rpm"].as_u64())
                .unwrap_or(0) as u32;
            let finish_pass = args["finishPass"]
                .as_bool()
                .or(args["finish_pass"].as_bool())
                .unwrap_or(false);
            let use_inches = args["useInches"]
                .as_bool()
                .or(args["use_inches"].as_bool())
                .unwrap_or(false);
            let origin = args["origin"].as_str().unwrap_or("front_left").to_string();

            crate::surfacing::generate_surfacing_toolpath(
                width,
                height,
                total_depth,
                depth_per_pass,
                stepover,
                angle_deg,
                overtravel,
                bidirectional,
                safe_z,
                feedrate,
                plunge_rate,
                spindle_rpm,
                finish_pass,
                use_inches,
                origin,
            )
            .map(serde_json::Value::String)
        }
        "save_checkpoint" => {
            let checkpoint_data = args.get("checkpoint").cloned();
            let save_path = args["savePath"].as_str().unwrap_or("").to_string();

            if let Some(checkpoint_json) = checkpoint_data {
                match serde_json::from_value::<crate::checkpoint::JobCheckpoint>(checkpoint_json) {
                    Ok(checkpoint) => {
                        match crate::checkpoint::save_checkpoint(&checkpoint, &save_path) {
                            Ok(_) => {
                                log_msg(&format!(
                                    "[Checkpoint] Successfully saved checkpoint for {}",
                                    checkpoint.file_name
                                ));
                                Ok(serde_json::Value::String(format!(
                                    "Checkpoint saved: {}",
                                    checkpoint.checkpoint_id
                                )))
                            }
                            Err(e) => {
                                log_err(&format!("[Checkpoint] Failed to save: {}", e));
                                Err(e)
                            }
                        }
                    }
                    Err(e) => {
                        let err_msg = format!("Invalid checkpoint data: {}", e);
                        log_err(&err_msg);
                        Err(err_msg)
                    }
                }
            } else {
                Err("No checkpoint data provided".to_string())
            }
        }
        "load_checkpoint" => {
            let checkpoint_path = args["checkpointPath"]
                .as_str()
                .or(args["path"].as_str())
                .or(args["filePath"].as_str())
                .unwrap_or("")
                .to_string();

            if checkpoint_path.is_empty() {
                return Err("No checkpoint path provided".to_string());
            }

            match crate::checkpoint::load_checkpoint(&checkpoint_path) {
                Ok(checkpoint) => {
                    log_msg(&format!(
                        "[Checkpoint] Loaded checkpoint: {} (line {}/{})",
                        checkpoint.file_name, checkpoint.current_line, checkpoint.total_lines
                    ));
                    serde_json::to_value(checkpoint).map_err(|e| e.to_string())
                }
                Err(e) => {
                    log_err(&format!("[Checkpoint] Failed to load: {}", e));
                    Err(e)
                }
            }
        }
        "get_resume_checkpoint_path" => {
            let gcode_path = args["path"]
                .as_str()
                .or(args["filePath"].as_str())
                .unwrap_or("")
                .to_string();

            if gcode_path.is_empty() {
                return Err("No G-code path provided".to_string());
            }

            let checkpoint_path = crate::checkpoint::get_resume_checkpoint_path(&gcode_path);
            Ok(serde_json::Value::String(
                checkpoint_path.to_string_lossy().to_string(),
            ))
        }
        "compute_file_hash" => {
            let file_path = args["path"]
                .as_str()
                .or(args["filePath"].as_str())
                .unwrap_or("")
                .to_string();

            if file_path.is_empty() {
                return Err("No file path provided".to_string());
            }

            match crate::checkpoint::compute_file_hash(&file_path) {
                Ok(hash) => Ok(serde_json::Value::String(hash)),
                Err(e) => Err(e),
            }
        }
        _ => Err(format!(
            "Command {} not implemented in standalone server",
            cmd
        )),
    }
}
