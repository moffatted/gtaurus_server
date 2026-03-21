//! WebSocket server entrypoint and request dispatcher.
//!
//! Loads runtime config, optionally performs serial auto-connect, and accepts
//! client connections for command/event routing.
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use tungstenite::Message;

use std::fs;

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
                crate::log_err(&format!(
                    "[WS] Auto-connect failed: Could not enumerate serial ports: {}",
                    e
                ));
                None
            }
        }
    };

    if let Some(port) = port_to_use {
        let baud = config.default_baud_rate.unwrap_or(115200);
        crate::log_msg(&format!(
            "[WS] Auto-connecting to {} at {} baud...",
            port, baud
        ));
        if let Ok(mut lock) = state.driver.lock() {
            if let Err(e) = lock.connect_serial(&port, baud) {
                crate::log_err(&format!("[WS] Auto-connect failed on port {}: {}", port, e));
            } else {
                crate::log_msg(&format!("[WS] Successfully auto-connected to {}", port));
                // Send a status query to verify the connection is alive and
                // to immediately populate the app with the controller's state.
                // This is especially important after a power cycle where the
                // controller may be in alarm state.
                if let Err(e) = lock.send_realtime(0x3F) {
                    crate::log_err(&format!(
                        "[WS] Post-connect status query failed: {}. Connection may be stale.",
                        e
                    ));
                }
            }
        }
    }
}

/// Start the WebSocket bridge server and serve requests until process shutdown.
///
/// # Arguments
/// * `state` - Shared application state containing driver and job tracker.
pub async fn start_server(state: Arc<crate::AppState>) {
    let config_path = "server_config.json";

    let config = if let Ok(data) = fs::read_to_string(config_path) {
        if let Ok(parsed) = serde_json::from_str::<ServerConfig>(&data) {
            parsed
        } else {
            crate::log_err(&format!(
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
            crate::log_err(&format!("[WS] Failed to bind to {}: {}", addr, e));
            return;
        }
    };
    crate::log_msg(&format!("[WS] WebSocket Bridge listening on ws://{}", addr));

    // Spin up HTTP static file server if a root is defined
    let web_root = config
        .web_root
        .clone()
        .unwrap_or_else(|| "./public".to_string());
    let http_port = config.http_port.unwrap_or(1420);

    crate::log_msg(&format!(
        "[HTTP] Web Dashboard: http://0.0.0.0:{}",
        http_port
    ));

    // Check if the directory exists, otherwise create it so the server doesn't panic
    if !std::path::Path::new(&web_root).exists() {
        let _ = std::fs::create_dir_all(&web_root);
        crate::log_msg(&format!(
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
                    crate::log_err(&format!("[WS] Handshake failed: {}", e));
                    return;
                }
            };

            crate::log_msg("[WS] New client connected");
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

            crate::log_msg("[WS] Client disconnected");
        });
    }
}

async fn handle_invoke(state: &crate::AppState, cmd: &str, args: Value) -> Result<Value, String> {
    match cmd {
        "list_serial_ports" => {
            crate::ws_connection::list_serial_ports()
        }
        "get_connection_status" => crate::ws_connection::get_connection_status(state),
        "send_gcode" => {
            let code = args["cmd"].as_str().unwrap_or("");
            crate::ws_connection::send_gcode(state, code)
        }
        "send_realtime" => {
            let byte_num = args["byte"].as_u64().unwrap_or(0) as u8;
            crate::ws_connection::send_realtime(state, byte_num)
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
            crate::ws_connection::connect_serial(state, port, baud)
        }
        "connect_telnet" => {
            let host = args["host"].as_str().unwrap_or("");
            let port = args["ws_port"]
                .as_u64()
                .or(args["wsPort"].as_u64())
                .map(|p| p as u16)
                .unwrap_or(23);
            crate::ws_connection::connect_telnet(state, host, port)
        }
        "disconnect" => crate::ws_connection::disconnect(state),
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
            crate::ws_local_files::ensure_dir_exists(path_arg)
        }
        "list_local_files" => {
            let path_arg = args["path"].as_str().unwrap_or("");
            crate::ws_local_files::list_local_files(path_arg)
        }
        "read_local_file" => {
            let path_arg = args["path"].as_str().unwrap_or("");
            let filename = args["filename"].as_str().unwrap_or("");
            crate::ws_local_files::read_local_file(path_arg, filename)
        }
        "save_local_file" => {
            let path_arg = args["path"].as_str().unwrap_or("");
            let filename = args["filename"].as_str().unwrap_or("");
            let content = args["content"].as_str().unwrap_or("");
            crate::ws_local_files::save_local_file(path_arg, filename, content)
        }
        "delete_local_file" => {
            let path_arg = args["path"].as_str().unwrap_or("");
            let filename = args["filename"].as_str().unwrap_or("");
            crate::ws_local_files::delete_local_file(path_arg, filename)
        }
        "copy_to_storage" => {
            let source_path_arg = args["sourcePath"].as_str().unwrap_or("");
            let dest_dir_arg = args["destDir"].as_str().unwrap_or("");
            crate::ws_local_files::copy_to_storage(source_path_arg, dest_dir_arg)
        }
        "get_home_dir" => crate::ws_local_files::get_home_dir(),
        "validate_gcode_file" => {
            let path_arg = args["path"].as_str().unwrap_or("");
            crate::ws_local_files::validate_gcode_file(path_arg)
        }
        "stream_local_gcode" => {
            crate::ws_gcode_streaming::stream_local_gcode(state, &args)
        }
        "get_job_status" => {
            crate::ws_job_control::get_job_status(state)
        }
        "pause_job" => crate::ws_job_control::pause_job(state),
        "resume_job" => crate::ws_job_control::resume_job(state),
        "cancel_job" => crate::ws_job_control::cancel_job(state),
        "get_camera_settings" => crate::ws_aux::get_camera_settings(&args),
        "set_camera_settings" => crate::ws_aux::set_camera_settings(&args),
        "parse_gcode_file" => crate::ws_aux::parse_gcode_file(&args),
        "generate_surfacing_toolpath" => crate::ws_surfacing::generate_surfacing_toolpath(&args),
        "save_checkpoint" => crate::ws_checkpoint::save_checkpoint(&args),
        "load_checkpoint" => crate::ws_checkpoint::load_checkpoint(&args),
        "get_resume_checkpoint_path" => crate::ws_checkpoint::get_resume_checkpoint_path(&args),
        "compute_file_hash" => crate::ws_checkpoint::compute_file_hash(&args),
        _ => Err(format!(
            "Command {} not implemented in standalone server",
            cmd
        )),
    }
}
