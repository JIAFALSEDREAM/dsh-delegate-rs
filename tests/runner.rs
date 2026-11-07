#![cfg(unix)]
mod common;
use dsh_delegate_rs::{config::Config, runner};
use std::{os::unix::fs::PermissionsExt, time::Duration};

async fn fake(body: &str) -> anyhow::Result<serde_json::Value> {
    fake_secret(body, None).await
}
async fn fake_secret(body: &str, secret: Option<&str>) -> anyhow::Result<serde_json::Value> {
    let state = tempfile::tempdir()?;
    let repo = tempfile::tempdir()?;
    let backend = state.path().join("backend");
    std::fs::write(&backend, format!("#!/bin/sh\ncat >/dev/null\n{body}\n"))?;
    std::fs::set_permissions(&backend, std::fs::Permissions::from_mode(0o700))?;
    let environment = state.path().join("api.env");
    if let Some(secret) = secret {
        std::fs::write(&environment, format!("FIXTURE_SECRET={secret}\n"))?;
    }
    let cfg = Config {
        codex: backend,
        dsh: "dsh".into(),
        dsh_env_file: secret.map(|_| environment),
        model: None,
        timeout: 1,
        state_dir: None,
        ..Config::default()
    };
    tokio::time::timeout(
        Duration::from_secs(9),
        runner::run(
            &cfg,
            repo.path(),
            "inspect",
            None,
            state.path(),
            async |event| {
                if let Some(secret) = secret {
                    assert!(!event.to_string().contains(secret));
                }
                Ok(())
            },
        ),
    )
    .await?
}
const SESSION: &str = "printf '%s\\n' '{\"type\":\"session\",\"sessionId\":\"fixture-session\"}'";
const FINAL: &str = "printf '%s\\n' '{\"type\":\"final\",\"sessionId\":\"fixture-session\",\"reason\":{\"kind\":\"completed\"},\"text\":\"ok\"}'";
fn tool_stream(error: serde_json::Value) -> String {
    let call = serde_json::json!({"type":"tool_call","tool":"bash","callId":"x","input":{}});
    let result = serde_json::json!({"type":"tool_result","tool":"bash","callId":"x","status":"error","error":error,"result":"tool failed"});
    format!("{SESSION}\nprintf '%s\\n' '{call}' '{result}'\n{FINAL}")
}
const SUCCESS: &str = "printf '%s\\n' '{\"type\":\"session\",\"sessionId\":\"fixture-session\"}' '{\"type\":\"final\",\"sessionId\":\"fixture-session\",\"reason\":{\"kind\":\"completed\"},\"text\":\"ok\"}'";

#[tokio::test]
async fn unreadable_instruction_imports_fail_before_starting_backend() {
    let state = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("AGENTS.md"), "@missing-rules.md").unwrap();
    let error = runner::run(
        &Config::default(),
        repo.path(),
        "inspect",
        None,
        state.path(),
        async |_| {
            panic!("Backend must not start with missing instructions");
        },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("missing-rules.md"));
    assert!(!state.path().join("tmp").exists());
}

