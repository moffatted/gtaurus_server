use serde_json::Value;

pub fn save_checkpoint(args: &Value) -> Result<Value, String> {
    let checkpoint_data = args.get("checkpoint").cloned();
    let save_path = args["savePath"].as_str().unwrap_or("").to_string();

    if let Some(checkpoint_json) = checkpoint_data {
        match serde_json::from_value::<crate::checkpoint::JobCheckpoint>(checkpoint_json) {
            Ok(checkpoint) => match crate::checkpoint::save_checkpoint(&checkpoint, &save_path) {
                Ok(_) => {
                    println!(
                        "[Checkpoint] Successfully saved checkpoint for {}",
                        checkpoint.file_name
                    );
                    Ok(Value::String(format!(
                        "Checkpoint saved: {}",
                        checkpoint.checkpoint_id
                    )))
                }
                Err(e) => {
                    eprintln!("[Checkpoint] Failed to save: {}", e);
                    Err(e)
                }
            },
            Err(e) => {
                let err_msg = format!("Invalid checkpoint data: {}", e);
                eprintln!("{}", err_msg);
                Err(err_msg)
            }
        }
    } else {
        Err("No checkpoint data provided".to_string())
    }
}

pub fn load_checkpoint(args: &Value) -> Result<Value, String> {
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
            println!(
                "[Checkpoint] Loaded checkpoint: {} (line {}/{})",
                checkpoint.file_name, checkpoint.current_line, checkpoint.total_lines
            );
            serde_json::to_value(checkpoint).map_err(|e| e.to_string())
        }
        Err(e) => {
            eprintln!("[Checkpoint] Failed to load: {}", e);
            Err(e)
        }
    }
}

pub fn get_resume_checkpoint_path(args: &Value) -> Result<Value, String> {
    let gcode_path = args["path"]
        .as_str()
        .or(args["filePath"].as_str())
        .unwrap_or("")
        .to_string();

    if gcode_path.is_empty() {
        return Err("No G-code path provided".to_string());
    }

    let checkpoint_path = crate::checkpoint::get_resume_checkpoint_path(&gcode_path);
    Ok(Value::String(checkpoint_path.to_string_lossy().to_string()))
}

pub fn compute_file_hash(args: &Value) -> Result<Value, String> {
    let file_path = args["path"]
        .as_str()
        .or(args["filePath"].as_str())
        .unwrap_or("")
        .to_string();

    if file_path.is_empty() {
        return Err("No file path provided".to_string());
    }

    match crate::checkpoint::compute_file_hash(&file_path) {
        Ok(hash) => Ok(Value::String(hash)),
        Err(e) => Err(e),
    }
}
