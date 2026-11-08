# 安装与配置

简体中文 | [English](INSTALL.en.md)

## 前提

- 从源码安装需要 Rust 1.88+ 和本平台的 C 编译工具链；本项目编译 SQLite。
- 上游 dsh 要求 Node.js **24.9.0+**。本机核对 `@deepseek-ai/dsh@0.2.2-rc.1`，须提供 `dsh-headless`、JSON 事件与 launcher `--patch`。
- Codex CLI 须支持命名权限配置及可用的 OS 沙箱，本机版本为 **0.166.0**。本工具调用 `codex sandbox`，不单独调用或检查系统 bubblewrap；Linux 的用户命名空间等条件须满足 Codex 后端要求。

本项目不捆绑或安装 dsh、Codex CLI。新项目实际验证仅限 Linux；六平台源码和 CI 矩阵不代表 macOS/Windows 或全部架构已通过验收。已安装 dsh 的启动与事件契约见 [dsh-contract.md](docs/dsh-contract.md)。

## 构建与初始化

可选择以下安装渠道：

- **npm**：`npm install -g dsh-delegate-rs`，需要 Node.js 24.9.0+，无需 Rust；通过可选依赖安装匹配系统和架构的二进制，不使用安装脚本。不要禁用 optional dependencies；缺少平台包时用 `npm install -g dsh-delegate-rs --include=optional` 重装。
- **GitHub Releases**：[下载对应平台的归档](https://github.com/JIAFALSEDREAM/dsh-delegate-rs/releases)，对照 `SHA256SUMS` 校验，解压后把二进制放入 PATH。
- **crates.io**：`cargo install dsh-delegate-rs --locked`，需要前述 Rust 和 C 编译工具链。

发布矩阵包含 Linux x64/arm64（musl）、macOS x64/arm64、Windows x64/arm64。以上渠道都不安装 dsh 或 Codex CLI。初始化后配置模型和认证，再执行 `dsh-delegate-rs install` 接入 Codex MCP。

在仓库目录执行：

```sh
cargo install --path . --locked
dsh-delegate-rs init
dsh-delegate-rs doctor
```

`cargo install` 把程序放入 Cargo 的 bin 目录，请确保该目录在 PATH 中。`init` 输出配置文件路径，默认使用系统用户配置目录中的 `dsh-delegate-rs/config.toml`。Linux 默认是 `~/.config/dsh-delegate-rs/config.toml`，受 XDG_CONFIG_HOME 影响。

也可以指定配置文件：

```sh
dsh-delegate-rs init --config /absolute/path/config.toml
dsh-delegate-rs --config /absolute/path/config.toml doctor
```

`init` 不覆盖已有文件，也不生成或复制密钥。`doctor` 输出 JSON，检查配置、环境文件、版本、headless 参数、状态目录和实际沙箱读写边界；失败时退出非零。它需要已安装 dsh，可能在私有临时 `DSH_HOME` 初始化 profile，并通过 `dsh_ptc_runtime` 在外层 Codex 沙箱中运行内层 `read-only` 的 PTC Node 探针，验证读取、写入拒绝和 control-pipe RPC；不调用模型，也不证明 API 连通或认证成功。按下文提供 API 环境后，用 `run` 验证一次真实调查。

## 配置

默认模板：

```toml
codex = "codex"
dsh = "dsh"
dsh_args = []
dsh_patches = []
dsh_plugins = []
dsh_credential_refs = []
# Written to agent-default-model in every managed.yml.
provider = "deepseek-official"
model = "deepseek-flash"
log_bytes = 4000000
profile_bytes = 128000000
timeout = 600
report_chars = 4000 # 兼容旧配置，已忽略
retention_days = 14
max_history_jobs = 1000
# dsh_env_file = "/absolute/path/api.env"
# dsh_credentials_file = "/absolute/path/.credentials.yaml"
# effort = "high"
# state_dir = "/absolute/path/state"
```

命令行参数优先于文件设置。配置覆盖参数放在子命令之前，例如 `dsh-delegate-rs --provider your-provider --model your-model --dsh-env-file /absolute/path/api.env run --cwd /path/to/repo --task '梳理入口'`；`--config` 和 `--timeout` 也可以放在子命令之后。文件中的相对路径以配置文件所在目录为基准；`dsh`、`codex` 的单独程序名从 PATH 查找，指定相对程序路径时写成 `./bin/dsh`。命令行路径以启动目录为基准。不支持自动展开 `~` 或路径中的环境变量，请写完整路径。

默认 `provider = "deepseek-official"`、`model = "deepseek-flash"`，与安装的 dsh-base 一致；没有配置文件、省略 model 或旧快照中为 null 时仍使用默认模型。`--provider String`、`--model`、`--effort` 是本工具的覆盖参数。每次新调查和续聊将路由写入 `managed.yml` 的 `agent-default-model.config`，模型不可用时直接失败。dsh 本身没有 `--model` / `--effort` 参数。

`effort` 为可选非空字符串，必须由实际 provider/模型支持；不同 provider 没有统一等级集合。显式值写入同一 row 的 `reasoningEffort`；省略时由 provider/会话默认决定，同 route 续聊可能保留既有 effort，需要固定时显式配置。

`timeout` 为任务执行期限，范围 1–7200 秒；`report_chars` 和 `--report-chars` 仅为兼容旧调用保留并忽略，不再限制报告长度或委派提示；`retention_days` 范围 1–365；`max_history_jobs` 范围 10–10000。后两项用于手动清理和 worker 保存任务结果后的自动清理。`dsh_args` 放在托管的 dsh 参数之前，例如 `dsh = "node"` 配合 `dsh_args = ["/absolute/path/dsh.js"]`；也可重复传入 `--dsh-arg`。参数中不要放凭据。`log_bytes` 范围 4096–32000000，限制单个 `dsh.log` 及保留的 stderr；总 stdout 协议输出仍有独立的 4,000,000 字节上限；超字节限制会失败，不按字符裁剪报告。`profile_bytes` 范围 1000000–1000000000，默认 128000000 字节，限制每个独立会话目录内的文件总大小，也可在子命令前传 `--profile-bytes`。

默认路由从 `DEEPSEEK_API_KEY` 读取 API 密钥；可在进程启动环境设置，或显式引用已有环境文件：

```sh
dsh-delegate-rs --dsh-env-file /absolute/path/api.env run --cwd /path/to/repo --task '梳理入口'
```

认证优先级为已有环境变量 > `dsh_env_file` > 显式指定的保存凭据引用（包括已导出的空值）。内容不写入任务记录；不要将密钥放进 TOML、argv 或 overlay。本工具不自动导入宿主 `api.env` 或交互界面登录。未知配置项报错，凭据环境值与持久任务路径须为 UTF-8。

复用已有 dsh 认证时，同时设置 `dsh_credentials_file` 和非空 `dsh_credential_refs`。仅支持 version-1 store 的指定字符串 refs，以同名环境变量注入，并注册到日志脱敏器；不写入、复制或迁移原文件。命令行对应 `--dsh-credentials-file` 和可重复的 `--dsh-credential-ref`。参见 [CommandCode 配置示例](README.md#配置与认证)。

`dsh_patches` 为按顺序加载的 provider overlay 路径，命令行可重复传 `--dsh-patch`。`dsh_plugins` 为已有本地插件包目录，命令行可重复传 `--dsh-plugin`；包须有有效的 package.json 名称，通过受管符号链接装入私有 headless profile，再在 overlay 中按包名加载。此方式让 dsh 使用当前安装版本的 peer 依赖。不会安装包，也不复制整个交互 profile。CommandCode overlay 见 [examples/commandcode.yml](examples/commandcode.yml)，可复制到本工具配置目录并修改 `dsh_patches`，以解除对源码目录位置的依赖。

`dsh_args` 可用于 Node wrapper，例如 `dsh = "node"` 配合 `dsh_args = ["/absolute/path/dsh/lib/bin.js"]`；也可重复 `--dsh-arg`。wrapper 路径请用绝对路径。launcher 在首个未知参数后转交 headless，故 `--patch` 必须在 `--json` 等参数前；本工具依次传 `dsh_args`、headless profile、`dsh_patches`、托管 overlay，最后传 JSON/权限参数。自定义 provider 须在 overlay 中配置实际适配器、协议、endpoint 和 API key 环境变量名称；同 row 的 config 整体替换，所需字段须完整。托管 overlay 最后应用，固定 provider/model、PTC per-call 及只读工具裁剪；不要用额外参数替换这些约束。详见 [dsh 契约](docs/dsh-contract.md)。

每个会话使用独立 `DSH_HOME`：`state_dir/conversations/<root_id>`；续聊复用它并以 `--session-id` 恢复。HOME、agents/cache 与 job tmp 的写入均须留在受管可写边界内。不复制宿主会话、插件、GUI 设置或认证；会话、缓存和日志可能包含敏感内容，请勿提交到版本库。

## 注册 Codex MCP

初始化后可以使用安装助手：

```sh
dsh-delegate-rs install
# 只注册 MCP，不添加策略：
dsh-delegate-rs register
# 指定 Codex 配置目录（也用于隔离验收）：
dsh-delegate-rs install --codex-home /absolute/path/codex-home
```

只有用户显式执行 `install` / `register` 才修改 Codex 配置；源码构建、`init`、`doctor` 不执行注册。`install` 先运行无模型调用的 doctor，通过后注册 MCP 并写入受管理的 AGENTS 策略；`register` 只注册 MCP。注册后重新加载 Codex。已有不属于本程序的注册或策略不会被接管。

注册会保存当前有效配置（包括命令行覆盖）的快照，后续修改配置需要重新执行 `register`。安装助手管理配置与 MCP 集成；程序文件由本地 Cargo 源码安装管理。

注册归属同时核对当前二进制路径和本 Codex 目录的配置快照。沿用固定安装路径的升级可以重新注册；要移动二进制或切换安装渠道，请先从原路径执行 `uninstall`，移动后再安装集成。程序不会自动接管来自其它二进制路径的注册。

安装、注册和卸载会互斥本工具的操作，并在关闭任务后重新核对注册归属。Codex 自身的配置写入没有共享锁或条件更新接口；操作期间请勿同时运行其它 `codex mcp add/remove` 或手动编辑同一配置。外部并发写入仍可能互相覆盖。

规则备份和恢复使用文件同步与原子替换；Unix 还同步父目录，按备份、规则、备份删除的顺序持久化。Windows 支持进程中断后的恢复，但不承诺断电时的目录写入持久性。

也可以手动在 Codex 的 `config.toml` 添加：

```toml
[mcp_servers.dsh-delegate-rs]
command = "/absolute/path/dsh-delegate-rs"
args = ["--config", "/absolute/path/config.toml", "mcp"]
```

Windows 路径可以使用 TOML 单引号。使用默认配置时 `args = ["mcp"]` 即可。重启或重新加载 Codex 的 MCP 服务后生效。

提供 `dsh_submit`、`dsh_status`、`dsh_wait`、`dsh_followup`、`dsh_cancel`、`dsh_list`。列表只返回简短状态；详细结果通过 status/wait 查询。调用方必须检查状态，`incomplete` 或失败不能当作完成。助手卸载只删除匹配本程序与配置快照的注册，并校验受管理策略未被编辑，保留其它设置和规则。

## 运行和清理

```sh
dsh-delegate-rs run --cwd /path/to/repo --task '梳理入口，附文件行号'
dsh-delegate-rs submit --cwd /path/to/repo --task '梳理入口，附文件行号'
dsh-delegate-rs status JOB_ID
dsh-delegate-rs wait JOB_ID --seconds 50
dsh-delegate-rs followup JOB_ID '继续核查调用链'
dsh-delegate-rs submit --cwd /path/to/repo --task-file /path/to/task.txt
dsh-delegate-rs followup JOB_ID --task-file /path/to/followup.txt
dsh-delegate-rs report JOB_ID --output /path/to/new-report.txt
dsh-delegate-rs cancel JOB_ID
dsh-delegate-rs list --limit 10
dsh-delegate-rs cleanup --dry-run
dsh-delegate-rs cleanup
```

`--task` 与 `--task-file` 二选一；`--task-file -` 从 stdin 读取 UTF-8。文件最多 96000 字节、任务最多 24000 个字符。状态和等待接口返回完整报告，有报告时 `report_truncated` 为 `false`；列表接口仍只返回简短状态。`report` 读取完整存储结果，导出时不覆盖已有文件。

后台任务由独立 worker 执行，MCP 服务关闭后仍继续；`wait` 超时不取消任务。状态默认在系统用户数据目录中，可用 `state_dir` 配置或 `--state-dir` 指定。

清理按任务最后更新时间判断过期，并保留最近的历史条数；任一条件达到即可选中已结束任务。`--dry-run` 只预览，检查 `selected_ids` 后再执行实际清理。运行中、孤儿任务，以及同一会话仍有活跃或未确认停止任务的历史都会保留；持有所有权锁或仍有沙箱进程的任务也跳过。

实际清理删除本工具的任务数据库记录和日志目录。清理后的任务 ID 不能再查询或追问；同一会话还有其它历史记录时，独立目录仍保留，可以通过未删除的记录追问。删除最后一条历史引用时一并删除对应的受管会话目录，结果通过 `deleted_conversation_ids` 返回；不触及用户的外部 dsh 配置。

同步 `run` 也使用任务记录和 worker，参与相同清理；Ctrl-C 请求取消任务，清理在已保存结果的 worker 中进行，不阻塞提交。同步结果中的 `cleanup_status` 可能仍为 `pending`；最终清理结果可通过 `status JOB_ID` 查询。未纳入任务记录的目录仍保留，需确认无进程使用后自行处理。没有历史记录可确认归属的独立目录也会保留。

清理先将任务标记为 `purging`，再删除日志和可清理的独立会话目录；在此期间不能追问同一会话。文件删除失败会返回失败项并保留任务记录，处理原因后可重试。如果会话目录删除失败，即使已删掉一部分数据也保留 `purging`，阻止恢复损坏的会话。清理进程中断后可再次运行 `cleanup` 完成恢复。过期和条数限制只作用于可安全清理的历史；被锁定或会话仍活跃的记录不占保留条数。

同一状态目录只允许一个实际清理进程；同时启动的第二个进程返回 `cleanup_active` 跳过结果。日志根目录中的 `.cleanup.lock` 会保留以维持进程间锁，请勿在工具运行期间手动删除它。检测到日志或会话目录被替换、无法核验安全删除时，会保留任务记录并报告跳过或错误。

## 沙箱范围与卸载

新任务中的项目与普通宿主文件只读；本工具管理的独立会话目录和任务临时目录可写，并允许联网。当前方案用于保护项目写入，不提供账号、凭据或网络隔离。工作目录与可写目录重叠时拒绝启动；沙箱启动失败时拒绝执行，不回退为无沙箱运行。内层 dsh 保留 `read-only`，外层 Codex OS 沙箱覆盖 dsh 及其所有子进程；本机嵌套沙箱与 PTC 探针已验证兼容。PTC 每次调用新建 Node runtime；模型 text/thinking 在 step 提交后输出，不是实时 token。

Unix worker 与沙箱组长均已退出、但仍有子进程存活且身份无法核实时，取消会报错并阻止追问，需要人工清理；当前没有独立监督进程。

先运行 `dsh-delegate-rs uninstall`，取消并确认本状态目录的任务停止、撤销匹配的 MCP 注册并恢复受管理策略，再用 `cargo uninstall dsh-delegate-rs` 移除本地安装的程序。无法确认任务停止或策略已编辑时，助手拒绝继续删除集成。该操作不会自动删除配置、任务历史或用户 dsh 配置；数据清理由用户单独决定。

任务运行期间每 100 毫秒检查应用日志大小；超限即终止任务，结束后保留的日志会截断并脱敏。独立会话目录在启动前、运行中每轮约 2 秒及停止后检查文件总大小，超出 `profile_bytes` 会终止整个沙箱进程组。扫描不跟随符号链接，无法安全扫描时任务失败。取消、超时或已有执行错误会保留原状态与诊断，并附加结束检查的错误。

每次会话目录扫描有 2 秒时限；仅扫描自身超时会完整重扫一次，并重新计数。显式配置的插件链接仅在路径和目标均匹配时放行，不递归扫描或计入源包大小；其它链接、特殊文件、读取错误或持续超时仍使任务失败。

这些监控不是文件系统硬配额，检查间隔内可能暂时超过限制。`profile_bytes` 只监控受管独立会话目录，不限制其它任务的磁盘总占用。

操作系统不能在期限内确认整个沙箱进程组结束时，任务会失败，tmp/UNVERIFIED_PROCESS 标记会说明原始日志仍保留在私有目录中；此时日志不能视为已脱敏。先确认并停止相关进程，再处理日志和清理。卸载会拒绝无法核验的任务目录。

卸载的取消范围是选中的 state_dir，包括 CLI 任务，即使当前 Codex 配置目录没有 Rust 注册也会停止该任务池。多个 Codex 配置目录共用状态目录时共用任务池和清理策略；需要隔离任务时，请分别配置 state_dir。程序会在关闭检查期间阻止新提交；遇到正在提交或启动的任务时需稍后重试卸载。

## 维护者发布流程

`npm/`、`scripts/release.py` 和发布工具支持六平台二进制、平台 npm 包、校验和与不可变版本核验。发布前应核实真实 repository 元数据、包名与账号权限、registry 认证及目标平台结果。

仓库为 `JIAFALSEDREAM/dsh-delegate-rs`。推送位于默认分支历史中的 `vMAJOR.MINOR.PATCH` 标签会触发 Release workflow：校验版本与仓库、构建和测试六个平台、生成归档及 `SHA256SUMS`，随后发布 GitHub Release、npm 入口及六个平台包、crates.io crate。

npm 使用 Trusted Publishing（GitHub OIDC），需为入口包和六个平台包配置此仓库的 `release.yml` 发布身份；crates.io 同样使用 GitHub OIDC，需为 `dsh-delegate-rs` crate 配置 owner `JIAFALSEDREAM`、仓库 `dsh-delegate-rs`、workflow `release.yml`，Environment 留空，无需 `CARGO_REGISTRY_TOKEN` secret。GitHub Release 使用内置 `GITHUB_TOKEN`。这些发布身份需在对应服务中配置，不写入源码。

npm 首次发布前，包尚不存在，不能配置可信发布。首发时将仓库变量 `NPM_BOOTSTRAP` 设为 `true`，Release 仍构建全部产物并发布 GitHub/crates.io，跳过 npm job；从 Release 下载并校验七个正式 `.tgz`，用已登录的 npm CLI 依次发布平台包和入口包，再通过 `npm trust github 包名 --file release.yml --repo JIAFALSEDREAM/dsh-delegate-rs --allow-publish --yes` 建立可信发布。完成后删除该变量，后续标签自动通过 OIDC 发布 npm。

版本准备只修改本包 Cargo/npm 版本，不改依赖、不生成提交；validate 拒绝仓库元数据不匹配和不在默认分支历史中的稳定 tag。脚本拒绝无关脏文件，已发布产物须校验一致后才复用。各发布渠道不具原子性。

本地验证（不发布）：

```sh
node --test scripts/test-launcher.cjs scripts/test-publish-npm.cjs
python3 -m unittest discover -s scripts -p 'test_*.py' -v
cargo check --locked
```

CI 的普通测试和 native sandbox 检查保留六平台矩阵。真实 headless doctor 仅在 Linux 安装固定 dsh 后运行，不调用模型；其余平台的 dsh 启动/认证需后续实机验证。