#[tokio::test]
async fn every_task_pins_headless_ptc_and_model_in_a_private_overlay() {
    for (model, provider, effort) in [
        (None, "deepseek-official", None),
        (Some("custom-model"), "custom-provider", None),
        (
            Some("custom-model"),
            "custom-provider",
            Some("provider-fast"),
        ),
        (Some("deepseek/deepseek-v4.1-flash"), "commandcode", None),
    ] {
        let state = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let script = state.path().join("dsh.sh");
        let args = state.path().join("args");
        let input = state.path().join("input");
        let home = state.path().join("home-path");
        std::fs::write(repo.path().join("AGENTS.md"), "INHERITED_RULE").unwrap();
        std::fs::write(
            &script,
            format!(
                "printf '%s\\n' \"$@\" > '{}'\nprintf '%s' \"$DSH_HOME\" > '{}'\ncat > '{}'\n{}\n",
                args.display(),
                home.display(),
                input.display(),
                SUCCESS.replace("fixture-session", "opaque:session/identity")
            ),
        )
        .unwrap();
        let sandbox_args = state.path().join("sandbox-args");
        let codex = state.path().join("sandbox-wrapper");
        std::fs::write(
            &codex,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexec '{}' \"$@\"\n",
                sandbox_args.display(),
                common::backend(state.path()).display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = Config {
            codex,
            dsh: "/bin/sh".into(),
            dsh_args: vec![script.to_str().unwrap().into()],
            model: model.map(str::to_owned),
            provider: provider.into(),
            effort: effort.map(str::to_owned),
            timeout: 2,
            ..Config::default()
        };
        let result = runner::run(
            &config,
            repo.path(),
            "inspect\nsecond line",
            Some("opaque:session/identity"),
            state.path(),
            async |_| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(result["status"], "succeeded", "{result}");
        assert_eq!(result["conversation_id"], "opaque:session/identity");
        assert!(result.get("usage").is_none(), "dsh does not emit usage");
        let argv = std::fs::read_to_string(args).unwrap();
        let patch = state.path().join("managed.yml");
        assert_eq!(
            argv.lines().collect::<Vec<_>>(),
            [
                "--profile",
                "headless",
                "--patch",
                patch.to_str().unwrap(),
                "--json",
                "--permission-mode",
                "read-only",
                "--approval-policy",
                "never",
                "--session-id",
                "opaque:session/identity",
                "-",
            ]
        );
        assert_eq!(
            std::fs::read_to_string(home).unwrap(),
            state.path().join("tmp/dsh-home").to_str().unwrap()
        );
        let prompt = std::fs::read_to_string(input).unwrap();
        assert!(prompt.contains("INHERITED_RULE"));
        assert!(prompt.ends_with("inspect\nsecond line"));
        assert!(serde_json::from_str::<serde_json::Value>(&prompt).is_err());
        let outer_argv = std::fs::read_to_string(sandbox_args).unwrap();
        let filesystem = outer_argv
            .lines()
            .find_map(|arg| arg.strip_prefix("permissions.dsh_delegate.filesystem="))
            .unwrap();
        let policy: toml::Table = toml::from_str(&format!("filesystem={filesystem}")).unwrap();
        let paths = policy["filesystem"].as_table().unwrap();
        assert_eq!(paths[":root"].as_str(), Some("read"));
        for (path, access) in paths {
            if access.as_str() == Some("write") {
                assert!(
                    !patch.starts_with(path),
                    "Managed overlay is writable through {path}"
                );
            }
        }
        let rows: serde_json::Value =
            serde_json::from_slice(&std::fs::read(patch).unwrap()).unwrap();
        let row = |id| {
            rows.as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == id)
                .unwrap()
        };
        let mut expected =
            serde_json::json!({"provider":provider,"model":model.unwrap_or("deepseek-flash")});
        if let Some(effort) = effort {
            expected["reasoningEffort"] = effort.into();
        }
        assert_eq!(row("agent-default-model")["config"], expected);
        assert_eq!(row("tool-executor-native")["disabled"], true);
        assert_eq!(row("tool-executor-ptc")["disabled"], false);
        assert_eq!(row("ptc-runtime")["config"]["executionMode"], "per-call");
        assert_eq!(
            row("tool-fs")["config"],
            serde_json::json!({"read":true,"readImage":true,"write":false,"edit":false,"applyPatch":false})
        );
        assert_eq!(
            row("tool-bash")["config"],
            serde_json::json!({"enableRunInBackground":false,"promoteOnTimeout":false})
        );
        for id in [
            "tool-plugin-manager",
            "tool-subagent",
            "tool-subagent-control",
            "tool-subagent-fork",
            "tool-workflow",
            "tool-goal",
            "goal-round-driver",
            "tool-todo",
            "plan-mode",
        ] {
            assert_eq!(row(id)["disabled"], true, "{id}");
        }
    }
}

#[tokio::test]
async fn library_runner_rejects_invalid_log_limits_before_launch() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        log_bytes: u64::MAX,
        ..Config::default()
    };
    let error = runner::run(
        &config,
        dir.path(),
        "inspect",
        None,
        dir.path(),
        async |_| Ok(()),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("log_bytes"));
    assert!(!dir.path().join("tmp").exists());
}

