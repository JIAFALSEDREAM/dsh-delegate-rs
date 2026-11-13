//! Signal the live dsh agent to report before the supervisor's final deadline.
use anyhow::Result;
use serde_json::json;
use std::{fs, path::Path, time::Duration};

pub(crate) fn grace(timeout: u64) -> Duration {
    Duration::from_secs(timeout.min(60))
}

pub(crate) fn install(job: &Path, signal: &Path, grace: Duration) -> Result<std::path::PathBuf> {
    let plugin = job.join("reporting.mjs");
    fs::write(&plugin, include_str!("reporting.mjs"))?;
    let patch = job.join("reporting.yml");
    fs::write(
        &patch,
        serde_json::to_vec(&json!([{"insert":[{
            "id":"delegate-reporting", "name":plugin,
            "config":{
                "signal":signal,
                "message":format!(
                    "Your investigation budget has been reached. Stop new investigation and \
                     additional tool calls. Write the final report now using evidence already \
                     collected, and state remaining gaps explicitly without guessing. You have \
                     up to {} seconds for reporting. Account for the original required checks \
                     and include the <acceptance> JSON if requested.", grace.as_secs())
            }
        }]}]))?,
    )?;
    Ok(patch)
}
