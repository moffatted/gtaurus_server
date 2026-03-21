use serde_json::Value;

pub fn list_serial_ports() -> Result<Value, String> {
    let ports = match serialport::available_ports() {
        Ok(ports) => ports.into_iter().map(|p| p.port_name).collect::<Vec<String>>(),
        Err(_) => vec![],
    };
    Ok(serde_json::to_value(ports).unwrap())
}

pub fn get_connection_status(state: &crate::AppState) -> Result<Value, String> {
    let driver = state.driver.lock().map_err(|_| "Lock failed")?;
    Ok(Value::String(driver.get_status()))
}

pub fn send_gcode(state: &crate::AppState, code: &str) -> Result<Value, String> {
    let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
    driver.send_command(code.to_string())?;
    Ok(Value::Null)
}

pub fn send_realtime(state: &crate::AppState, byte_num: u8) -> Result<Value, String> {
    let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
    driver.send_realtime(byte_num)?;
    Ok(Value::Null)
}

pub fn connect_serial(state: &crate::AppState, port: &str, baud: u32) -> Result<Value, String> {
    let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
    driver.set_auto_connect_suspended(false);
    driver.connect_serial(port, baud)?;
    Ok(Value::String(format!("Connected to {}", port)))
}

pub fn connect_telnet(state: &crate::AppState, host: &str, port: u16) -> Result<Value, String> {
    let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
    driver.set_auto_connect_suspended(false);
    driver.connect_telnet(host, port)?;
    Ok(Value::String(format!("Connected to {}:{}", host, port)))
}

pub fn disconnect(state: &crate::AppState) -> Result<Value, String> {
    let mut driver = state.driver.lock().map_err(|_| "Lock failed")?;
    driver.disconnect();
    driver.set_auto_connect_suspended(true);
    Ok(Value::Null)
}
