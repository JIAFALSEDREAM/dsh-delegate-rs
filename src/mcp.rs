use crate::jobs::Jobs;
use anyhow::{Context, Result};
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt, model::*, service::RequestContext};
use serde_json::{Value, json};
use std::path::Path;

fn tools() -> Vec<Tool> {
    let string = json!({"type":"string","minLength":1});
    let checks = json!({"type":"array","description":"Required investigation coverage. Each item needs evidence or a reason it is not applicable; missing items make a completed turn incomplete.","maxItems":32,"items":{"type":"string","minLength":1,"maxLength":500}});
    let budget = json!({"type":"integer","description":"Wall-clock seconds for this job. Omit to use the configured timeout; follow-ups inherit the prior job's budget.","minimum":1,"maximum":7200});
    let definitions = [
        (
            "dsh_submit",
            "Start a read-only investigation. Returns immediately; check status before using its report.",
            json!({"cwd":string,"task":{"type":"string","minLength":1,"maxLength":24000},"timeout":budget,"checks":checks}),
            vec!["cwd", "task"],
        ),
        (
            "dsh_status",
            "Read task status, conversation ID and final report. Failed or incomplete tasks are not completed investigations.",
            json!({"job_id":string}),
            vec!["job_id"],
        ),
        (
            "dsh_wait",
            "Wait up to 55 seconds. A wait timeout leaves the task running.",
            json!({"job_id":string,"seconds":{"type":"integer","minimum":0,"maximum":55,"default":50}}),
            vec!["job_id"],
        ),
        (
            "dsh_followup",
            "Continue a stopped task in its original dsh conversation; creates a new job.",
            json!({"job_id":string,"message":{"type":"string","minLength":1,"maxLength":24000},"timeout":budget,"checks":checks}),
            vec!["job_id", "message"],
        ),
        (
            "dsh_cancel",
            "Request cancellation of a task and its owned sandbox process tree.",
            json!({"job_id":string}),
            vec!["job_id"],
        ),
        (
            "dsh_list",
            "Recover recent task IDs after reconnecting.",
            json!({"limit":{"type":"integer","minimum":1,"maximum":20,"default":10}}),
            vec![],
        ),
    ];
    definitions.into_iter().map(|(name,description,properties,required)| {
        Tool::new(name,description,json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}).as_object().unwrap().clone()).with_annotations(ToolAnnotations::from_raw(None,
            Some(matches!(name, "dsh_status" | "dsh_wait" | "dsh_list")),
            Some(name == "dsh_cancel"), None,
            Some(matches!(name, "dsh_submit" | "dsh_followup"))))
    }).collect()
}
fn text<'a>(args: &'a JsonObject, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("Missing string field: {key}"))
}
fn number(args: &JsonObject, key: &str, default: u64) -> Result<u64> {
    match args.get(key) {
        None => Ok(default),
        Some(value) => value
            .as_u64()
            .with_context(|| format!("{key} must be a nonnegative integer")),
    }
}

fn validate(name: &str, args: &JsonObject) -> Result<()> {
    let tool = tools()
        .into_iter()
        .find(|t| t.name == name)
        .context("Unknown tool")?;
    for key in tool.input_schema["required"].as_array().unwrap() {
        anyhow::ensure!(
            args.contains_key(key.as_str().unwrap()),
            "Missing required field: {key}"
        );
    }
    for (key, value) in args {
        let property = tool.input_schema["properties"]
            .get(key)
            .context("Unknown argument")?;
        if property["type"] == "string" {
            let text = value.as_str().context("Expected string argument")?;
            anyhow::ensure!(
                !text.is_empty()
                    && text.chars().count()
                        <= property["maxLength"].as_u64().unwrap_or(u64::MAX) as usize,
                "String argument outside allowed length"
            );
        } else if property["type"] == "array" {
            let items = value.as_array().context("Expected string array")?;
            anyhow::ensure!(
                items.len() <= 32
                    && items.iter().all(|v| v
                        .as_str()
                        .is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= 500)),
                "Provide at most 32 checks, each containing 1–500 characters"
            );
        } else {
            let n = value
                .as_u64()
                .context("Expected nonnegative integer argument")?;
            anyhow::ensure!(
                n >= property["minimum"].as_u64().unwrap_or(0)
                    && n <= property["maximum"].as_u64().unwrap_or(u64::MAX),
                "Integer argument outside allowed range"
            );
        }
    }
    Ok(())
}

impl ServerHandler for Jobs {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
            Implementation::new("dsh-delegate-rs", env!("CARGO_PKG_VERSION")),
        ).with_instructions("Delegate bounded, read-only repository investigations to DSH. Submit returns immediately; do independent work, then wait for the report. Follow up using a job ID. Report failure or incomplete status instead of assuming success.")
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        tools().into_iter().find(|t| t.name == name)
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: tools(),
            ..Default::default()
        })
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = request.arguments.unwrap_or_default();
        validate(&request.name, &args)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
        let name = request.name.into_owned();
        let listing = name == "dsh_list";
        let result: Result<Value> = if name == "dsh_wait" {
            self.wait(
                text(&args, "job_id").unwrap(),
                number(&args, "seconds", 50).unwrap(),
            )
            .await
        } else {
            let jobs = self.clone();
            tokio::task::spawn_blocking(move || match name.as_str() {
                "dsh_submit" => jobs.submit_with_options(
                    text(&args, "task")?,
                    Path::new(text(&args, "cwd")?),
                    None,
                    &serde_json::from_value(Value::Object(args.clone()))?,
                ),
                "dsh_status" => jobs.get(text(&args, "job_id")?),
                "dsh_followup" => jobs.followup_with_options(
                    text(&args, "job_id")?,
                    text(&args, "message")?,
                    &serde_json::from_value(Value::Object(args.clone()))?,
                ),
                "dsh_cancel" => jobs.cancel(text(&args, "job_id")?),
                "dsh_list" => jobs.list(number(&args, "limit", 10)?),
                _ => anyhow::bail!("Unknown tool"),
            })
            .await
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?
        };
        Ok(match result {
            Ok(value) if listing => {
                let mut result =
                    CallToolResult::success(vec![ContentBlock::text(value.to_string())]);
                result.structured_content = Some(json!({"result":value}));
                result.into()
            }
            Ok(value) => CallToolResult::structured(value).into(),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]).into(),
        })
    }
}
pub async fn serve(jobs: Jobs) -> Result<()> {
    jobs.serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}
