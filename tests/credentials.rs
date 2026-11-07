use dsh_delegate_rs::config::Config;

#[test]
fn saved_credentials_are_fallbacks_and_never_enter_config_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let store = dir.path().join("credentials.yaml");
    let env = dir.path().join("api.env");
    let key = "DSH_DELEGATE_FIXTURE_API_KEY";
    std::fs::write(
        &store,
        format!("version: 1\nrefs:\n  {key}: saved-fixture-secret\nrecords: {{}}\n"),
    )
    .unwrap();
    let config = Config {
        dsh_credentials_file: Some(store.clone()),
        dsh_credential_refs: vec![key.into()],
        ..Config::default()
    };
    config.validate().unwrap();
    let environment = config.environment().unwrap();
    assert_eq!(
        environment[std::ffi::OsStr::new(key)],
        "saved-fixture-secret"
    );
    assert!(
        Config::secrets(&environment)
            .unwrap()
            .contains(&"saved-fixture-secret".into())
    );
    assert!(
        !serde_json::to_string(&config)
            .unwrap()
            .contains("saved-fixture-secret")
    );
    std::fs::write(&env, format!("{key}=env-file-secret\n")).unwrap();
    let config = Config {
        dsh_env_file: Some(env),
        ..config
    };
    assert_eq!(
        config.environment().unwrap()[std::ffi::OsStr::new(key)],
        "env-file-secret"
    );
    std::fs::remove_file(store).unwrap();
    assert!(
        config.environment().is_ok(),
        "Present launch/env-file values need no store access"
    );
    assert!(
        config.lookup_environment().is_ok(),
        "Executable lookup never requires credentials"
    );
}

#[test]
fn credential_configuration_requires_an_explicit_store_and_valid_refs() {
    for config in [
        Config {
            dsh_credentials_file: Some("store.yaml".into()),
            ..Config::default()
        },
        Config {
            dsh_credential_refs: vec!["KEY".into()],
            ..Config::default()
        },
        Config {
            dsh_credentials_file: Some("store.yaml".into()),
            dsh_credential_refs: vec!["BAD=KEY".into()],
            ..Config::default()
        },
    ] {
        assert!(config.validate().is_err());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn saved_key_is_redacted_from_reports_events_and_diagnostics() {
    use dsh_delegate_rs::runner;
    use std::os::unix::fs::PermissionsExt;
    let state = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let store = state.path().join("credentials.yaml");
    let source = "version: 1\nrefs:\n  SAVED_MODEL_API_KEY: saved-sensitive-fixture\nrecords: {}\n";
    std::fs::write(&store, source).unwrap();
    let backend = state.path().join("backend");
    std::fs::write(&backend, "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"session\",\"sessionId\":\"session-a\"}'\nprintf '{\"type\":\"final\",\"sessionId\":\"session-a\",\"reason\":{\"kind\":\"completed\"},\"text\":\"%s\"}\\n' \"$SAVED_MODEL_API_KEY\"\nprintf '%s' \"$SAVED_MODEL_API_KEY\" >&2\n").unwrap();
    std::fs::set_permissions(&backend, std::fs::Permissions::from_mode(0o700)).unwrap();
    let config = Config {
        codex: backend,
        dsh_credentials_file: Some(store.clone()),
        dsh_credential_refs: vec!["SAVED_MODEL_API_KEY".into()],
        ..Config::default()
    };
    let result = runner::run(
        &config,
        repo.path(),
        "inspect",
        None,
        state.path(),
        async |event| {
            assert!(!event.to_string().contains("saved-sensitive-fixture"));
            Ok(())
        },
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "succeeded");
    assert!(!result.to_string().contains("saved-sensitive-fixture"));
    for path in [
        state.path().join("stderr.log"),
        state.path().join("tmp/dsh.log"),
    ] {
        assert!(
            !std::fs::read_to_string(path)
                .unwrap()
                .contains("saved-sensitive-fixture")
        );
    }
    assert_eq!(std::fs::read_to_string(store).unwrap(), source);
}
