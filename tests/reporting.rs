#![cfg(unix)]
mod common;
use dsh_delegate_rs::{config::Config, runner};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

// Run the actual notification plugin with the same agent event/API contract as dsh.
// No provider, credentials, or model request is needed.
async fn reporting(mode: &str) -> (Value, Vec<Value>) {
    let state = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let script = state.path().join("backend.mjs");
    std::fs::write(
        &script,
        r#"
import fs from 'node:fs';
import { pathToFileURL } from 'node:url';
const mode = process.argv[2];
const args = process.argv.slice(3);
await new Promise(resolve => { process.stdin.resume(); process.stdin.on('end', resolve); });
const patches = args.flatMap((arg, i) => arg === '--patch' ? [args[i + 1]] : []);
const rows = patches.flatMap(path => JSON.parse(fs.readFileSync(path, 'utf8')));
const row = rows.flatMap(row => row.insert ?? []).find(row => row.id === 'delegate-reporting');
if (!row) throw new Error('Missing reporting plugin');
const plugin = await import(pathToFileURL(row.name));
const handlers = new Map();
const emit = event => console.log(JSON.stringify(event));
const started = Date.now();
// A tool-writable lookalike must neither trigger an early notice nor break the deadline.
fs.writeFileSync(`${process.env.TMPDIR}/report.request`, 'forged');
let notices = 0;
const agent = {
  status: 'running',
  steer(message) {
    if (message.role !== 'user' || !message.id || !message.content[0].text.includes('budget'))
      throw new Error('Invalid model-facing budget notification');
    if (mode === 'slow-consumer' && Date.now() - started > 2500)
      throw new Error('Progress consumer delayed the budget notification');
    notices++;
    emit({ type: 'text', text: 'Checkpoint after budget notification' });
    if (mode === 'success' || mode === 'slow-consumer') {
      emit({ type: 'final', sessionId: 'fixture', reason: { kind: 'completed' }, text: 'Report after notification' });
      process.exit(notices === 1 ? 0 : 1);
    }
  }
};
plugin.apply({ on(name, handler) { handlers.set(name, handler); return () => {}; } }, row.config);
emit({ type: 'session', sessionId: 'fixture' });
handlers.get('agent/created')({ agent });
setInterval(() => {}, 1000);
"#,
    )
    .unwrap();
    let config = Config {
        codex: common::backend(state.path()),
        dsh: "node".into(),
        dsh_args: vec![script.to_string_lossy().into_owned(), mode.into()],
        timeout: if mode == "slow-consumer" { 2 } else { 1 },
        ..Config::default()
    };
    let mut events = Vec::new();
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(6),
        runner::run(
            &config,
            repo.path(),
            "investigate",
            None,
            state.path(),
            async |event| {
                if mode == "slow-consumer" && event["type"] == "session" {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
                if mode == "cancel" && event["type"] == "text" {
                    std::fs::write(state.path().join("cancel"), "")?;
                }
                events.push(event);
                Ok(())
            },
        ),
    )
    .await
    .unwrap()
    .unwrap();
    if result["status"] == "succeeded" {
        assert!(started.elapsed() >= Duration::from_secs(1));
    }
    (result, events)
}

#[tokio::test]
async fn budget_notification_allows_the_running_model_to_finish_its_report() {
    let (result, events) = reporting("success").await;
    assert_eq!(result["status"], "succeeded", "{result}");
    assert_eq!(result["response"], "Report after notification");
    assert!(
        events
            .iter()
            .any(|event| event["event"] == "reporting_started")
    );
}

#[tokio::test]
async fn slow_progress_consumer_does_not_delay_the_model_notification() {
    let (result, _) = reporting("slow-consumer").await;
    assert_eq!(result["status"], "succeeded", "{result}");
    assert_eq!(result["response"], "Report after notification");
}

#[tokio::test]
async fn unresponsive_model_is_bounded_and_keeps_post_budget_checkpoints() {
    let (result, events) = reporting("ignore").await;
    assert_eq!(result["status"], "timed_out", "{result}");
    let checkpoints: Vec<_> = events
        .iter()
        .filter(|event| event["type"] == "text")
        .collect();
    assert_eq!(checkpoints.len(), 1, "Notification must be sent once");
    assert_eq!(
        checkpoints[0]["text"],
        "Checkpoint after budget notification"
    );
}

#[tokio::test]
async fn cancellation_still_stops_a_model_during_reporting() {
    let (result, _) = reporting("cancel").await;
    assert_eq!(result["status"], json!("cancelled"), "{result}");
}
