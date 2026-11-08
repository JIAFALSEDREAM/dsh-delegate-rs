use anyhow::{Result, ensure};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Args, Clone, Debug, Default, Deserialize, Serialize)]
pub struct TaskOptions {
    /// Override this task's timeout in seconds (1–7200).
    #[arg(long = "task-timeout")]
    #[serde(rename = "timeout")]
    pub timeout_seconds: Option<u64>,
    /// Required investigation item; repeat for each item.
    #[arg(long = "check")]
    #[serde(default)]
    pub checks: Vec<String>,
}

impl TaskOptions {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.timeout_seconds.is_none_or(|n| (1..=7200).contains(&n)),
            "Task timeout must be 1–7200 seconds"
        );
        ensure!(
            self.checks.len() <= 32
                && self
                    .checks
                    .iter()
                    .all(|s| !s.trim().is_empty() && s.chars().count() <= 500),
            "Provide at most 32 checks, each containing 1–500 characters"
        );
        Ok(())
    }

    pub(crate) fn prompt(&self, task: &str, timeout: u64) -> String {
        let mut text = format!(
            "Task wall-clock budget: {timeout} seconds, including startup. Reserve the final quarter \
             for reporting; stop investigation when evidence is sufficient. After discovery and \
             verification, emit a short text checkpoint with findings, source references and remaining \
             gaps before more tool calls. Checkpoints are retained on timeout/cancellation; do not \
             include private reasoning.\n\nTask:\n{task}"
        );
        if !self.checks.is_empty() {
            text.push_str("\n\nRequired investigation checks (all must be accounted for):\n");
            for (i, item) in self.checks.iter().enumerate() {
                text.push_str(&format!("{}: {}\n", i + 1, item));
            }
            text.push_str(
                "End the final report with <acceptance>JSON</acceptance>. JSON must be an object \
                 keyed by every check number as a string; each value has status \
                 (done, missing, or not_applicable) and nonempty evidence (source references, \
                 or the reason it is missing/not applicable). Account for checks even when blocked. \
                 Example: <acceptance>{\"1\":{\"status\":\"done\",\"evidence\":\"src/a.rs:12\"}}</acceptance>. \
                 These are investigation coverage claims, not proof that the investigated product passes."
            );
        }
        text
    }

    pub(crate) fn assess(&self, job: &mut Value) {
        if self.checks.is_empty() {
            return;
        }
        let report = job["report"].as_str().unwrap_or("").trim_end();
        let claims = report
            .rsplit_once("<acceptance>")
            .and_then(|(_, tail)| tail.strip_suffix("</acceptance>"))
            .and_then(|s| serde_json::from_str::<Value>(s.trim()).ok());
        let mut complete = claims
            .as_ref()
            .and_then(Value::as_object)
            .is_some_and(|map| map.len() == self.checks.len());
        let checks: Vec<_> = (0..self.checks.len())
            .map(|i| {
                let id = (i + 1).to_string();
                let claim = &claims.as_ref().unwrap_or(&Value::Null)[&id];
                let state = claim["status"].as_str().unwrap_or("missing");
                let evidence = claim["evidence"].as_str().unwrap_or("");
                let valid =
                    matches!(state, "done" | "not_applicable") && !evidence.trim().is_empty();
                complete &= valid;
                json!({"id":id,"status":if valid {state} else {"missing"},"evidence":evidence})
            })
            .collect();
        job["checks"] = checks.into();
        if !complete && job["status"] == "succeeded" {
            job["status"] = "incomplete".into();
            job["error"] = "Required investigation checks are missing or lack evidence".into();
        }
    }
}
