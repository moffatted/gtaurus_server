pub fn extract_tool_info(tool_name: Option<&str>) -> (f32, String, Option<f32>) {
    if let Some(name) = tool_name {
        let lower = name.to_lowercase();

        let diameter = if let Some(mm_pos) = lower.find("mm") {
            if mm_pos >= 2 {
                let num_str = &lower[..mm_pos];
                if let Some(first_digit) = num_str.rfind(|c: char| !c.is_numeric() && c != '.') {
                    let num_part = &num_str[first_digit + 1..];
                    num_part.parse::<f32>().unwrap_or(5.0)
                } else {
                    num_str.parse::<f32>().unwrap_or(5.0)
                }
            } else {
                5.0
            }
        } else {
            5.0
        };

        let tool_type = if lower.contains("flat") || lower.contains("endmill") {
            if lower.contains("ball") {
                "ballnose".to_string()
            } else {
                "flatendmill".to_string()
            }
        } else if lower.contains("chamfer") {
            "chamfer".to_string()
        } else if lower.contains("v-bit") || lower.contains("vbit") {
            "vbit".to_string()
        } else if lower.contains("bull") {
            "bullnose".to_string()
        } else if lower.contains("surfac") || lower.contains("fly") || lower.contains("facing") {
            "surfacing".to_string()
        } else {
            "unknown".to_string()
        };

        let mut angle_deg: Option<f32> = None;
        for token in lower.split_whitespace() {
            let cleaned = token.replace('°', "deg");
            if let Some(idx) = cleaned.find("deg") {
                let number_part = &cleaned[..idx];
                if !number_part.is_empty() {
                    if let Ok(v) = number_part.parse::<f32>() {
                        angle_deg = Some(v);
                        break;
                    }
                }
            }
        }

        (diameter, tool_type, angle_deg)
    } else {
        (5.0, "unknown".to_string(), None)
    }
}

pub fn linearize_arc(
    start: [f32; 3],
    end: [f32; 3],
    center_offset: [f32; 2],
    is_clockwise: bool,
    segments: usize,
) -> Vec<[f32; 3]> {
    let mut points = Vec::new();
    let cx = start[0] + center_offset[0];
    let cy = start[1] + center_offset[1];

    let r = (center_offset[0].powi(2) + center_offset[1].powi(2)).sqrt();
    let start_angle = (start[1] - cy).atan2(start[0] - cx);
    let mut end_angle = (end[1] - cy).atan2(end[0] - cx);

    let is_full_circle = (start[0] - end[0]).abs() < 0.001
        && (start[1] - end[1]).abs() < 0.001
        && (center_offset[0].abs() > 0.001 || center_offset[1].abs() > 0.001);

    if is_full_circle {
        if is_clockwise {
            end_angle = start_angle - 2.0 * std::f32::consts::PI;
        } else {
            end_angle = start_angle + 2.0 * std::f32::consts::PI;
        }
    } else if is_clockwise {
        if end_angle >= start_angle {
            end_angle -= 2.0 * std::f32::consts::PI;
        }
    } else if end_angle <= start_angle {
        end_angle += 2.0 * std::f32::consts::PI;
    }

    let angle_diff = end_angle - start_angle;
    let dz = end[2] - start[2];

    for i in 1..=segments {
        let ratio = i as f32 / segments as f32;
        let angle = start_angle + angle_diff * ratio;
        let px = cx + angle.cos() * r;
        let py = cy + angle.sin() * r;
        let pz = start[2] + dz * ratio;
        points.push([px, py, pz]);
    }
    points
}
