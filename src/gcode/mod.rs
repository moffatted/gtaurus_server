//! G-code parsing and analysis module.
//!
//! Exposes geometry points, operation grouping, and utility helpers.

pub mod models;
pub mod parser;
pub mod utils;

pub use parser::parse_gcode_file_impl;
