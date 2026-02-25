use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
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
            http_port: Some(8080),
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
    log_msg(&format!("[WS] Server listening on ws://{}", addr));

    // Spin up HTTP static file server if a root is defined
    let web_root = config
        .web_root
        .clone()
        .unwrap_or_else(|| "./public".to_string());
    let http_port = config.http_port.unwrap_or(8080);

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

        log_msg(&format!(
            "[HTTP] Dashboard Web Server listening on http://0.0.0.0:{} (serving '{}')",
            http_port, web_root
        ));
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
            let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let driver_clone = state.driver.clone();
            std::thread::spawn(move || {
                println!("[GTaurus Server] Starting G-code stream job...");
                for line in content.lines() {
                    let l = line.trim();
                    if l.is_empty() || l.starts_with(';') || l.starts_with('(') {
                        continue;
                    }
                    if let Ok(mut driver) = driver_clone.lock() {
                        let _ = driver.send_command(l.to_string());
                    }
                }
                println!("[GTaurus Server] Finished streaming G-code job.");
            });
            Ok(serde_json::Value::String("Streaming started".to_string()))
        }
        _ => Err(format!(
            "Command {} not implemented in standalone server",
            cmd
        )),
    }
}
