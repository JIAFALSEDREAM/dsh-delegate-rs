#![cfg(unix)]
mod common;
use serde_json::{Value, json};

#[test]
fn token_cache_preserves_cumulative_scope() {
    let state = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let cache = json!({"version":7,"record":{"rows":{"tokenUsage":{"seq":42,"ver":2,"val":{"totals":{"uncachedInputTokens":100,"outputTokens":20,"cacheReadTokens":30,"cacheWriteTokens":5}}}}}});
    let script = state.path().join("fixture.sh");
    std::fs::write(&script, format!("cat >/dev/null\nprintf '%s\\n' '{{\"type\":\"session\",\"sessionId\":\"fixture\"}}'\nmkdir -p \"$DSH_HOME/storages/session_projcache/sessions\"\nprintf '%s' '{cache}' > \"$DSH_HOME/storages/session_projcache/sessions/fixture.json\"\nprintf '%s\\n' '{{\"type\":\"final\",\"sessionId\":\"fixture\",\"reason\":{{\"kind\":\"completed\"}},\"text\":\"ok\"}}'\n")).unwrap();
    let output = common::invoke(state.path(), repo.path(), &script, "inspect");
    assert!(output.status.success(), "{output:?}");
    let finished: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(finished["status"], "succeeded");
    assert_eq!(finished["usage"]["totalTokens"], 155);
    assert_eq!(finished["usage"]["uncachedInputTokens"], 100);
    assert_eq!(finished["usage_scope"], "conversation_cumulative");
    assert_eq!(finished["usage_source"], "session_projection_cache");
    assert_eq!(finished["usage_checkpoint_seq"], 42);
}
