use serde_json::Value;

pub fn generate_surfacing_toolpath(args: &Value) -> Result<Value, String> {
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
    .map(Value::String)
}
