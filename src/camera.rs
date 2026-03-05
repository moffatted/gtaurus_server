use serde_json::Value;
use std::fs;
use std::process::Command;

pub fn get_camera_settings(config_path: &str) -> Result<Value, String> {
    let mut current_settings = serde_json::json!({});

    // Read v4l2-ctl
    if let Ok(output) = Command::new("v4l2-ctl")
        .args(["-d", "/dev/video0", "-L"])
        .output()
    {
        if output.status.success() {
            let out_str = String::from_utf8_lossy(&output.stdout);
            // Quick and dirty parse
            // e.g., brightness 0x00980900 (int)    : min=-64 max=64 step=1 default=0 value=-10
            for line in out_str.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() == 2 {
                    let left = parts[0].trim();
                    let right = parts[1].trim();
                    let name = left.split_whitespace().next().unwrap_or("");

                    let mut value_str = "0";
                    for prop in right.split_whitespace() {
                        if prop.starts_with("value=") {
                            value_str = prop.trim_start_matches("value=");
                        }
                    }
                    if !name.is_empty() {
                        if let Ok(v) = value_str.parse::<i32>() {
                            current_settings[name] = serde_json::json!(v);
                        } else {
                            current_settings[name] = serde_json::json!(value_str);
                        }
                    }
                }
            }
        }
    }

    // Read Crowsnest
    if !config_path.is_empty() {
        if let Ok(content) = fs::read_to_string(config_path) {
            for line in content.lines() {
                let l = line.trim();
                if l.starts_with('#') || l.is_empty() {
                    continue;
                }
                let parts: Vec<&str> = l.splitn(2, '=').collect();
                if parts.len() == 2 {
                    let k = parts[0].trim();
                    let v = parts[1].trim();
                    current_settings[k] = serde_json::json!(v);
                }
            }
        }
    }

    Ok(current_settings)
}

pub fn set_camera_settings(config_path: &str, updates: Value) -> Result<Value, String> {
    let obj = updates.as_object().ok_or("Updates must be an object")?;

    let hardware_props = vec![
        "brightness",
        "contrast",
        "saturation",
        "hue",
        "gamma",
        "gain",
        "exposure",
        "zoom",
    ];

    let mut file_changed = false;

    for (k, v) in obj {
        if hardware_props.contains(&k.as_str()) {
            // Apply via v4l2-ctl
            if let Some(val_num) = v.as_i64() {
                let ctrl = format!("{}={}", k, val_num);
                let _ = Command::new("v4l2-ctl")
                    .args(["-d", "/dev/video0", "--set-ctrl", &ctrl])
                    .output();
            }
        } else if !config_path.is_empty() {
            // Attempt to apply to crowsnest.conf
            if let Ok(content) = fs::read_to_string(config_path) {
                let mut new_lines = vec![];
                let mut replaced = false;
                for line in content.lines() {
                    let l = line.trim();
                    if l.starts_with(k.as_str()) && l.contains('=') {
                        if let Some(s) = v.as_str() {
                            new_lines.push(format!("{} = {}", k, s));
                            replaced = true;
                            continue;
                        }
                    }
                    new_lines.push(line.to_string());
                }

                if !replaced {
                    if let Some(s) = v.as_str() {
                        new_lines.push(format!("{} = {}", k, s));
                    }
                }

                if fs::write(config_path, new_lines.join("\n")).is_ok() {
                    file_changed = true;
                }
            }
        }
    }

    // Restart crowsnest user service to apply changes
    if file_changed {
        let _ = Command::new("systemctl")
            .args(["--user", "restart", "crowsnest"])
            .output();
    }

    Ok(serde_json::json!({"status": "success"}))
}
