//! Deployment choices owned by the supervisor, repeated on every invocation.
use crate::config::{Config, DEFAULT_MODEL};
use anyhow::{Context, Result, ensure};
use cap_std::fs::Dir;
use serde_json::{Value, json};
use std::{io::Write, path::Path};

pub(crate) type PluginLinks = std::collections::BTreeMap<std::path::PathBuf, std::path::PathBuf>;

/// Pin the exact external module roots permitted in managed storage scans.
fn plugin_links(packages: &[std::path::PathBuf]) -> Result<PluginLinks> {
    let mut links = PluginLinks::new();
    for package in packages {
        let target = package
            .canonicalize()
            .context("Cannot resolve local dsh plugin package")?;
        let file = std::fs::File::open(target.join("package.json"))?;
        let mut bytes = Vec::new();
        use std::io::Read;
        file.take(65_537).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 65_536, "Plugin package.json exceeds 64 KiB");
        let manifest: Value =
            serde_json::from_slice(&bytes).context("Invalid plugin package.json")?;
        let name = manifest["name"]
            .as_str()
            .context("Plugin package needs a name")?;
        let parts = name.split('/').collect::<Vec<_>>();
        let valid = |part: &str| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        };
        ensure!(
            (parts.len() == 1 && valid(parts[0]))
                || (parts.len() == 2
                    && parts[0].starts_with('@')
                    && valid(&parts[0][1..])
                    && valid(parts[1])),
            "Invalid local plugin package name"
        );
        let relative = Path::new("profiles/headless/node_modules").join(name);
        ensure!(
            links.insert(relative, target).is_none(),
            "Duplicate local plugin package name"
        );
    }
    Ok(links)
}

