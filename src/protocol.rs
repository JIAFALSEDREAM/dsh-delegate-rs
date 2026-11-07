//! Validate the headless projection before translating it into a job outcome.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

#[derive(Default)]
pub(crate) struct Stream {
    session: Option<String>,
    terminal: Option<Value>,
    error: Option<String>,
}

impl Stream {
    pub fn observe(&mut self, event: &Value, resumed: Option<&str>) -> Result<()> {
        ensure!(
            self.terminal.is_none() && self.error.is_none(),
            "Event after terminal dsh event"
        );
        let kind = event["type"].as_str().context("Missing dsh event type")?;
        if kind == "error" {
            self.error = Some(
                event["message"]
                    .as_str()
                    .context("Missing dsh error message")?
                    .to_owned(),
            );
            return Ok(());
        }
        if kind == "session" {
            ensure!(self.session.is_none(), "Duplicate dsh session event");
            let id = event["sessionId"]
                .as_str()
                .filter(|id| !id.is_empty())
                .context("Missing dsh session ID")?;
            ensure!(
                resumed.is_none_or(|expected| expected == id),
                "dsh resumed a different session"
            );
            self.session = Some(id.to_owned());
            return Ok(());
        }
        let id = self
            .session
            .as_deref()
            .context("dsh event before opening session")?;
        if kind == "final" {
            ensure!(
                event["sessionId"].as_str() == Some(id),
                "dsh final session ID mismatch"
            );
            ensure!(
                event["text"].is_string()
                    && (event["reason"].is_object() || event["reason"].is_null()),
                "Invalid dsh final event"
            );
            self.terminal = Some(event.clone());
        }
        Ok(())
    }

    pub fn finish(self, exit: i32) -> Value {
        let mut outcome = classify(exit, self.terminal);
        outcome["conversation_id"] = self.session.into();
        if let Some(error) = self.error {
            outcome["status"] = "failed".into();
            outcome["error"] = error.into();
        }
        outcome
    }
}

pub fn classify(exit: i32, terminal: Option<Value>) -> Value {
    let final_event = terminal.unwrap_or(Value::Null);
    let text = final_event["text"].as_str();
    let kind = final_event["reason"]["kind"].as_str();
    let status = match (exit, kind, text) {
        (0, Some("completed"), Some(text)) if !text.trim().is_empty() => "succeeded",
        (_, Some("aborted" | "interrupted"), _) => "interrupted",
        (_, Some("blocked"), _) => "blocked",
        (_, Some("max-tokens"), _) => "incomplete",
        _ => "failed",
    };
    let error = if status == "succeeded" {
        Value::Null
    } else {
        final_event["reason"]["error"]["message"]
            .as_str()
            .unwrap_or(match kind {
                Some("completed") if exit != 0 => "dsh exited unsuccessfully after its final event",
                Some("completed") => "dsh completed without a report",
                Some("aborted" | "interrupted") => "dsh turn was interrupted",
                Some("blocked") => "dsh turn is blocked",
                Some("max-tokens") => "dsh turn exhausted its token budget",
                Some("error") => "dsh turn failed",
                _ => "dsh exited without a completed terminal turn",
            })
            .into()
    };
    json!({"status":status,"response":text,"conversation_id":final_event["sessionId"],
        "reason":final_event["reason"],"error":error,"exit_code":exit})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn final_event(reason: Value) -> Value {
        json!({"type":"final","sessionId":"session-a","reason":reason,"text":"Evidence 🦀"})
    }
    fn opened() -> Stream {
        let mut stream = Stream::default();
        stream
            .observe(&json!({"type":"session","sessionId":"session-a"}), None)
            .unwrap();
        stream
    }
    #[test]
    fn success_requires_both_process_and_durable_turn_completion() {
        for (exit, reason, expected) in [
            (0, json!({"kind":"completed"}), "succeeded"),
            (1, json!({"kind":"completed"}), "failed"),
            (0, Value::Null, "failed"),
            (0, json!({"kind":"aborted"}), "interrupted"),
            (
                1,
                json!({"kind":"error","error":{"message":"provider unavailable"}}),
                "failed",
            ),
        ] {
            let mut stream = opened();
            stream.observe(&final_event(reason), None).unwrap();
            let result = stream.finish(exit);
            assert_eq!(result["status"], expected);
            assert_eq!(result["response"], "Evidence 🦀");
            assert_eq!(result["conversation_id"], "session-a");
        }
        let mut event = final_event(json!({"kind":"completed"}));
        event["text"] = "  ".into();
        assert_eq!(classify(0, Some(event))["status"], "failed");
        assert_eq!(opened().finish(0)["status"], "failed");
    }
    #[test]
    fn stream_enforces_session_identity_and_terminal_ordering() {
        assert!(
            Stream::default()
                .observe(&final_event(Value::Null), None)
                .is_err()
        );
        assert!(
            opened()
                .observe(&json!({"type":"session","sessionId":"session-a"}), None)
                .is_err()
        );
        assert!(
            Stream::default()
                .observe(
                    &json!({"type":"session","sessionId":"session-a"}),
                    Some("other")
                )
                .is_err()
        );
        let mut event = final_event(Value::Null);
        event["sessionId"] = "other".into();
        assert!(opened().observe(&event, None).is_err());
        let mut stream = opened();
        stream.observe(&final_event(Value::Null), None).unwrap();
        assert!(
            stream
                .observe(&json!({"type":"text","text":"late"}), None)
                .is_err()
        );
    }
    #[test]
    fn startup_error_needs_no_session_and_never_succeeds() {
        let mut stream = Stream::default();
        stream
            .observe(&json!({"type":"error","message":"profile failed"}), None)
            .unwrap();
        let result = stream.finish(0);
        assert_eq!(result["status"], "failed");
        assert_eq!(result["error"], "profile failed");
    }
}
