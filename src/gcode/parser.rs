use super::models::{GCodeAnalysis, GCodePoint, OperationInfo};
use super::utils::{extract_tool_info, linearize_arc};
use std::fs::File;
use std::io::{BufRead, BufReader};

pub fn parse_gcode_file_impl(path: String) -> Result<GCodeAnalysis, String> {
    let file = File::open(&path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);

    let mut points = Vec::new();
    let mut operations = Vec::new();
    let mut raw_lines = Vec::new();
    let mut last_x = 0.0;
    let mut last_y = 0.0;
    let mut last_z = 0.0;
    let mut last_f = 0.0;
    let mut last_is_rapid = false;
    let mut is_relative = false;
    let mut is_inch = false;

    let mut bbox_min = [f32::MAX, f32::MAX, f32::MAX];
    let mut bbox_max = [f32::MIN, f32::MIN, f32::MIN];

    let mut total_dist_cut = 0.0;
    let mut total_dist_rapid = 0.0;

    let mut min_feedrate = f32::MAX;
    let mut max_feedrate = 0.0f32;

    let mut workpiece_min_z = 0.0f32;
    let mut workpiece_max_z = 0.0f32;
    let mut first_move = true;

    let mut wcs = "G54".to_string();
    let mut header_comments = Vec::new();
    let mut detected_unit = "Metric (mm)".to_string();

    let mut current_operation_id: u32 = 1;
    let mut current_operation_tool: Option<u32> = None;
    let mut pending_tool_number: Option<u32> = None;
    let mut pending_tool_name: Option<String> = None;
    let mut recent_tool_context: Option<String> = None;
    let mut current_operation_tool_name: Option<String> = None;
    let mut current_operation_tool_diameter: f32 = 5.0;
    let mut current_operation_tool_type: String = "unknown".to_string();
    let mut current_operation_tool_angle_deg: Option<f32> = None;
    let mut current_operation_start_point_idx: usize = 0;
    let mut current_operation_start_line: u32 = 1;
    let mut has_points_in_current_operation = false;
    let mut last_motion_line: u32 = 1;

    for (i, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        raw_lines.push(trimmed.to_string());

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('(') || trimmed.starts_with(';') {
            if i < 50 {
                header_comments.push(trimmed.to_string());
            }
            let comment_lower = trimmed.to_lowercase();
            if comment_lower.contains("tool")
                || comment_lower.contains("endmill")
                || comment_lower.contains("chamfer")
                || comment_lower.contains("v-bit")
                || comment_lower.contains("vbit")
                || comment_lower.contains("mm")
                || comment_lower.contains("deg")
                || comment_lower.contains('°')
            {
                let clean = trimmed
                    .trim_start_matches(|c| c == '(' || c == ';')
                    .trim_end_matches(')')
                    .trim()
                    .to_string();
                if !clean.is_empty() {
                    recent_tool_context = Some(clean);
                }
            }
            continue;
        }

        let line_content = if let Some(idx) = trimmed.find(|c| c == ';' || c == '(') {
            let comment = &trimmed[idx..];
            if i < 50 {
                header_comments.push(comment.to_string());
            }
            &trimmed[..idx]
        } else {
            trimmed
        }
        .trim();

        if line_content.is_empty() {
            continue;
        }

        let line_comment = if let Some(idx) = trimmed.find(|c| c == ';' || c == '(') {
            Some(&trimmed[idx..])
        } else {
            None
        };

        let mut current_x = last_x;
        let mut current_y = last_y;
        let mut current_z = last_z;
        let mut current_f = last_f;
        let mut move_type: Option<i32> = None;
        let mut changed = false;
        let mut arc_i = 0.0;
        let mut arc_j = 0.0;
        let mut line_has_m6 = false;
        let mut line_tool_number: Option<u32> = None;

        for part in line_content.split_whitespace() {
            if part.is_empty() {
                continue;
            }
            let cmd = part.chars().next().unwrap_or(' ').to_ascii_uppercase();
            let val = part[1..].parse::<f32>().unwrap_or(0.0);

            match cmd {
                'G' => {
                    let g_val = val as i32;
                    match g_val {
                        0 | 1 | 2 | 3 => {
                            move_type = Some(g_val);
                            changed = true;
                        }
                        20 => {
                            is_inch = true;
                            detected_unit = "Inches".to_string();
                        }
                        21 => {
                            is_inch = false;
                            detected_unit = "Metric (mm)".to_string();
                        }
                        54..=59 => wcs = format!("G{}", g_val),
                        90 => is_relative = false,
                        91 => is_relative = true,
                        _ => {}
                    }
                }
                'M' => {
                    let m_val = val as i32;
                    if m_val == 6 {
                        line_has_m6 = true;
                    }
                }
                'T' => {
                    if let Ok(t_val) = part[1..].parse::<u32>() {
                        line_tool_number = Some(t_val);
                    }
                }
                'X' => {
                    let mut v = val;
                    if is_inch {
                        v *= 25.4;
                    }
                    current_x = if is_relative { last_x + v } else { v };
                    changed = true;
                }
                'Y' => {
                    let mut v = val;
                    if is_inch {
                        v *= 25.4;
                    }
                    current_y = if is_relative { last_y + v } else { v };
                    changed = true;
                }
                'Z' => {
                    let mut v = val;
                    if is_inch {
                        v *= 25.4;
                    }
                    current_z = if is_relative { last_z + v } else { v };
                    changed = true;
                }
                'I' => {
                    let mut v = val;
                    if is_inch {
                        v *= 25.4;
                    }
                    arc_i = v;
                }
                'J' => {
                    let mut v = val;
                    if is_inch {
                        v *= 25.4;
                    }
                    arc_j = v;
                }
                'F' => {
                    let mut v = val;
                    if is_inch {
                        v *= 25.4;
                    }
                    current_f = v;
                    if v > 0.0 {
                        min_feedrate = min_feedrate.min(v);
                        max_feedrate = max_feedrate.max(v);
                    }
                }
                _ => {}
            }
        }

        if line_tool_number.is_some() {
            pending_tool_number = line_tool_number;
            pending_tool_name = None;
            if let Some(comment) = line_comment {
                let clean_comment = comment
                    .trim_start_matches(|c| c == '(' || c == ';')
                    .trim_end_matches(')')
                    .trim();
                if !clean_comment.is_empty() {
                    pending_tool_name = Some(clean_comment.to_string());
                }
            }
            if pending_tool_name.is_none() {
                pending_tool_name = recent_tool_context.clone();
            }
        }

        if line_has_m6 {
            if has_points_in_current_operation {
                operations.push(OperationInfo {
                    id: current_operation_id,
                    tool_number: current_operation_tool,
                    tool_name: current_operation_tool_name.clone(),
                    tool_diameter: current_operation_tool_diameter,
                    tool_type: current_operation_tool_type.clone(),
                    tool_angle_deg: current_operation_tool_angle_deg,
                    start_point_idx: current_operation_start_point_idx,
                    end_point_idx: points.len().saturating_sub(1),
                    start_line: current_operation_start_line,
                    end_line: last_motion_line,
                });
                current_operation_id += 1;
                current_operation_start_point_idx = points.len();
                current_operation_start_line = (i + 1) as u32;
                has_points_in_current_operation = false;
            } else {
                current_operation_start_line = (i + 1) as u32;
            }
            current_operation_tool = pending_tool_number;
            current_operation_tool_name = pending_tool_name.clone();
            let (diameter, tool_type, angle_deg) = extract_tool_info(pending_tool_name.as_deref());
            current_operation_tool_diameter = diameter;
            current_operation_tool_type = tool_type;
            current_operation_tool_angle_deg = angle_deg;
        }

        if changed {
            let m_type = move_type.unwrap_or(if last_is_rapid { 0 } else { 1 });
            if m_type == 2 || m_type == 3 {
                let arc_points = linearize_arc(
                    [last_x, last_y, last_z],
                    [current_x, current_y, current_z],
                    [arc_i, arc_j],
                    m_type == 2,
                    64,
                );
                for p in arc_points {
                    let dist = ((p[0] - last_x).powi(2)
                        + (p[1] - last_y).powi(2)
                        + (p[2] - last_z).powi(2))
                    .sqrt();
                    total_dist_cut += dist;
                    points.push(GCodePoint {
                        x: p[0],
                        y: p[1],
                        z: p[2],
                        is_rapid: false,
                        line_number: (i + 1) as u32,
                        feedrate: current_f,
                        operation_id: current_operation_id,
                    });
                    has_points_in_current_operation = true;
                    last_motion_line = (i + 1) as u32;
                    bbox_min[0] = bbox_min[0].min(p[0]);
                    bbox_min[1] = bbox_min[1].min(p[1]);
                    bbox_min[2] = bbox_min[2].min(p[2]);
                    bbox_max[0] = bbox_max[0].max(p[0]);
                    bbox_max[1] = bbox_max[1].max(p[1]);
                    bbox_max[2] = bbox_max[2].max(p[2]);
                    if first_move {
                        workpiece_max_z = p[2];
                        workpiece_min_z = p[2];
                        first_move = false;
                    } else {
                        workpiece_max_z = workpiece_max_z.max(p[2]);
                        workpiece_min_z = workpiece_min_z.min(p[2]);
                    }
                    last_x = p[0];
                    last_y = p[1];
                    last_z = p[2];
                }
                last_is_rapid = false;
            } else {
                let is_rapid = m_type == 0;
                let dist = ((current_x - last_x).powi(2)
                    + (current_y - last_y).powi(2)
                    + (current_z - last_z).powi(2))
                .sqrt();
                if is_rapid {
                    total_dist_rapid += dist;
                } else {
                    total_dist_cut += dist;
                }
                points.push(GCodePoint {
                    x: current_x,
                    y: current_y,
                    z: current_z,
                    is_rapid,
                    line_number: (i + 1) as u32,
                    feedrate: current_f,
                    operation_id: current_operation_id,
                });
                has_points_in_current_operation = true;
                last_motion_line = (i + 1) as u32;
                if !is_rapid {
                    if first_move {
                        workpiece_max_z = current_z;
                        workpiece_min_z = current_z;
                        first_move = false;
                    } else {
                        workpiece_max_z = workpiece_max_z.max(current_z);
                        workpiece_min_z = workpiece_min_z.min(current_z);
                    }
                }
                bbox_min[0] = bbox_min[0].min(current_x);
                bbox_min[1] = bbox_min[1].min(current_y);
                bbox_min[2] = bbox_min[2].min(current_z);
                bbox_max[0] = bbox_max[0].max(current_x);
                bbox_max[1] = bbox_max[1].max(current_y);
                bbox_max[2] = bbox_max[2].max(current_z);
                last_x = current_x;
                last_y = current_y;
                last_z = current_z;
                last_is_rapid = is_rapid;
            }
            last_f = current_f;
        }
    }

    if points.is_empty() {
        bbox_min = [0.0, 0.0, 0.0];
        bbox_max = [0.0, 0.0, 0.0];
    } else if has_points_in_current_operation {
        operations.push(OperationInfo {
            id: current_operation_id,
            tool_number: current_operation_tool,
            tool_name: current_operation_tool_name.clone(),
            tool_diameter: current_operation_tool_diameter,
            tool_type: current_operation_tool_type.clone(),
            tool_angle_deg: current_operation_tool_angle_deg,
            start_point_idx: current_operation_start_point_idx,
            end_point_idx: points.len().saturating_sub(1),
            start_line: current_operation_start_line,
            end_line: last_motion_line,
        });
    }

    let f_cut_avg = if max_feedrate > 0.0 {
        max_feedrate
    } else {
        1000.0
    };
    let f_rapid_avg = 3000.0;
    let estimated_time_s =
        (total_dist_cut / (f_cut_avg / 60.0)) + (total_dist_rapid / (f_rapid_avg / 60.0));

    Ok(GCodeAnalysis {
        points,
        operations,
        bbox_min,
        bbox_max,
        total_dist_cut,
        total_dist_rapid,
        estimated_time_s,
        min_z: bbox_min[2],
        max_z: bbox_max[2],
        workpiece_min_z,
        workpiece_max_z,
        min_feedrate: if min_feedrate == f32::MAX {
            0.0
        } else {
            min_feedrate
        },
        max_feedrate,
        wcs,
        unit: detected_unit,
        comments: header_comments,
        raw_lines,
    })
}
