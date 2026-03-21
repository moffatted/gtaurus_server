//! Auxiliary WebSocket handlers for camera settings and G-code analysis.

use serde_json::Value;

/// Read camera settings from the runtime device and optional config file.
pub fn get_camera_settings(args: &Value) -> Result<Value, String> {
    let config_path = args["configPath"].as_str().unwrap_or("");
    crate::camera::get_camera_settings(config_path)
}

/// Apply camera settings updates to hardware and config.
pub fn set_camera_settings(args: &Value) -> Result<Value, String> {
    let config_path = args["configPath"].as_str().unwrap_or("");
    let updates = args["updates"].clone();
    crate::camera::set_camera_settings(config_path, updates)
}

/// Parse and analyze a G-code file into structured geometry/operation metadata.
pub fn parse_gcode_file(args: &Value) -> Result<Value, String> {
    let path = args["path"]
        .as_str()
        .or(args["filePath"].as_str())
        .unwrap_or("")
        .to_string();
    let analysis = crate::gcode::parse_gcode_file_impl(path)?;
    serde_json::to_value(analysis).map_err(|e| e.to_string())
}
