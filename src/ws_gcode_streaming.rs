/*
 * @file ws_gcode_streaming.rs
 * @purpose Handles streaming G-code from local files with support for pause, resume, and cancel operations.
 */

use serde_json::Value;
use std::sync::atomic::Ordering;

pub fn stream_local_gcode(
    state: &crate::AppState,
    args: &Value,
) -> Result<Value, String> {
    let path = args["path"].as_str().unwrap_or("").to_string();
    let feed_override: Option<f64> = args["feedRateOverride"].as_f64();
    // 1-indexed file line to start from. 0 or 1 means start from the beginning.
    let start_line: usize = args["startLine"].as_u64().unwrap_or(0) as usize;

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;

    // Count the active G-code lines so the frontend can display progress.
    let total_active: usize = content
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with(';') && !t.starts_with('(')
        })
        .count();

    let driver_clone = state.driver.clone();
    let job = state.job.clone();

    // Signal any currently running job to stop before starting a new one.
    // Incrementing the generation causes the old streaming thread to detect the
    // mismatch and exit without needing a sleep-based race.
    let my_gen = job.generation.fetch_add(1, Ordering::SeqCst) + 1;

    // Reset job state for the new run.
    job.cancel_flag.store(false, Ordering::SeqCst);
    job.pause_flag.store(false, Ordering::SeqCst);
    job.total_lines.store(total_active, Ordering::SeqCst);
    job.current_line.store(0, Ordering::SeqCst);
    if let Ok(mut s) = job.status.lock() {
        *s = "running".to_string();
    }
    if let Ok(mut fp) = job.file_path.lock() {
        *fp = path.clone();
    }

    // Broadcast the initial "running" status to all connected clients.
    job.broadcast(
        &serde_json::json!({
            "type": "event",
            "event": "job://status",
            "payload": {
                "status": "running",
                "filePath": path,
                "currentLine": 0,
                "totalLines": total_active,
                "startLine": start_line
            }
        })
        .to_string(),
    );

    std::thread::spawn(move || {
        crate::log_msg(&format!(
            "[GTaurus Server] Starting G-code stream job (startLine={}, totalLines={})...",
            start_line, total_active
        ));

        let mut has_sent_initial_f = false;
        let mut active_line_count: usize = 0;

        for (file_line_idx, line) in content.lines().enumerate() {
            // Exit if a new job has superseded this one or if explicitly cancelled.
            if job.generation.load(Ordering::SeqCst) != my_gen
                || job.cancel_flag.load(Ordering::SeqCst)
            {
                crate::log_msg(&format!(
                    "[GTaurus Server] Job cancelled at file line {}.",
                    file_line_idx + 1
                ));
                job.broadcast_cancelled(total_active);
                return;
            }

            // Skip lines before the requested start position (1-indexed; 0 means from
            // the beginning, equivalent to 1).
            let effective_start = if start_line == 0 { 1 } else { start_line };
            if (file_line_idx + 1) < effective_start {
                continue;
            }

            let l = line.trim();
            if l.is_empty() || l.starts_with(';') || l.starts_with('(') {
                continue;
            }

            // Wait while the job is paused, checking for cancellation each tick.
            while job.pause_flag.load(Ordering::SeqCst) {
                if job.generation.load(Ordering::SeqCst) != my_gen
                    || job.cancel_flag.load(Ordering::SeqCst)
                {
                    job.broadcast_cancelled(total_active);
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            // Apply optional feed-rate override.
            let mut final_line = l.to_string();
            if let Some(target_f) = feed_override {
                if l.contains('F') || l.contains('f') {
                    let parts: Vec<&str> = l.split_whitespace().collect();
                    let mut new_parts = Vec::new();
                    for p in parts {
                        if p.starts_with('F') || p.starts_with('f') {
                            new_parts.push(format!("F{:.1}", target_f));
                        } else {
                            new_parts.push(p.to_string());
                        }
                    }
                    final_line = new_parts.join(" ");
                    has_sent_initial_f = true;
                } else if (l.contains("G1") || l.contains("G2") || l.contains("G3"))
                    && !has_sent_initial_f
                {
                    final_line = format!("{} F{:.1}", l, target_f);
                    has_sent_initial_f = true;
                }
            }

            if let Ok(mut driver) = driver_clone.lock() {
                let _ = driver.send_command(final_line);
            }

            active_line_count += 1;
            job.current_line.store(active_line_count, Ordering::SeqCst);

            // Throttle progress broadcasts to every 10 lines to avoid flooding clients
            // on large files, while still providing responsive feedback.
            if active_line_count % 10 == 0 {
                job.broadcast(
                    &serde_json::json!({
                        "type": "event",
                        "event": "job://status",
                        "payload": {
                            "status": "running",
                            "currentLine": active_line_count,
                            "totalLines": total_active
                        }
                    })
                    .to_string(),
                );
            }
        }

        if let Ok(mut s) = job.status.lock() {
            *s = "completed".to_string();
        }
        job.broadcast(
            &serde_json::json!({
                "type": "event",
                "event": "job://status",
                "payload": {
                    "status": "completed",
                    "currentLine": active_line_count,
                    "totalLines": total_active
                }
            })
            .to_string(),
        );
        crate::log_msg("[GTaurus Server] Finished streaming G-code job.");
    });
    Ok(serde_json::Value::String("Streaming started".to_string()))
}
