#![cfg(unix)]
mod common;
use common::CommandExt;
use serde_json::{Value, json};
use std::{path::Path, process::Command};

fn command(state: &Path, script: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dsh-delegate-rs"));
    cmd.arg("--state-dir")
        .arg(state)
        .arg("--codex")
        .arg(common::backend(state))
        .args(["--dsh", "/bin/sh", "--dsh-arg"])
        .arg(script);
    cmd
}
fn result(cmd: &mut Command) -> Value {
    let output = cmd.bounded_output();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn wait(state: &Path, script: &Path, id: &Value) -> Value {
    result(command(state, script).args(["wait", id.as_str().unwrap(), "--seconds", "5"]))
}
fn fixture(state: &Path, body: &str) -> std::path::PathBuf {
    let script = state.join("fixture.sh");
    std::fs::write(&script, format!("cat >/dev/null\nprintf '%s\\n' '{{\"type\":\"session\",\"sessionId\":\"fixture\"}}'\n{body}\n")).unwrap();
    script
}
fn checkpoint(state: &Path, script: &Path, id: &Value) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let status = result(command(state, script).args(["status", id.as_str().unwrap()]));
        if status["partial_report"].is_string() {
            return status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Checkpoint was not persisted"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn task_budget_retains_redacted_text_on_timeout_and_followup_can_override_it() {
    let state = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let script = fixture(
        state.path(),
        "printf '%s\\n' '{\"type\":\"thinking\",\"text\":\"PRIVATE_REASONING\"}' '{\"type\":\"text\",\"text\":\"Found src/a.rs:12 fixture-secret-value\"}'\nsleep 20",
    );
    let env = state.path().join("api.env");
    std::fs::write(&env, "FIXTURE_KEY=fixture-secret-value\n").unwrap();
    let submitted = result(
        command(state.path(), &script)
            .arg("--dsh-env-file")
            .arg(&env)
            .args(["submit", "--cwd"])
            .arg(repo.path())
            .args(["--task", "inspect", "--task-timeout", "1"]),
    );
    assert_eq!(submitted["timeout"], 1);
    let stopped = wait(state.path(), &script, &submitted["id"]);
    assert_eq!(stopped["status"], "timed_out");
    assert_eq!(stopped["report_partial"], true);
    assert!(stopped["report"].as_str().unwrap().contains("src/a.rs:12"));
    assert!(!stopped.to_string().contains("fixture-secret-value"));
    assert!(!stopped.to_string().contains("PRIVATE_REASONING"));
    std::fs::write(&script, "cat >/dev/null\nprintf '%s\\n' '{\"type\":\"session\",\"sessionId\":\"fixture\"}' '{\"type\":\"final\",\"sessionId\":\"fixture\",\"reason\":{\"kind\":\"completed\"},\"text\":\"complete\"}'\n").unwrap();
    let followup = result(command(state.path(), &script).args([
        "followup",
        submitted["id"].as_str().unwrap(),
        "finish",
        "--task-timeout",
        "3",
    ]));
    assert_eq!(followup["timeout"], 3);
    assert_eq!(followup["root_id"], submitted["root_id"]);
    let finished = wait(state.path(), &script, &followup["id"]);
    assert_eq!(finished["status"], "succeeded");
    assert_eq!(finished["report"], "complete");
    assert_eq!(finished["report_partial"], false);
    assert!(finished["usage"].is_null());
}

#[test]
fn reporting_preserves_late_checkpoints_and_still_enforces_required_checks() {
    for complete in [false, true] {
        let state = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let report = if complete {
            "Report <acceptance>{\"1\":{\"status\":\"done\",\"evidence\":\"src/a.rs:12\"}}</acceptance>"
        } else {
            "Report without required evidence"
        };
        let final_event = json!({"type":"final","sessionId":"fixture","reason":{"kind":"completed"},"text":report});
        let script = fixture(
            state.path(),
            &format!(
                "while [ \"$(cat \"$TMPDIR/../report.request\")\" != report ]; do sleep 0.02; done\nprintf '%s\\n' '{{\"type\":\"text\",\"text\":\"Late checkpoint src/a.rs:12\"}}' '{final_event}'"
            ),
        );
        let job = result(
            command(state.path(), &script)
                .args(["submit", "--cwd"])
                .arg(repo.path())
                .args([
                    "--task",
                    "inspect",
                    "--task-timeout",
                    "1",
                    "--check",
                    "Locate evidence",
                ]),
        );
        let stopped = wait(state.path(), &script, &job["id"]);
        assert_eq!(
            stopped["status"],
            if complete { "succeeded" } else { "incomplete" }
        );
        assert!(stopped["reporting_started"].is_number());
        assert!(
            stopped["partial_report"]
                .as_str()
                .unwrap()
                .contains("Late checkpoint")
        );
        assert_eq!(stopped["report"], report);
        assert_eq!(stopped["report_partial"], false);
    }
}

#[test]
fn empty_final_report_does_not_discard_a_checkpoint() {
    let state = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let script = fixture(
        state.path(),
        "printf '%s\\n' '{\"type\":\"text\",\"text\":\"Found src/a.rs:12\"}' '{\"type\":\"final\",\"sessionId\":\"fixture\",\"reason\":{\"kind\":\"completed\"},\"text\":\" \"}'",
    );
    let job = result(
        command(state.path(), &script)
            .args(["submit", "--cwd"])
            .arg(repo.path())
            .args(["--task", "inspect"]),
    );
    let stopped = wait(state.path(), &script, &job["id"]);
    assert_eq!(stopped["status"], "failed");
    assert_eq!(stopped["report_partial"], true);
    assert!(stopped["report"].as_str().unwrap().contains("src/a.rs:12"));
}

#[test]
fn cancellation_preserves_checkpoint_and_unicode_retention_is_bounded() {
    let state = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let checkpoint_event = json!({"type":"text","text":"🦀".repeat(9000)});
    let script = fixture(
        state.path(),
        &format!("printf '%s\\n' '{checkpoint_event}'\nsleep 20"),
    );
    let job = result(
        command(state.path(), &script)
            .args(["submit", "--cwd"])
            .arg(repo.path())
            .args(["--task", "inspect"]),
    );
    checkpoint(state.path(), &script, &job["id"]);
    result(command(state.path(), &script).args(["cancel", job["id"].as_str().unwrap()]));
    let stopped = wait(state.path(), &script, &job["id"]);
    assert_eq!(stopped["status"], "cancelled");
    assert_eq!(
        stopped["partial_report"].as_str().unwrap().chars().count(),
        8000
    );
    assert_eq!(stopped["report_partial"], true);
}

#[test]
fn stopped_worker_cancellation_exposes_its_durable_checkpoint_as_a_report() {
    let state = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let script = fixture(
        state.path(),
        "printf '%s\\n' '{\"type\":\"text\",\"text\":\"Found src/a.rs:12\"}'\nsleep 20",
    );
    let job = result(
        command(state.path(), &script)
            .args(["submit", "--cwd"])
            .arg(repo.path())
            .args(["--task", "inspect"]),
    );
    let running = checkpoint(state.path(), &script, &job["id"]);
    assert_eq!(
        unsafe {
            libc::kill(
                running["worker_pid"].as_u64().unwrap() as i32,
                libc::SIGKILL,
            )
        },
        0
    );
    let owner = std::fs::File::open(
        state
            .path()
            .join("jobs")
            .join(job["id"].as_str().unwrap())
            .join("owner.lock"),
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while fs2::FileExt::try_lock_exclusive(&owner).is_err() {
        assert!(
            std::time::Instant::now() < deadline,
            "Worker did not release ownership"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    drop(owner);
    let stopped =
        result(command(state.path(), &script).args(["cancel", job["id"].as_str().unwrap()]));
    assert_eq!(stopped["status"], "cancelled");
    assert_eq!(stopped["report_partial"], true);
    let exported = command(state.path(), &script)
        .args(["report", job["id"].as_str().unwrap()])
        .bounded_output();
    assert!(exported.status.success());
    assert!(
        String::from_utf8(exported.stdout)
            .unwrap()
            .contains("src/a.rs:12")
    );
}

#[test]
fn required_checks_reject_omissions_and_accept_evidenced_coverage() {
    for (claims, expected) in [
        ("", "incomplete"),
        (
            r#"<acceptance>{"1":{"status":"done","evidence":"src/a.rs:1"}}</acceptance>"#,
            "incomplete",
        ),
        (
            r#"<acceptance>{"1":{"status":"done","evidence":"src/a.rs:1"},"2":{"status":"missing","evidence":"Permission denied"}}</acceptance>"#,
            "incomplete",
        ),
        (
            r#"<acceptance>{"1":{"status":"done","evidence":"src/a.rs:1"},"2":{"status":"not_applicable","evidence":"No archive supplied"}}</acceptance>"#,
            "succeeded",
        ),
    ] {
        let state = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let event = json!({"type":"final","sessionId":"fixture","reason":{"kind":"completed"},"text":format!("Report\n{claims}\n")});
        let script = fixture(state.path(), &format!("printf '%s\\n' '{event}'"));
        let job = result(
            command(state.path(), &script)
                .args(["submit", "--cwd"])
                .arg(repo.path())
                .args([
                    "--task",
                    "inspect",
                    "--check",
                    "Read source",
                    "--check",
                    "Verify outside receipt",
                ]),
        );
        let finished = wait(state.path(), &script, &job["id"]);
        assert_eq!(finished["status"], expected, "{finished}");
        assert_eq!(finished["checks"].as_array().unwrap().len(), 2);
    }
}
