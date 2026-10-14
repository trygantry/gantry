use crate::output::Output;
use gantry_core::{EvaluationOptions, compile_launch_index_ctx};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub fn handle(output: &Output, vault_path: &Path, opts: &EvaluationOptions) -> Result<i32, String> {
    if !vault_path.exists() {
        return Err("Error: No .gantry/ vault found in the current directory.".to_string());
    }

    let (tx, rx) = mpsc::channel::<Result<Event, notify::Error>>();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())
        .map_err(|e| format!("Failed to create watcher: {}", e))?;

    watcher
        .watch(vault_path, RecursiveMode::Recursive)
        .map_err(|e| format!("Failed to watch vault: {}", e))?;

    output.print_stdout(&format!(
        "Gantry watch started. Monitoring {} for changes.",
        vault_path.display()
    ));
    output.print_stdout("Press Ctrl+C to stop.");
    output.print_stdout("=====================================");

    let run_validate = || {
        let ts = chrono::Local::now().format("%H:%M:%S");
        match compile_launch_index_ctx(opts) {
            Ok(payload) => {
                let icon = match payload.readiness.as_str() {
                    "Go" => output.style("GO", Some("green"), true),
                    "At Risk" => output.style("AT RISK", Some("yellow"), true),
                    _ => output.style("NO-GO", Some("red"), true),
                };
                output.print_stdout(&format!(
                    "[{}] {} — score {}/100, {} blocker(s), {}/{} QA",
                    ts,
                    icon,
                    payload.readiness_score,
                    payload.open_blockers,
                    payload.qa_completed,
                    payload.qa_total
                ));
                if !payload.readiness_reasons.is_empty() {
                    for r in payload.readiness_reasons.iter().take(3) {
                        output.print_stdout(&format!("         ↳ {}", r));
                    }
                }
            }
            Err(e) => output.print_stderr(&format!("[{}] Error: {}", ts, e)),
        }
    };

    run_validate();

    let debounce = Duration::from_millis(500);
    let mut last_event: Option<Instant> = None;

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(_) => {
                last_event = Some(Instant::now());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(t) = last_event {
                    if t.elapsed() >= debounce {
                        last_event = None;
                        run_validate();
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(0)
}
