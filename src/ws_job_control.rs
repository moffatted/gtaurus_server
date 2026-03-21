use serde_json::Value;
use std::sync::atomic::Ordering;

pub fn get_job_status(state: &crate::AppState) -> Result<Value, String> {
    let status = state
        .job
        .status
        .lock()
        .map(|s| s.clone())
        .unwrap_or_else(|_| "unknown".to_string());
    let file_path = state
        .job
        .file_path
        .lock()
        .map(|s| s.clone())
        .unwrap_or_default();
    let current_line = state.job.current_line.load(Ordering::SeqCst);
    let total_lines = state.job.total_lines.load(Ordering::SeqCst);

    Ok(serde_json::json!({
        "status": status,
        "filePath": file_path,
        "currentLine": current_line,
        "totalLines": total_lines
    }))
}

pub fn pause_job(state: &crate::AppState) -> Result<Value, String> {
    state.job.pause_flag.store(true, Ordering::SeqCst);
    if let Ok(mut s) = state.job.status.lock() {
        *s = "paused".to_string();
    }

    if let Ok(mut driver) = state.driver.lock() {
        let _ = driver.send_realtime(0x21);
    }

    state.job.broadcast(
        &serde_json::json!({
            "type": "event",
            "event": "job://status",
            "payload": {
                "status": "paused",
                "currentLine": state.job.current_line.load(Ordering::SeqCst),
                "totalLines": state.job.total_lines.load(Ordering::SeqCst)
            }
        })
        .to_string(),
    );

    Ok(Value::String("Job paused".to_string()))
}

pub fn resume_job(state: &crate::AppState) -> Result<Value, String> {
    state.job.pause_flag.store(false, Ordering::SeqCst);
    if let Ok(mut s) = state.job.status.lock() {
        *s = "running".to_string();
    }

    if let Ok(mut driver) = state.driver.lock() {
        let _ = driver.send_realtime(0x7E);
    }

    state.job.broadcast(
        &serde_json::json!({
            "type": "event",
            "event": "job://status",
            "payload": {
                "status": "running",
                "currentLine": state.job.current_line.load(Ordering::SeqCst),
                "totalLines": state.job.total_lines.load(Ordering::SeqCst)
            }
        })
        .to_string(),
    );

    Ok(Value::String("Job resumed".to_string()))
}

pub fn cancel_job(state: &crate::AppState) -> Result<Value, String> {
    state.job.pause_flag.store(false, Ordering::SeqCst);
    state.job.cancel_flag.store(true, Ordering::SeqCst);

    let total = state.job.total_lines.load(Ordering::SeqCst);
    state.job.broadcast_cancelled(total);

    if let Ok(mut driver) = state.driver.lock() {
        let _ = driver.send_realtime(0x18);
    }

    Ok(Value::String("Job cancelled".to_string()))
}
