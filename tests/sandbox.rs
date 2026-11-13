use dsh_delegate_rs::{
    config::{Config, private_dir},
    runner::classify,
};
use serde_json::json;
use std::process::Stdio;

fn config() -> Config {
    Config {
        codex: "codex".into(),
        dsh: "dsh".into(),
        dsh_env_file: None,
        model: None,
        timeout: 5,
        state_dir: None,
        ..Config::default()
    }
}

#[test]
fn zero_exit_with_no_report_is_failure() {
    for malformed in [json!("bad"), json!([]), json!(true), json!(1)] {
        assert_eq!(classify(0, Some(malformed))["status"], "failed");
    }
    assert_eq!(classify(0, None)["status"], "failed");
    assert_eq!(
        classify(
            0,
            Some(
                json!({"type":"final","sessionId":"fixture","reason":{"kind":"completed"},"text":""})
            )
        )["status"],
        "failed"
    );
    assert_eq!(
        classify(
            0,
            Some(
                json!({"type":"final","sessionId":"fixture","reason":{"kind":"aborted"},"text":"partial"})
            )
        )["status"],
        "interrupted"
    );
    assert_eq!(
        classify(
            1,
            Some(
                json!({"type":"final","sessionId":"fixture","reason":{"kind":"completed"},"text":"fake"})
            )
        )["status"],
        "failed"
    );
}

#[test]
fn decoded_secrets_with_json_escapes_are_redacted() {
    let secret = "fixture\\quoted\"key\nline".to_owned();
    let mut event: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&json!({"nested":[secret]})).unwrap()).unwrap();
    dsh_delegate_rs::runner::redact_value(&mut event, &[secret]);
    assert_eq!(event["nested"][0], "[REDACTED]");
    let mut keys = json!({"fixture-secret-key":{"fixture-secret-key":"fixture-secret-key"}});
    dsh_delegate_rs::runner::redact_value(&mut keys, &["fixture-secret-key".into()]);
    assert_eq!(keys["[REDACTED]"]["[REDACTED]"], "[REDACTED]");
    let mut numeric = json!({"usage":{"key":12345678}});
    dsh_delegate_rs::runner::redact_value(&mut numeric, &["12345678".into()]);
    assert_eq!(numeric["usage"]["key"], "[REDACTED]");
}

#[cfg(unix)]
#[test]
fn executable_lookup_uses_effective_path() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("fixture-backend");
    std::fs::write(&executable, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let env =
        std::collections::BTreeMap::from([("PATH".into(), dir.path().as_os_str().to_owned())]);
    assert_eq!(
        config().executable(std::path::Path::new("fixture-backend"), &env, dir.path()),
        executable
    );
}

#[test]
fn writable_directory_cannot_be_in_project() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = private_dir(&dir.path().join("tmp")).unwrap();
    assert!(config().sandbox(dir.path(), &tmp).is_err());
}

#[test]
#[ignore = "helper invoked explicitly by native sandbox test"]
fn fixture_child() {
    let file =
        std::env::var("DSH_SANDBOX_FIXTURE").expect("sandbox must forward fixture environment");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "original");
    assert!(std::fs::write(file, "changed").is_err());
    let signal = std::path::PathBuf::from(std::env::var_os("DSH_REPORTING_FIXTURE").unwrap());
    assert_eq!(std::fs::read_to_string(&signal).unwrap(), "report");
    assert!(std::fs::write(&signal, "forged").is_err());
    assert!(std::fs::remove_file(&signal).is_err());
    tempfile::NamedTempFile::new().expect("sandbox must permit native temporary files");
}

#[tokio::test]
#[ignore = "requires installed Codex CLI and a usable OS sandbox"]
async fn native_sandbox_preserves_fixture() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let tmp = private_dir(&state.path().join("tmp")).unwrap();
    let file = workspace.path().join("source.txt");
    std::fs::write(&file, "original").unwrap();
    let signal = state.path().join("report.request");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let mut signal_owner = options.open(&signal).unwrap();
    std::io::Write::write_all(&mut signal_owner, b"report").unwrap();
    let mut cmd = config().sandbox(workspace.path(), &tmp).unwrap();
    cmd.arg(std::env::current_exe().unwrap())
        .args(["--exact", "fixture_child", "--nocapture", "--ignored"])
        .env("DSH_SANDBOX_FIXTURE", &file)
        .env("DSH_REPORTING_FIXTURE", &signal)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        cmd.kill_on_drop(true).output(),
    )
    .await
    .unwrap()
    .unwrap();
    drop(signal_owner);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(file).unwrap(), "original");
}

#[cfg(windows)]
#[test]
#[ignore = "helper invoked by Windows environment test"]
fn windows_environment_child() {
    let mut cfg = config();
    cfg.dsh_env_file = Some(std::env::var_os("DSH_ENV_FIXTURE").unwrap().into());
    let env = cfg.environment().unwrap();
    assert_eq!(
        env.get(&std::ffi::OsString::from("PATH")).unwrap(),
        "inherited-path"
    );
    assert!(!env.contains_key(&std::ffi::OsString::from("Path")));
}

#[cfg(windows)]
#[test]
fn windows_inherited_path_takes_precedence_over_dotenv() {
    let state = tempfile::tempdir().unwrap();
    let file = state.path().join("api.env");
    std::fs::write(&file, "PATH=dotenv-path\n").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "windows_environment_child", "--ignored"])
        .env("Path", "inherited-path")
        .env("DSH_ENV_FIXTURE", file)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