#[tokio::test]
async fn wrapper_argv_precedes_managed_dsh_options() {
    let state = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let codex = state.path().join("codex");
    std::fs::write(
        &codex,
        "#!/bin/sh\nwhile [ \"$1\" != -- ]; do shift; done\nshift\nexec \"$@\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o700)).unwrap();
    let dsh = state.path().join("dsh.sh");
    std::fs::write(&dsh, format!("[ \"$1\" = 'wrapper argument' ] || exit 99\n[ \"$2\" = --profile ] || exit 98\ncat >/dev/null\n{SUCCESS}\n")).unwrap();
    let config = Config {
        codex,
        dsh: "/bin/sh".into(),
        dsh_args: vec![dsh.to_str().unwrap().into(), "wrapper argument".into()],
        timeout: 2,
        ..Config::default()
    };
    let result = runner::run(
        &config,
        repo.path(),
        "inspect",
        None,
        state.path(),
        async |_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "succeeded");
}

#[tokio::test]
async fn credential_redaction_cannot_hide_a_permission_denial() {
    let body =
        tool_stream(serde_json::json!({"name":"PermissionError","code":"PERMISSION_DENIED"}));
    let result = fake_secret(&body, Some("PERMISSION_DENIED")).await.unwrap();
    assert_eq!(result["status"], "incomplete");
}

#[tokio::test]
async fn permission_denials_cannot_be_reported_as_completed_investigations() {
    for body in [
        format!("printf 'cat: private.txt: Permission denied\\n' >&2\n{SUCCESS}"),
        format!("printf 'EACCES: permission denied\\n' >&2\n{SUCCESS}"),
        tool_stream(serde_json::json!({"name":"PermissionError","code":"PERMISSION_DENIED"})),
        format!("head -c 70000 /dev/zero >&2\nprintf 'permission denied' >&2\n{SUCCESS}"),
        tool_stream(
            serde_json::json!({"name":"PermissionError","message":"EACCES: permission denied"}),
        ),
        format!(
            "head -c 256 /dev/zero | tr '\\000' ' ' >&2; sleep 0.1; printf 'permission denied\\n' >&2\n{SUCCESS}"
        ),
    ] {
        let value = fake(&body).await.unwrap();
        assert_eq!(value["status"], "incomplete");
        assert_eq!(value["response"], "ok");
        assert!(value["error"].as_str().unwrap().contains("denied"));
    }
    for diagnostic in [
        "permission declaration: missing file",
        "no permission denied errors",
        "quoted: permission denied",
        "operation requires approval",
        "Error: no permission denied errors",
        "permission denied errors: 0",
        "SOFT_DENIED_COUNT",
    ] {
        let value = fake(&format!("printf '%s' '{diagnostic}' >&2\n{SUCCESS}"))
            .await
            .unwrap();
        assert_eq!(value["status"], "succeeded", "{diagnostic}");
        let body = tool_stream(serde_json::json!({"name":"ToolError","message":diagnostic}));
        assert_eq!(fake(&body).await.unwrap()["status"], "succeeded");
    }
    let body = format!(
        "{{ printf 'quoted: '; head -c 8049 /dev/zero | tr '\\000' x; printf 'permission denied'; sleep 0.1; printf ' more context\\n'; }} >&2\n{SUCCESS}"
    );
    assert_eq!(fake(&body).await.unwrap()["status"], "succeeded");
    let body =
        format!("printf 'permission denied' >&2; sleep 0.1; printf ' errors: 0\\n' >&2\n{SUCCESS}");
    assert_eq!(fake(&body).await.unwrap()["status"], "succeeded");
}

#[tokio::test]
async fn ownership_acknowledgement_precedes_task_delivery_and_is_cancellable() {
    for ordinary_event in [false, true] {
        let state = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let backend = state.path().join("backend");
        let input = state.path().join("input");
        std::fs::write(
            &backend,
            format!(
                "#!/bin/sh\ncat > '{}'\nprintf '%s\\n' '{{\"type\":\"session\",\"sessionId\":\"fixture-session\"}}'\nsleep 30\n",
                input.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&backend, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cfg = Config {
            codex: backend,
            dsh: "dsh".into(),
            dsh_env_file: None,
            model: None,
            timeout: 30,
            state_dir: None,
            ..Config::default()
        };
        let result = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(
                runner::run(
                    &cfg,
                    repo.path(),
                    "inspect",
                    None,
                    state.path(),
                    async |event| {
                        if ordinary_event && event["event"] == "sandbox_started" {
                            return Ok(());
                        }
                        std::future::pending::<anyhow::Result<()>>().await
                    }
                ),
                async {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    if !ordinary_event {
                        assert!(std::fs::read(&input).unwrap_or_default().is_empty());
                    }
                    std::fs::write(state.path().join("cancel"), "cancel").unwrap();
                }
            )
            .0
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result["status"], "cancelled");
    }
}

#[tokio::test]
async fn inherited_stderr_pipe_cannot_hang_a_completed_task() {
    let result = fake(&format!("sleep 30 >/dev/null &\n{SUCCESS}"))
        .await
        .unwrap();
    assert_eq!(result["status"], "succeeded");
}

#[tokio::test]
async fn unterminated_result_survives_cancelled_reads() {
    let result = fake(&format!("{SESSION}\nprintf '%s' '{{\"type\":\"final\",\"sessionId\":\"fixture-session\",\"reason\":{{\"kind\":\"completed\"}},\"text\":\"ok\"}}'\nsleep 0.2"))
        .await
        .unwrap();
    assert_eq!(result["status"], "succeeded");
}
#[tokio::test]
async fn stderr_is_drained_and_non_utf8_is_lossy() {
    let result = fake(&format!(
        "printf '\\377' >&2\nhead -c 100000 /dev/zero >&2\n{SUCCESS}"
    ))
    .await
    .unwrap();
    assert_eq!(result["status"], "succeeded");
}
#[tokio::test]
async fn total_stdout_and_unterminated_frame_limits_are_enforced() {
    for body in [
        "head -c 5000000 /dev/zero",
        r#"python3 -c 'import json; print(json.dumps({"type":"session","sessionId":"fixture-session"})); [print(json.dumps({"type":"text","text":"x"*90000})) for _ in range(50)]'"#,
    ] {
        let error = fake(body).await.unwrap_err();
        assert!(format!("{error:#}").contains("exceeded 4 MB"), "{error:#}");
    }
}
#[tokio::test]
async fn final_reason_and_process_exit_both_determine_completion() {
    for (reason, text, exit, expected) in [
        (
            serde_json::json!({"kind":"completed"}),
            "Evidence 🦀",
            0,
            "succeeded",
        ),
        (
            serde_json::json!({"kind":"completed"}),
            "Evidence",
            7,
            "failed",
        ),
        (serde_json::json!({"kind":"completed"}), "  ", 0, "failed"),
        (
            serde_json::json!({"kind":"aborted"}),
            "partial",
            0,
            "interrupted",
        ),
        (
            serde_json::json!({"kind":"error","error":{"message":"provider unavailable"}}),
            "partial",
            0,
            "failed",
        ),
        (serde_json::Value::Null, "partial", 0, "failed"),
    ] {
        let final_event = serde_json::json!({"type":"final","sessionId":"fixture-session","reason":reason,"text":text});
        let result = fake(&format!(
            "{SESSION}\nprintf '%s\\n' '{final_event}'\nexit {exit}"
        ))
        .await
        .unwrap();
        assert_eq!(result["status"], expected, "{result}");
        assert_eq!(result["response"], text);
        assert_eq!(result["conversation_id"], "fixture-session");
    }
    assert_eq!(fake(SESSION).await.unwrap()["status"], "failed");
    let result = fake("printf '%s\\n' '{\"type\":\"error\",\"message\":\"profile unavailable\"}'")
        .await
        .unwrap();
    assert_eq!(result["status"], "failed");
    assert_eq!(result["error"], "profile unavailable");
}

#[tokio::test]
async fn malformed_or_out_of_order_session_streams_are_protocol_failures() {
    for body in [
        format!("{SESSION}\n{SESSION}\n{FINAL}"),
        format!("{SESSION}\n{FINAL}\n{FINAL}"),
        format!("{SESSION}\n{FINAL}\nprintf '%s\\n' '{{\"type\":\"text\",\"text\":\"late\"}}'"),
        format!(
            "{SESSION}\n{}",
            FINAL.replace("fixture-session", "different")
        ),
        FINAL.into(),
        format!(
            "{SESSION}\nprintf '%s\\n' '{{\"type\":\"final\",\"sessionId\":\"fixture-session\",\"reason\":[],\"text\":\"bad\"}}'"
        ),
        format!("{SESSION}\nprintf '%s\\n' 'not-json'"),
        format!("{SESSION}\nprintf '%s\\n' '{{\"event\":\"sandbox_started\",\"pid\":null}}'"),
    ] {
        let result = fake(&body).await;
        assert!(
            result.is_err(),
            "Invalid stream accepted: {body}: {result:?}"
        );
    }
}

#[tokio::test]
async fn resumed_session_identity_must_match_the_requested_opaque_id() {
    let state = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let backend = state.path().join("backend");
    std::fs::write(&backend, format!("#!/bin/sh\ncat >/dev/null\n{SUCCESS}\n")).unwrap();
    std::fs::set_permissions(&backend, std::fs::Permissions::from_mode(0o700)).unwrap();
    let result = runner::run(
        &Config {
            codex: backend,
            timeout: 2,
            ..Config::default()
        },
        repo.path(),
        "inspect",
        Some("requested:opaque/id"),
        state.path(),
        async |_| Ok(()),
    )
    .await;
    assert!(
        result.is_err(),
        "Mismatched resumed session accepted: {result:?}"
    );
}

#[test]
fn relative_codex_uses_host_directory() {
    let host = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    for (dir, response) in [(host.path(), "trusted"), (repo.path(), "untrusted")] {
        let backend = dir.join("backend");
        std::fs::write(&backend,format!("#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{{\"type\":\"session\",\"sessionId\":\"fixture-session\"}}' '{{\"type\":\"final\",\"sessionId\":\"fixture-session\",\"reason\":{{\"kind\":\"completed\"}},\"text\":\"{response}\"}}'\n")).unwrap();
        std::fs::set_permissions(&backend, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_dsh-delegate-rs"))
        .current_dir(host.path())
        .arg("--codex")
        .arg("./backend")
        .arg("--state-dir")
        .arg(state.path())
        .arg("run")
        .arg("--cwd")
        .arg(repo.path())
        .args(["--task", "inspect"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["response"], "trusted");
}

#[test]
fn relative_project_and_dsh_resolve_to_the_same_directory() {
    let host = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let project = host.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let codex = host.path().join("codex");
    std::fs::write(&codex,"#!/bin/sh\nwhile [ \"$1\" != \"--\" ]; do\nif [ \"$1\" = \"-C\" ]; then shift; cd \"$1\" || exit; fi\nshift\ndone\nshift\nexec \"$@\"\n").unwrap();
    let dsh = project.join("backend");
    std::fs::write(&dsh, format!("#!/bin/sh\ncat >/dev/null\n{SUCCESS}\n")).unwrap();
    for executable in [&codex, &dsh] {
        std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_dsh-delegate-rs"))
        .current_dir(host.path())
        .args(["--codex", "./codex", "--dsh", "./backend"])
        .arg("--state-dir")
        .arg(state.path())
        .args(["run", "--cwd", "project", "--task", "inspect"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn cancellation_after_stdout_eof_stops_the_task() {
    for blocked_stdin in [false, true] {
        let state = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let backend = state.path().join("backend");
        let body = if blocked_stdin {
            "#!/bin/sh\nsleep 30\n"
        } else {
            "#!/bin/sh\ncat >/dev/null\nexec 1>&-\nsleep 30\n"
        };
        std::fs::write(&backend, body).unwrap();
        std::fs::set_permissions(&backend, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cfg = Config {
            codex: backend,
            dsh: "dsh".into(),
            dsh_env_file: None,
            model: None,
            timeout: 30,
            state_dir: None,
            ..Config::default()
        };
        let cancel = state.path().join("cancel");
        let task = if blocked_stdin {
            "x".repeat(100_000)
        } else {
            "inspect".into()
        };
        let result = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(
                runner::run(&cfg, repo.path(), &task, None, state.path(), async |_| Ok(
                    ()
                )),
                async {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    std::fs::write(cancel, "cancel").unwrap();
                }
            )
            .0
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result["status"], "cancelled");
    }
}
