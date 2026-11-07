# dsh-headless 集成契约

适用于 `@deepseek-ai/dsh@0.2.2-rc.1`。上游要求 Node.js 24.9.0+。升级 dsh 后需重新检查参数、overlay row ID、协议字段和运行时探针。

## 启动与配置顺序

本工具以调查目录为 cwd，使用 stdin 传入 UTF-8 任务文本，启动参数如下：

```text
dsh [dsh_args] --profile headless
    [--patch provider.yml ...] --patch managed.yml
    --json --permission-mode read-only --approval-policy never
    [--session-id session-<uuid>] -
```

`dsh_args` 用于 Node wrapper 等启动前参数；`dsh_patches` 按配置顺序传入 provider overlays。最后的 `managed.yml` 由程序逐次生成，固定 PTC、模型路由和只读工具配置。launcher 在第一个未知参数处把剩余参数转交 headless，因此 `--profile`、`--patch` 必须位于 `--json` 等 headless 参数之前。

Overlay 为顶层 YAML 数组，支持按 `id` 修改 row 和通过 `insert` 插入插件。row 的 `config` 整体替换，不做递归合并。未知 row ID 可能只产生告警，不能仅以进程启动成功证明裁剪有效。

实现见 `src/runner.rs` 和 `src/harness.rs`。

## PTC 与工具范围

受管 overlay 关闭 `tool-executor-native`，开启 `tool-executor-ptc`，并将 `ptc-runtime.config.executionMode` 固定为 `per-call`。每次 `run_code` 调用使用新的 Node runtime，不保留 JavaScript 内存状态。

模型通过 `run_code` 调用保留的文件读取、搜索、前台 shell 等工具。文件写入、编辑、applyPatch、后台 bash、子代理、workflow、goal/plan、插件管理和配置编辑能力被关闭。具体 row ID 以 `src/harness.rs` 为准；新增 bundle 或第三方插件后须复核其能力。

PTC Node 能使用 Node API，因此工具裁剪不能替代操作系统权限控制。整个 dsh 进程树必须位于外层 Codex OS 只读沙箱内，内层 dsh 同时使用 `read-only`。任一沙箱启动失败即停止，不回退为无沙箱运行。

## 私有会话与写入边界

每个根任务拥有独立的 `state_dir/conversations/<root_id>`，作为其 `DSH_HOME`。追问复用该目录；每次调用的 HOME、agents/cache 与临时写入均指向受管目录。项目和普通宿主文件只读；允许写入的受管会话和临时目录不得与调查目录重叠。

dsh 首次启动可能创建 headless profile、包清单和 Cordis 配置；运行期间会保存 session、storage、attachment、cache 及启动日志。这些是运行状态，不属于项目文件修改。会话、缓存和报告可能含调查内容，不应提交到版本库。

会话目录通过 `profile_bytes` 定期计量。扫描不跟随符号链接；仅允许 `dsh_plugins` 声明且路径和目标完全匹配的受管插件链接，外部包大小不计入会话限额。其它链接或特殊文件使任务失败。清理仅删除可确认归属的任务及会话数据，不删除外部插件源包。

实现见 `src/config.rs`、`src/profiles.rs` 和 `src/tree.rs`。沙箱限制写入，不隔离网络或凭据读取。

## 模型与认证

每次调用把 `provider`、`model` 和显式 `effort` 写入 `agent-default-model.config`，对应字段为 `provider`、`model`、`reasoningEffort`。dsh-headless 本身没有 `--model` 或 `--effort` 参数。模型不可用时直接失败，不自动切换 provider。

`effort` 是 provider/模型接受的非空字符串，没有跨 provider 的统一枚举。省略时由 provider 或既有会话决定；同路由追问可能保留此前 effort，需固定时应显式设置。

认证来源优先级为已有环境变量 > `dsh_env_file` > 指定保存凭据引用；已有空值同样覆盖后备来源。`dsh_credentials_file` 与非空 `dsh_credential_refs` 必须同时指定。凭据解析仅支持 version-1 store 中选定的非空字符串 refs，不迁移、复制或写入源文件，解析错误不回显源内容。

选定 refs 在内存中变为同名环境变量，并加入日志脱敏器；配置快照仅记录路径与引用名。该机制不自动导入交互界面登录，也不启用 dsh 的 credential 管理服务。实现见 `src/credentials.rs` 和 `src/redaction.rs`。

### 可选第三方 provider

`dsh_plugins` 显式列出已有本地插件包目录。本工具校验 package.json 中的包名，通过受管链接将包挂载到私有 headless profile，overlay 通过包名加载，以便 dsh 使用当前安装版本的 peer 依赖。直接导入其它 profile 的绝对入口可能加载旧 peer，应避免这种配置。

CommandCode 示例见 [commandcode.yml](../examples/commandcode.yml)，路由与认证配置见 [README](../README.md#配置与认证)。它是可选 provider，不是内置依赖或默认路由。插件、API endpoint 和账号由使用者提供；本工具不安装或打包第三方插件。

## NDJSON 协议与恢复

headless 的 stdout 是逐行 JSON，没有外层 `event`/`data` 包装：

```json
{"type":"session","sessionId":"session-<uuid>","cwd":"/absolute/workspace"}
{"type":"final","sessionId":"session-<uuid>","reason":{"kind":"completed"},"text":"调查报告"}
```

成功要求进程退出码为 0、最终 `reason.kind` 为 `completed`，且报告非空。报告来自 `final.text`。`aborted`、`blocked`、`error`、`max-tokens`、`interrupted` 等原因不得归类为成功；session/final 的顺序、重复和身份一致性由 `src/protocol.rs` 校验。

`text`、`thinking` 在 step 提交时输出，不是实时 token 流。`tool_call`、`tool_result` 是工具事件，单个工具返回 completed 不保证内部 shell 退出码为零。非最终事件可能被 dsh 截断，读取器允许工具细节缺失；最终报告不套用工具事件的行长限制。

启动错误可能只有 stderr，或输出 `{"type":"error","message":"..."}` 而没有 final。supervisor 同时保留脱敏后的 NDJSON、stderr 与退出状态，并执行总协议字节上限和日志限额。

首次调用不传 `--session-id`，由 dsh 创建 session。追问必须复用原 `DSH_HOME`、cwd 与已保存 session ID；该参数用于恢复已有会话，不能指定任意新 ID。同会话串行运行，旧 worker 和沙箱进程必须确认停止后才能追问。实现见 `src/jobs.rs`。

## 兼容性验证

`doctor` 检查 launcher 参数、私有 HOME、外层沙箱读写边界，并执行真实 PTC Node 程序，验证读取、拒绝写入及 control-pipe RPC。它不调用模型，不能证明远端认证有效。

模型认证和续聊需使用真实调查单独验收。测试命令及已验证范围见 [validation.md](validation.md)。上游契约可从安装包的以下相对位置核对，无需依赖特定用户的安装目录：

| 包 | 参考文件 |
| --- | --- |
| `@deepseek-ai/dsh` | `lib/bin.js`、`package.json` |
| `@deepseek-ai/dsh-headless` | `README.md`、`lib/startup.js`、`lib/index.js` |
| `@deepseek-ai/dsh-base` | `cordis.patch.yml` |
| `@deepseek-ai/dsh-tool-executor-ptc` | `README.md` |
| `@deepseek-ai/dsh-ptc-runtime-node` | `lib/index.js` |
| `@deepseek-ai/dsh-credentials-local` | `README.md`、`lib/index.js` |