/// Keep imports on the current dsh runtime's linked-profile resolver.
pub(crate) fn link_plugins(home: &Dir, packages: &[std::path::PathBuf]) -> Result<PluginLinks> {
    let links = plugin_links(packages)?;
    for (relative, target) in &links {
        let mut parent = home.try_clone()?;
        for part in relative.parent().unwrap().components() {
            parent = crate::profiles::child(&parent, part.as_os_str().to_str().unwrap())?;
        }
        let leaf = relative.file_name().unwrap();
        match parent.symlink_metadata(leaf) {
            Ok(metadata) => ensure!(
                metadata.is_symlink() && parent.read_link_contents(leaf)? == *target,
                "Private plugin link changed; refusing to replace it"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                #[cfg(not(windows))]
                parent.symlink_contents(target, leaf)?;
                #[cfg(windows)]
                parent.symlink_dir(target, leaf)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(links)
}

pub(crate) fn provider_patches(command: &mut tokio::process::Command, config: &Config) {
    for patch in &config.dsh_patches {
        command.arg("--patch").arg(patch);
    }
}

pub(crate) fn overlay(config: &Config) -> Value {
    let mut model = json!({"provider":config.provider,
        "model":config.model.as_deref().unwrap_or(DEFAULT_MODEL)});
    if let Some(effort) = &config.effort {
        model["reasoningEffort"] = effort.clone().into();
    }
    let mut rows = vec![
        json!({"id":"agent-default-model","config":model}),
        json!({"id":"tool-executor-native","disabled":true}),
        json!({"id":"tool-executor-ptc","disabled":false}),
        json!({"id":"ptc-runtime","config":{"executionMode":"per-call"}}),
        json!({"id":"tool-fs","config":{"read":true,"readImage":true,
            "write":false,"edit":false,"applyPatch":false}}),
        json!({"id":"tool-bash","config":{"enableRunInBackground":false,"promoteOnTimeout":false}}),
        json!({"id":"system-prompt","config":{"personaPrefix":
            "You are a read-only investigator delegated by a parent agent. Gather evidence and answer the assigned task. Do not modify project files, implement changes, or delegate work."}}),
    ];
    for id in [
        "tool-plugin-manager",
        "tool-subagent-control",
        "tool-subagent",
        "tool-subagent-fork",
        "tool-workflow",
        "tool-todo",
        "tool-goal",
        "goal-round-driver",
        "plan-mode",
        "tool-jobs",
        "jobs",
        "workflow-ptc",
        "subagent-spawn-in-process",
        "subagent-fork-in-process",
        "subagent",
        "command-goal",
        "goal",
        "command-feedback",
        "user-questions",
        "tool-skill",
        "skill-filesystem",
        "skill",
        "tool-working-directory",
        "plugin-manager",
        "config-editor",
        "settings",
        "session-title-llm",
        "session-telemetry-otel",
        "session-log-deepseek",
    ] {
        rows.push(json!({"id":id,"disabled":true}));
    }
    Value::Array(rows)
}

/// JSON is a YAML subset. Atomic replacement also removes any previous symlink.
pub(crate) fn write_overlay(config: &Config, job: &Path) -> Result<std::path::PathBuf> {
    let path = job.join("managed.yml");
    let mut file = tempfile::NamedTempFile::new_in(job)?;
    file.write_all(&serde_json::to_vec_pretty(&overlay(config))?)?;
    file.as_file().sync_all()?;
    file.persist(&path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn managed_overlay_pins_execution_and_model_without_credentials() {
        let config = Config {
            provider: "custom".into(),
            model: Some("chosen".into()),
            effort: Some("high".into()),
            ..Config::default()
        };
        let rows = overlay(&config);
        let row = |id| {
            rows.as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == id)
                .unwrap()
        };
        assert_eq!(row("tool-executor-native")["disabled"], true);
        assert_eq!(row("tool-executor-ptc")["disabled"], false);
        assert_eq!(
            row("agent-default-model")["config"],
            json!({"provider":"custom","model":"chosen","reasoningEffort":"high"})
        );
        assert_eq!(row("tool-subagent")["disabled"], true);
        assert_eq!(row("tool-fs")["config"]["write"], false);
        assert_eq!(row("tool-fs")["config"]["edit"], false);
        assert_eq!(row("tool-bash")["config"]["enableRunInBackground"], false);
        let dir = tempfile::tempdir().unwrap();
        let path = write_overlay(&config, dir.path()).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap(),
            rows
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_plugins_are_linked_without_following_redirected_private_directories() {
        let home = tempfile::tempdir().unwrap();
        let package = tempfile::tempdir().unwrap();
        std::fs::write(
            package.path().join("package.json"),
            r#"{"name":"@fixture/provider"}"#,
        )
        .unwrap();
        let root = Dir::open_ambient_dir(home.path(), cap_std::ambient_authority()).unwrap();
        let packages = vec![package.path().to_owned()];
        let links = link_plugins(&root, &packages).unwrap();
        link_plugins(&root, &packages).unwrap();
        assert!(crate::tree::usage(&root, 1_000_000).is_err());
        assert!(crate::tree::usage_with_links(&root, 1_000_000, &links).unwrap() < 1_000_000);
        let link = home
            .path()
            .join("profiles/headless/node_modules/@fixture/provider");
        assert_eq!(
            std::fs::read_link(link).unwrap(),
            package.path().canonicalize().unwrap()
        );
        let outside = tempfile::tempdir().unwrap();
        std::fs::rename(
            home.path().join("profiles"),
            home.path().join("old-profiles"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path(), home.path().join("profiles")).unwrap();
        assert!(link_plugins(&root, &packages).is_err());
        assert!(crate::tree::usage_with_links(&root, 1_000_000, &links).is_err());
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(package.path()).unwrap().count(), 1);
    }

    #[test]
    fn invalid_package_names_cannot_escape_private_modules() {
        let home = tempfile::tempdir().unwrap();
        let package = tempfile::tempdir().unwrap();
        let root = Dir::open_ambient_dir(home.path(), cap_std::ambient_authority()).unwrap();
        for name in [
            "..",
            "@../escape",
            "@scope/../escape",
            "absolute/path",
            "@scope/..",
        ] {
            std::fs::write(
                package.path().join("package.json"),
                json!({"name":name}).to_string(),
            )
            .unwrap();
            assert!(link_plugins(&root, &[package.path().to_owned()]).is_err());
        }
    }
}
