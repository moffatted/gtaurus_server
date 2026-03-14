use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GCodePoint {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub is_rapid: bool,
    pub line_number: u32,
    pub feedrate: f32,
    pub operation_id: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OperationInfo {
    pub id: u32,
    pub tool_number: Option<u32>,
    pub tool_name: Option<String>,
    pub tool_diameter: f32,
    pub tool_type: String,
    pub tool_angle_deg: Option<f32>,
    pub start_point_idx: usize,
    pub end_point_idx: usize,
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GCodeAnalysis {
    pub points: Vec<GCodePoint>,
    pub operations: Vec<OperationInfo>,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    pub total_dist_cut: f32,
    pub total_dist_rapid: f32,
    pub estimated_time_s: f32,
    pub min_z: f32,
    pub max_z: f32,
    pub workpiece_min_z: f32,
    pub workpiece_max_z: f32,
    pub min_feedrate: f32,
    pub max_feedrate: f32,
    pub wcs: String,
    pub unit: String,
    pub comments: Vec<String>,
    pub raw_lines: Vec<String>,
}
