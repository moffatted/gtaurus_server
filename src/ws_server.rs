use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use tungstenite::Message;

pub async fn start_server(state: Arc<crate::AppState>) {
    let addr = "0.0.0.0:9001";
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[WS] Failed to bind to {}: {}", addr, e);
            return;
        }
    };
    println!("[WS] Server listening on ws://{}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        let state_clone = state.clone();
        tokio::spawn(async move {
            let ws_stream = match accept_async(stream).await {
                Ok(ws) => ws,
                Err(e) => {
                    eprintln!("[WS] Handshake failed: {}", e);
                    return;
                }
            };

            println!("[WS] New client connected");
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
                    if ws_tx.send(Message::Text(msg.to_string().into())).await.is_err() {
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

            println!("[WS] Client disconnected");
        });
    }
}

async fn handle_invoke(state: &crate::AppState, cmd: &str, args: Value) -> Result<Value, String> {
    match cmd {
        "list_serial_ports" => {
            let ports = match serialport::available_ports() {
                Ok(ports) => ports.into_iter().map(|p| p.port_name).collect::<Vec<String>>(),
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
            let port = args["port_name"].as_str().unwrap_or("");
            let baud = args["baud_rate"].as_u64().unwrap_or(115200) as u32;
            let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
            driver.connect_serial(port, baud)?;
            Ok(serde_json::Value::String(format!("Connected to {}", port)))
        }
        "connect_telnet" => {
            let host = args["host"].as_str().unwrap_or("");
            let port = args["ws_port"].as_u64().map(|p| p as u16).unwrap_or(23);
            let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
            driver.connect_telnet(host, port)?;
            Ok(serde_json::Value::String(format!("Connected to {}:{}", host, port)))
        }
        "disconnect" => {
            let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
            driver.disconnect();
            Ok(serde_json::Value::Null)
        }
        _ => Err(format!("Command {} not implemented in standalone server", cmd)),
    }
}
