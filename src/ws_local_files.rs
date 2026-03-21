//! WebSocket handlers for local file and directory operations.

use serde_json::Value;
use std::path::PathBuf;

/// Resolve a client-provided path into a host path, falling back to `gcode_files` under home.
pub fn get_resolved_path(path_arg: &str) -> PathBuf {
    let pb = PathBuf::from(path_arg);

    let is_windows_path = path_arg.contains(':') || path_arg.contains('\\');
    let is_host_windows = cfg!(windows);

    if path_arg.is_empty()
        || (is_windows_path && !is_host_windows)
        || (!is_windows_path && is_host_windows && !path_arg.starts_with('\\'))
    {
        let home = if is_host_windows {
            std::env::var("USERPROFILE").unwrap_or_else(|_| "C:/".to_string())
        } else {
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
        };
        let mut base = PathBuf::from(home);
        base.push("gcode_files");
        return base;
    }

    pb
}

/// Ensure a directory exists, creating it recursively if needed.
pub fn ensure_dir_exists(path_arg: &str) -> Result<Value, String> {
    let path = get_resolved_path(path_arg);
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(Value::Null)
}

/// List regular files in a directory with name, size, and modification time.
pub fn list_local_files(path_arg: &str) -> Result<Value, String> {
    let path = get_resolved_path(path_arg);
    println!("[GTaurus Server] list_local_files: resolved_path={:?}", path);

    let mut files = Vec::new();
    let entries = std::fs::read_dir(&path).map_err(|e| {
        println!("[GTaurus Server] Failed to read dir: {:?} - {}", path, e);
        e.to_string()
    })?;
    for entry in entries.flatten() {
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() {
                let modified = metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                files.push(serde_json::json!({
                    "name": entry.file_name().to_string_lossy().to_string(),
                    "size": metadata.len(),
                    "modified": modified
                }));
            }
        }
    }
    Ok(Value::Array(files))
}

/// Read a UTF-8 text file from the resolved directory.
pub fn read_local_file(path_arg: &str, filename: &str) -> Result<Value, String> {
    let mut full_path = get_resolved_path(path_arg);
    full_path.push(filename);
    let content = std::fs::read_to_string(full_path).map_err(|e| e.to_string())?;
    Ok(Value::String(content))
}

/// Write a UTF-8 text file to the resolved directory, creating parents when required.
pub fn save_local_file(path_arg: &str, filename: &str, content: &str) -> Result<Value, String> {
    let path = get_resolved_path(path_arg);

    println!(
        "[GTaurus Server] save_local_file: path={:?}, filename={:?}, size={}",
        path,
        filename,
        content.len()
    );
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let mut full_path = path;
    full_path.push(filename);
    std::fs::write(&full_path, content).map_err(|e| e.to_string())?;
    println!("[GTaurus Server] Successfully saved: {:?}", full_path);
    Ok(Value::Null)
}

/// Delete a file in the resolved directory.
pub fn delete_local_file(path_arg: &str, filename: &str) -> Result<Value, String> {
    let mut full_path = get_resolved_path(path_arg);
    full_path.push(filename);
    std::fs::remove_file(full_path).map_err(|e| e.to_string())?;
    Ok(Value::Null)
}

/// Copy a source file into a destination storage directory.
pub fn copy_to_storage(source_path_arg: &str, dest_dir_arg: &str) -> Result<Value, String> {
    let source = PathBuf::from(source_path_arg);
    if let Some(filename) = source.file_name() {
        let dest_path = get_resolved_path(dest_dir_arg);
        std::fs::create_dir_all(&dest_path).map_err(|e| e.to_string())?;
        let mut dest = dest_path;
        dest.push(filename);
        std::fs::copy(source, dest).map_err(|e| e.to_string())?;
        Ok(Value::Null)
    } else {
        Err("Invalid filename".to_string())
    }
}

/// Return the current user's home directory path.
pub fn get_home_dir() -> Result<Value, String> {
    let home = if cfg!(windows) {
        std::env::var("USERPROFILE").unwrap_or_else(|_| "C:/".to_string())
    } else {
        std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
    };
    Ok(Value::String(home))
}

/// Perform a lightweight heuristic check that a file contains likely G-code lines.
pub fn validate_gcode_file(path_arg: &str) -> Result<Value, String> {
    let path = get_resolved_path(path_arg);
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let is_gcode = content.lines().any(|line| {
        let l = line.trim();
        if l.is_empty() || l.starts_with(';') || l.starts_with('(') {
            return false;
        }
        l.starts_with('G')
            || l.starts_with('M')
            || l.starts_with('X')
            || l.starts_with('Y')
            || l.starts_with('Z')
            || l.starts_with('$')
            || l.starts_with('F')
            || l.starts_with('S')
            || l.starts_with('T')
    });
    Ok(Value::Bool(is_gcode))
}
