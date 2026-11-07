# dsh-delegate-rs

简体中文 | [English](README.en.md)

[![npm version](https://img.shields.io/npm/v/dsh-delegate-rs?style=flat-square&logo=npm)](https://www.npmjs.com/package/dsh-delegate-rs)
[![crates.io version](https://img.shields.io/crates/v/dsh-delegate-rs?style=flat-square&logo=rust)](https://crates.io/crates/dsh-delegate-rs)
[![GitHub release](https://img.shields.io/github/v/release/JIAFALSEDREAM/dsh-delegate-rs?style=flat-square&logo=github)](https://github.com/JIAFALSEDREAM/dsh-delegate-rs/releases/latest)
[![CI status](https://img.shields.io/github/actions/workflow/status/JIAFALSEDREAM/dsh-delegate-rs/ci.yml?branch=main&style=flat-square&logo=githubactions&label=CI)](https://github.com/JIAFALSEDREAM/dsh-delegate-rs/actions/workflows/ci.yml)
[![Rust 1.88+](https://img.shields.io/badge/Rust-1.88%2B-orange?style=flat-square&logo=rust)](INSTALL.md#前提)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)

把代码库的只读调查交给 DSH CLI（`dsh`），通过 Codex 的操作系统沙箱保护项目文件。支持直接在命令行运行，也支持作为 Codex MCP 服务提交后台任务、查询结果和继续追问。

适合梳理入口与调用链、查找配置、统计文件、比较实现等调查任务。项目调用你已安装并配置好的 dsh，不包含模型服务或 API 协议转换代理。

默认使用 dsh-base 的 **`deepseek-official/deepseek-flash`** 路由；每次调查及续聊通过托管 overlay 固定模型。运行强制使用 `dsh-headless` 与 **PTC per-call Node runtime**。

## 快速开始

### 1. 准备依赖

- **DSH CLI**：提供 `dsh-headless`、JSON 事件及 launcher `--patch`；本机核对版本为 `0.2.2-rc.1`。
- **Codex CLI**：支持命名权限配置，本机验证版本为 `0.166.0`。
- **Codex 沙箱可用**：本工具直接调用 `codex sandbox`，由 Codex 管理平台沙箱后端；`install` 会实际验证读写边界。
- **Node.js 24.9.0+**：上游 dsh 的运行要求；从源码构建本工具另需 Rust 1.88+ 和 C 编译工具链。

依赖安装方法见[安装指南](INSTALL.md#前提)。本工具的安装包不会安装 dsh 或 Codex CLI。

### 2. 安装并接入 Codex

```sh
npm install -g dsh-delegate-rs
dsh-delegate-rs init
dsh-delegate-rs install
```

npm 自动选择当前平台的二进制，无需 Rust。`init` 创建配置后，先按下文设置模型和认证，再执行 `install`。安装助手会检查依赖和沙箱、注册 MCP 并写入受管理的 AGENTS 策略；成功后新开 Codex 会话，即可使用 `dsh_submit` 等工具。只检查运行环境可执行 `doctor`，不注册 MCP。

也可从 [GitHub Releases](https://github.com/JIAFALSEDREAM/dsh-delegate-rs/releases) 下载预编译包，或使用 `cargo install dsh-delegate-rs --locked`。发布流程覆盖 Linux、macOS、Windows 的 x86_64 与 ARM64；实际验证范围见下文。源码构建使用 `cargo install --path . --locked`。详细步骤见[安装指南](INSTALL.md)。

### 3. 验证一次真实调查

```sh
dsh-delegate-rs run --cwd /absolute/path/to/repo --task '梳理任务调度入口和调用链，附文件行号'
```

安装检查和 `doctor` 都不调用模型，但需要已安装 dsh 与可用 Node runtime。这一步会调用模型，用于确认 API 认证和模型可用。dsh 的 `text` / `thinking` 事件在 step 提交时输出，并非实时 token 流。

## 日常使用

### 命令行

`run` 等待调查结束并返回结果。需要后台执行时，使用 `submit`，再用返回的任务 ID 查询或追问：

```sh
dsh-delegate-rs submit --cwd /absolute/path/to/repo --task '查找配置加载位置，附文件行号'
dsh-delegate-rs wait JOB_ID --seconds 50
dsh-delegate-rs followup JOB_ID '继续核查配置覆盖顺序'
```

追问会产生新的任务 ID，并复用原会话；同一会话串行执行，旧 worker 必须确认停止后才能续聊。后台任务由独立 worker 执行，MCP 服务关闭后仍继续；`wait` 等待超时不会取消任务。

长任务说明可以用 `--task-file /absolute/path/task.txt` 代替 `--task`。导出完整报告：

```sh
dsh-delegate-rs report JOB_ID --output /absolute/path/new-report.txt
```

导出不会覆盖已有文件。更多命令见[运行和清理](INSTALL.md#运行和清理)。

### Codex MCP

| 工具 | 用途 |
| --- | --- |
| `dsh_submit` | 提交只读调查，立即返回任务 ID |
| `dsh_status` | 查询任务状态和结果 |
| `dsh_wait` | 等待任务结果；等待超时后任务继续运行 |
| `dsh_followup` | 沿用原会话继续追问，生成新任务 |
| `dsh_cancel` | 请求停止任务的本地进程树 |
| `dsh_list` | 列出近期任务的简短状态，恢复任务 ID |

提交时应给出明确的调查范围和仓库绝对路径。取得结果后必须检查任务状态：失败、未完成或超时的调查不能当作已完成。详细结果通过 `dsh_status` 或 `dsh_wait` 获取。

CLI 和 MCP 的状态、等待接口返回完整报告，`report_truncated` 固定为 `false`（有报告时）；`dsh_list` 继续只列简短状态。委派提示要求结论、证据、不确定性和简洁表达，不设报告字符上限。旧配置 `report_chars` 和参数 `--report-chars` 仅为兼容保留并忽略，不再影响报告或提示；完整报告仍存入数据库并可通过 `report` 导出。总 stdout 协议上限为 4,000,000 字节，`log_bytes` 默认也为 4,000,000 字节；超字节限制会失败，完整报告不保证无限大小。任务超时和只读沙箱继续生效。

每次调查及追问都会由宿主加载 `$CODEX_HOME`（默认 `~/.codex`）的全局指令，以及从文件系统根目录到 `cwd` 的逐级目录指令。同目录优先使用 `AGENTS.override.md`，否则使用 `AGENTS.md`；独立成行的 `@文件路径` 引用会展开（支持相对路径、绝对路径和 `~/`），重复文件及循环引用只读取一次。内容连同来源路径一起注入，子代理无需重新搜索这些规则；进入新的子目录时仍需检查该目录的局部规则。指令总量上限为 64 KiB、32 个文件，读取失败或超限会明确报错。

## 配置与认证

认证环境默认继承，也可以加载已有的环境文件：

```sh
dsh-delegate-rs --dsh-env-file /absolute/path/api.env run --cwd /absolute/path/to/repo --task '查找配置入口'
```

已有环境变量优先于环境文件中的值，环境文件内容不写入任务记录。`--provider`、`--model`、`--effort`、`--dsh`、`--codex`、`--state-dir` 等覆盖参数放在子命令之前。

需要配置文件时，先执行 `dsh-delegate-rs init`，再编辑它输出的文件路径。默认位于系统用户配置目录下；Linux 通常为 `~/.config/dsh-delegate-rs/config.toml`。配置修改后需重新执行 `register`，更新 MCP 使用的配置快照。

默认模型路由与 dsh-base 一致：

```toml
provider = "deepseek-official"
model = "deepseek-flash"
# effort = "high" # 仅使用 provider/模型实际接受的非空值
```

`--provider`、`--model`、`--effort` 是本工具的配置参数。每次调用生成的 `managed.yml` 在 `agent-default-model` row 设置 provider/model 及显式 effort；dsh 自身没有 `--model` 或 `--effort` 参数。省略 effort 时由 provider/会话默认决定；需要固定等级时显式配置，不承诺跨 provider 通用的等级集合。模型不可用时失败，不自动切换。

默认 DeepSeek API 使用 `DEEPSEEK_API_KEY`；可在启动环境设置，或用 `dsh_env_file` 引用已有文件。headless 默认不读取交互界面保存的凭据；本工具支持显式读取既有 dsh version-1 credential store 中指定的字符串引用，在内存中传给子进程，不复制或迁移源文件。

已验证可复用 CommandCode 的 **DeepSeek V4.1 Flash (latest) (CC)**。示例配置如下；将路径替换为实际的绝对路径：

```toml
provider = "commandcode"
model = "deepseek/deepseek-v4.1-flash"
dsh_credentials_file = "/home/you/.dsh/.credentials.yaml"
dsh_credential_refs = ["COMMANDCODE_API_KEY"]
dsh_plugins = ["/home/you/.dsh/profiles/web/node_modules/@mars-sea/dsh-commandcode-provider"]
dsh_patches = ["/absolute/path/dsh-delegate-rs/examples/commandcode.yml"]
```

配置只保存路径和引用名，不填密钥。优先级为已有环境变量 > `dsh_env_file` > 指定凭据引用（已有空值也会覆盖后备来源）。provider 插件通过受管链接加载，overlay 先于强制 PTC/只读配置应用。详细选项见[安装指南](INSTALL.md#配置)。

遇到环境或沙箱问题时，可运行 `dsh-delegate-rs doctor` 检查配置、依赖和实际沙箱读写边界；模型认证仍需通过真实调查验证。

完整配置项、自定义 Codex 目录和手动 MCP 注册方法见[安装指南](INSTALL.md)。自定义 API 若需要协议转换，请单独运行转换服务。

## 沙箱与数据边界

- **只读范围**：项目及普通宿主文件只读。新任务仅允许写入本工具管理的独立会话目录和任务临时目录；工作目录与可写目录重叠时拒绝启动。
- **联网与凭据**：允许联网调用模型。沙箱用于限制文件写入，不提供网络或凭据隔离。
- **启动保障**：内层 dsh 使用 `read-only`，外层 Codex OS 沙箱覆盖整个进程树。本机已通过嵌套沙箱与 PTC 探针；沙箱启动失败时任务直接失败。
- **会话数据**：每个会话拥有独立 `DSH_HOME`，追问复用 `state_dir/conversations/<root_id>`。不复制宿主 GUI 认证、设置或其它产品的登录文件。
- **存储限制**：会话目录默认限制为 128,000,000 字节（`profile_bytes`）。显式配置的外部插件仅核对受管链接及其目标，不递归计入源包大小；其它链接仍拒绝。这是定时监控，超限会终止任务，并非文件系统硬配额。

状态默认保存在系统用户数据目录的 `dsh-delegate-rs` 下，可通过 `--state-dir` 指定。报告、缓存和私有登录文件可能包含敏感数据，请勿提交到版本库。

清理历史前可先预览：

```sh
dsh-delegate-rs cleanup --dry-run
dsh-delegate-rs cleanup
```

清理后的任务不能再查询或追问；删除会话的最后一条历史引用时，也会删除对应的独立会话目录。清理只处理可确认归属的受管数据。清理条件与卸载步骤见[安装指南](INSTALL.md#运行和清理)及[沙箱范围与卸载](INSTALL.md#沙箱范围与卸载)。

## 支持与验证

实际验证范围为 Linux。源码和 CI 配置覆盖 Linux、macOS、Windows 的 x86_64/ARM64 六平台构建矩阵；其它平台仍需实机验收。doctor 已验证 `dsh_ptc_runtime`：在 Codex 0.166.0 外层沙箱与 dsh 内层 `read-only` 下运行 PTC Node，检查读取、禁止写入及 control-pipe RPC；不调用模型，不证明模型认证成功。

另已通过 CommandCode 真实模型验收：只读调查、同会话追问、工具执行期间取消、取消后续接均成功。验证命令与兼容性范围见[验证指南](docs/validation.md)。

在源码仓库运行测试：

```sh
cargo test --locked
cargo test --test sandbox native_sandbox_preserves_fixture -- --ignored
```

## 许可证

[MIT](LICENSE) 许可证覆盖本项目代码；派生自 agy-delegate-rs，保留上游 `Copyright (c) 2026 agy-delegate-rs contributors` 署名。本项目调用外部安装的 Codex CLI，不捆绑其二进制或 bubblewrap。
