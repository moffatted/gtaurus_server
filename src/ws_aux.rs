use serde_json::Value;

pub fn get_camera_settings(args: &Value) -> Result<Value, String> {
    let config_path = args["configPath"].as_str().unwrap_or("");
    crate::camera::get_camera_settings(config_path)
}

pub fn set_camera_settings(args: &Value) -> Result<Value, String> {
    let config_path = args["configPath"].as_str().unwrap_or("");
    let updates = args["updates"].clone();
    crate::camera::set_camera_settings(config_path, updates)
}

pub fn parse_gcode_file(args: &Value) -> Result<Value, String> {
    let path = args["path"]
        .as_str()
        .or(args["filePath"].as_str())
        .unwrap_or("")
        .to_string();
    let analysis = crate::gcode::parse_gcode_file_impl(path)?;
    serde_json::to_value(analysis).map_err(|e| e.to_string())
}
