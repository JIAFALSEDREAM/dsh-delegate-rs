//! Read selected dsh credential refs without migrating or writing the source store.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{collections::BTreeMap, ffi::OsString, io::Read, path::Path};

#[derive(Deserialize)]
struct Store {
    version: u64,
    refs: BTreeMap<String, serde_yaml_ng::Value>,
}

pub(crate) fn load(path: &Path, refs: &[String]) -> Result<Vec<(OsString, OsString)>> {
    let file = std::fs::File::open(path).context("Cannot open saved dsh credentials")?;
    ensure!(
        file.metadata()?.is_file(),
        "Saved dsh credentials must be a regular file"
    );
    let mut bytes = Vec::new();
    file.take(1_048_577).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 1_048_576,
        "Saved dsh credentials exceed 1 MB"
    );
    // Parser errors can quote source lines containing secrets. Never expose them.
    let store: Store = serde_yaml_ng::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("Invalid saved dsh credentials; contents are not logged"))?;
    ensure!(
        store.version == 1,
        "Only dsh credential store version 1 is supported; source is never migrated"
    );
    refs.iter()
        .map(|name| {
            let value = store
                .refs
                .get(name)
                .and_then(serde_yaml_ng::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .context(
                    "Requested saved dsh credential ref is missing or not a non-empty string",
                )?;
            Ok((name.into(), value.into()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_requested_refs_are_returned_and_the_store_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("credentials.yaml");
        let source = "version: 1\nrefs:\n  MODEL_API_KEY: selected-secret\n  OTHER_API_KEY: unrelated-secret\nrecords: {}\n";
        std::fs::write(&file, source).unwrap();
        let selected = load(&file, &["MODEL_API_KEY".into()]).unwrap();
        assert_eq!(
            selected,
            vec![("MODEL_API_KEY".into(), "selected-secret".into())]
        );
        assert_eq!(std::fs::read_to_string(file).unwrap(), source);
    }
    #[test]
    fn malformed_legacy_missing_and_non_string_refs_fail_without_exposing_values() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("credentials.yaml");
        for source in [
            "version: 1\nrefs: [sensitive-fixture",
            "MODEL_API_KEY: sensitive-fixture",
            "version: 0\nrefs:\n  MODEL_API_KEY: sensitive-fixture",
            "version: 1\nrefs:\n  MODEL_API_KEY: [sensitive-fixture]",
            "version: 1\nrefs:\n  OTHER_API_KEY: sensitive-fixture",
        ] {
            std::fs::write(&file, source).unwrap();
            assert!(
                !format!("{:#}", load(&file, &["MODEL_API_KEY".into()]).unwrap_err())
                    .contains("sensitive-fixture")
            );
            assert_eq!(std::fs::read_to_string(&file).unwrap(), source);
        }
    }
}
