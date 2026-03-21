//! WebSocket handlers for machine connection and direct command operations.

use serde_json::Value;

/// List all serial ports visible to the current host.
///
/// # Returns
/// A JSON array of port names.
pub fn list_serial_ports() -> Result<Value, String> {
    let ports = match serialport::available_ports() {
        Ok(ports) => ports.into_iter().map(|p| p.port_name).collect::<Vec<String>>(),
        Err(_) => vec![],
    };
    Ok(serde_json::to_value(ports).unwrap())
}

/// Get the current machine connection status.
///
/// # Example
///
/// Typical response payload:
/// ```json
/// { "status": "Disconnected" }
/// ```
///
/// # Errors
///
/// Returns an error if the shared driver lock cannot be acquired.
pub fn get_connection_status(state: &crate::AppState) -> Result<Value, String> {
    let driver = state.driver.lock().map_err(|_| "Lock failed")?;
    Ok(Value::String(driver.get_status()))
}

/// Queue a G-code line for asynchronous transmission.
///
/// # Errors
/// Returns an error if the driver lock fails or no active connection exists.
///
/// # Example
///
/// Client request payload:
/// ```json
/// { "code": "G0 X10 Y10" }
/// ```
pub fn send_gcode(state: &crate::AppState, code: &str) -> Result<Value, String> {
    let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
    driver.send_command(code.to_string())?;
    Ok(Value::Null)
}

/// Send a realtime control byte immediately (bypasses command queue).
///
/// # Example
///
/// Soft reset (0x18):
/// ```json
/// { "byte": 24 }
/// ```
///
/// # Errors
///
/// Returns an error if the driver lock fails or no active connection exists.
pub fn send_realtime(state: &crate::AppState, byte_num: u8) -> Result<Value, String> {
    let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
    driver.send_realtime(byte_num)?;
    Ok(Value::Null)
}

/// Connect to a machine through a serial port.
///
/// # Example
///
/// Client request payload:
/// ```json
/// { "port": "COM3", "baud": 115200 }
/// ```
///
/// # Errors
///
/// Returns an error if lock acquisition fails or the serial connection attempt fails.
pub fn connect_serial(state: &crate::AppState, port: &str, baud: u32) -> Result<Value, String> {
    let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
    driver.set_auto_connect_suspended(false);
    driver.connect_serial(port, baud)?;
    Ok(Value::String(format!("Connected to {}", port)))
}

/// Connect to a machine through Telnet (TCP).
///
/// # Example
///
/// Client request payload:
/// ```json
/// { "host": "192.168.1.120", "port": 23 }
/// ```
///
/// # Errors
///
/// Returns an error if lock acquisition fails or the TCP connection attempt fails.
pub fn connect_telnet(state: &crate::AppState, host: &str, port: u16) -> Result<Value, String> {
    let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
    driver.set_auto_connect_suspended(false);
    driver.connect_telnet(host, port)?;
    Ok(Value::String(format!("Connected to {}:{}", host, port)))
}

/// Disconnect from the current machine and suspend auto-connect.
///
/// # Errors
///
/// Returns an error if the shared driver lock cannot be acquired.
pub fn disconnect(state: &crate::AppState) -> Result<Value, String> {
    let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
    driver.disconnect();
    driver.set_auto_connect_suspended(true);
    Ok(Value::Null)
}
