# Installation and configuration

[简体中文](INSTALL.md) | English

## Prerequisites

- Source installation requires Rust 1.88+ and a C toolchain for your platform; the project compiles SQLite.
- Upstream dsh requires Node.js **24.9.0+**. The locally checked version is `@deepseek-ai/dsh@0.2.2-rc.1`, providing `dsh-headless`, JSON events, and launcher `--patch`.
- Codex CLI must support named permission profiles and a working OS sandbox. The local version is **0.166.0**. This tool calls `codex sandbox`, without separately invoking or checking system bubblewrap. Linux must satisfy the Codex backend's requirements, including user namespaces where needed.

This project does not bundle or install dsh or Codex CLI. Actual validation of this new project is limited to Linux; the six-platform source/CI matrix does not establish macOS/Windows or all-architecture acceptance. See [dsh-contract.md](docs/dsh-contract.md) for the installed dsh startup and event contract.

## Build and initialize

Choose an installation channel:

- **npm**: `npm install -g dsh-delegate-rs`. Requires Node.js 24.9.0+, without Rust. Optional dependencies select the platform binary; no install script is used. Keep optional dependencies enabled. If the platform package is missing, reinstall with `npm install -g dsh-delegate-rs --include=optional`.
- **GitHub Releases**: [download the archive for your platform](https://github.com/JIAFALSEDREAM/dsh-delegate-rs/releases), verify it against `SHA256SUMS`, extract it, and put the binary on PATH.
- **crates.io**: `cargo install dsh-delegate-rs --locked`, requiring the Rust and C toolchains above.

The release matrix includes Linux x64/arm64 (musl), macOS x64/arm64, and Windows x64/arm64. These channels do not install dsh or Codex CLI. Initialize configuration, set the model and authentication, then run `dsh-delegate-rs install` to connect Codex MCP.

For installation from the repository:

```sh
cargo install --path . --locked
dsh-delegate-rs init
dsh-delegate-rs doctor
```

Ensure Cargo's bin directory is on PATH. `init` prints the configuration path, normally `dsh-delegate-rs/config.toml` under your system's user configuration directory. On Linux this is usually `~/.config/dsh-delegate-rs/config.toml`, subject to `XDG_CONFIG_HOME`.

You can select a configuration file explicitly:

```sh
dsh-delegate-rs init --config /absolute/path/config.toml
dsh-delegate-rs --config /absolute/path/config.toml doctor
```

`init` never overwrites existing files or generates/copies credentials. `doctor` returns JSON and exits nonzero on failed checks of configuration, environment files, versions, headless options, state storage, or actual sandbox boundaries. It requires installed dsh and may initialize a profile in a private temporary `DSH_HOME`, and runs the `dsh_ptc_runtime` probe under the outer Codex sandbox with inner dsh `read-only`, checking PTC Node reads, rejected writes, and control-pipe RPC. It makes no model call and does not prove API connectivity/authentication. Supply API environment values below, then verify a real investigation with `run`.

## Configuration

Default template:

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
report_chars = 4000 # Legacy compatibility setting; ignored
retention_days = 14
max_history_jobs = 1000
# dsh_env_file = "/absolute/path/api.env"
# dsh_credentials_file = "/absolute/path/.credentials.yaml"
# effort = "high"
# state_dir = "/absolute/path/state"
```

Explicit command-line options override file settings. Put configuration overrides before the subcommand, for example `dsh-delegate-rs --provider your-provider --model your-model --dsh-env-file /absolute/path/api.env run --cwd /path/to/repo --task 'Trace entry points'`. `--config` and `--timeout` also work after the subcommand. File-relative paths resolve against the configuration directory. Plain `dsh` and `codex` names are looked up on PATH; write relative executable paths as `./bin/dsh`. Command-line paths resolve against the launch directory. `~` and environment variables in paths are not expanded; use full paths.

Defaults are `provider = "deepseek-official"` and `model = "deepseek-flash"`, matching the installed dsh-base. The default model also applies without a config file, when omitted, or for null in old snapshots. `--provider String`, `--model`, and `--effort` configure this tool. Each investigation/follow-up writes the route to `agent-default-model.config` in `managed.yml`; unavailable models fail without fallback. dsh itself has no `--model` / `--effort` options.

`effort` is an optional nonempty string accepted by the actual provider/model; there is no common effort set across providers. An explicit value becomes `reasoningEffort` in the same row. Omitted effort uses provider/session defaults; same-route follow-ups may retain the saved effort. Set it explicitly to pin it.

Other settings:

| Setting | Default | Purpose and range |
| --- | --- | --- |
| `timeout` | `600` | Task execution deadline, 1–7200 seconds |
| `report_chars` | `4000` | Deprecated and ignored, as is `--report-chars`; no report or prompt limit |
| `retention_days` | `14` | History retention, 1–365 days |
| `max_history_jobs` | `1000` | History count limit, 10–10000 |
| `log_bytes` | `4000000` | Limit for an individual dsh log and retained stderr, 4096–32000000 bytes |
| `profile_bytes` | `128000000` | Total file bytes in each managed conversation directory, 1000000–1000000000 |

Retention settings apply to manual cleanup and automatic cleanup after workers save results. `dsh_args` precede managed dsh options: for example, `dsh = "node"` with `dsh_args = ["/absolute/path/dsh.js"]`. You can also repeat `--dsh-arg`. Keep credentials out of arguments. Total stdout protocol output has a separate 4,000,000-byte limit. Byte-limit violations fail the task; reports are not clipped by character count. `--profile-bytes` can override the conversation limit before the subcommand.

The default route reads its API key from `DEEPSEEK_API_KEY`. Supply it through the launch environment, or explicitly load an existing environment file:

```sh
dsh-delegate-rs --dsh-env-file /absolute/path/api.env run --cwd /path/to/repo --task 'Trace entry points'
```

Authentication precedence is inherited environment > `dsh_env_file` > explicitly selected saved credential refs, including exported empty values. Contents are not written to task records; keep secrets out of TOML, argv, and overlays. The tool does not automatically import interactive login or the host's `api.env`. Unknown config keys are rejected; credential environment values and persisted task paths must be UTF-8.

To reuse dsh authentication, set both `dsh_credentials_file` and a nonempty `dsh_credential_refs`. Only selected string refs from a version-1 store are supported. They become same-named environment variables and are registered with the log redactor; the source file is never written, copied, or migrated. CLI equivalents are `--dsh-credentials-file` and repeatable `--dsh-credential-ref`. See the [CommandCode example](README.en.md#configuration-and-authentication).

`dsh_patches` lists provider overlays in application order; use repeatable `--dsh-patch` on the CLI. `dsh_plugins` lists existing local package directories; use repeatable `--dsh-plugin`. Each package needs a valid package.json name. Managed symlinks expose packages to the private headless profile; load them by package name in overlays so dsh resolves peers from its current installation. No packages are installed and no interactive profile is copied. You can copy [examples/commandcode.yml](examples/commandcode.yml) into the tool's configuration directory and update `dsh_patches` to avoid depending on the source tree location.

`dsh_args` supports Node wrappers, for example `dsh = "node"` with `dsh_args = ["/absolute/path/dsh/lib/bin.js"]`; you can repeat `--dsh-arg`. Use absolute wrapper paths. The launcher forwards arguments after the first unknown option, so `--patch` must precede `--json` and other headless arguments. This tool passes `dsh_args`, the headless profile, `dsh_patches`, the managed overlay, then JSON/permission arguments. Custom providers require an overlay defining the actual adapter, protocol, endpoint, and API-key environment variable name. A row's config is replaced as a whole; include all needed fields. The managed overlay is applied last to pin provider/model, PTC per-call, and read-only tool configuration. Do not replace these constraints with extra arguments. See the [dsh contract](docs/dsh-contract.md).

Each conversation has a private `DSH_HOME` at `state_dir/conversations/<root_id>`. Follow-ups reuse it with `--session-id`. HOME, agents/cache, and job temporary writes must remain within managed writable boundaries. Host sessions, plugins, GUI settings, and credentials are not copied. Keep potentially sensitive sessions, caches, and logs out of version control.

## Register with Codex MCP

After initialization:

```sh
dsh-delegate-rs install
# Register MCP without adding delegation rules:
dsh-delegate-rs register
# Select a Codex configuration directory, also useful for isolated verification:
dsh-delegate-rs install --codex-home /absolute/path/codex-home
```

Only an explicit `install` / `register` invocation changes Codex configuration; source builds, `init`, and `doctor` do not register MCP. `install` runs doctor without model calls, then registers MCP and adds managed AGENTS rules. `register` only registers MCP. Reload Codex afterward. Registrations or policies owned by another program are not taken over.

Registration saves a snapshot of the effective configuration, including command-line overrides. Run `register` again after changes. The installer manages configuration and MCP integration; manage the executable through local Cargo source installation.

Ownership checks verify both the current executable path and the snapshot for the selected Codex directory. Upgrades retaining the same path can register again. Before moving the executable or changing installation channels, run `uninstall` from the original path. The tool does not take over registrations owned by another executable path.

Install, register, and uninstall operations are mutually exclusive and recheck ownership after stopping tasks. Codex configuration writes have no shared lock or conditional update interface. Avoid simultaneous `codex mcp add/remove` calls or manual edits to the same file; external concurrent writes can still overwrite each other.

Policy backup and restoration use file synchronization and atomic replacement. Unix also synchronizes the parent directory, persisting the backup, policy, and backup removal in order. Windows supports recovery after process interruption, but does not guarantee directory durability after power loss.

For manual registration, add this to Codex's `config.toml`:

```toml
[mcp_servers.dsh-delegate-rs]
command = "/absolute/path/dsh-delegate-rs"
args = ["--config", "/absolute/path/config.toml", "mcp"]
```

TOML single-quoted strings are convenient for Windows paths. With default configuration, use `args = ["mcp"]`. Restart or reload Codex's MCP server to apply it.

Available tools are `dsh_submit`, `dsh_status`, `dsh_wait`, `dsh_followup`, `dsh_cancel`, and `dsh_list`. Lists return brief status; use status/wait for details. Check task status and do not treat `incomplete` or failed tasks as completed. Uninstallation removes only matching registrations and verifies that managed rules have not been edited, preserving other settings and rules.

## Run and clean up

```sh
dsh-delegate-rs run --cwd /path/to/repo --task 'Trace entry points with file and line references'
dsh-delegate-rs submit --cwd /path/to/repo --task 'Trace entry points with file and line references'
dsh-delegate-rs status JOB_ID
dsh-delegate-rs wait JOB_ID --seconds 50
dsh-delegate-rs followup JOB_ID 'Continue checking the call chain'
dsh-delegate-rs submit --cwd /path/to/repo --task-file /path/to/task.txt
dsh-delegate-rs followup JOB_ID --task-file /path/to/followup.txt
dsh-delegate-rs report JOB_ID --output /path/to/new-report.txt
dsh-delegate-rs cancel JOB_ID
dsh-delegate-rs list --limit 10
dsh-delegate-rs cleanup --dry-run
dsh-delegate-rs cleanup
```

Choose either `--task` or `--task-file`. `--task-file -` reads UTF-8 from stdin. Files are limited to 96000 bytes and tasks to 24000 characters. Status and wait interfaces return complete reports with `report_truncated: false` whenever a report is present; list interfaces still return compact status entries. `report` returns the complete stored result; exports preserve existing files.

Independent background workers continue after the MCP server closes. A wait timeout does not cancel work. State defaults to the system user data directory, configurable with `state_dir` or `--state-dir`.

Cleanup selects finished tasks by last update time or by the retained history count. Preview `selected_ids` with `--dry-run`. Running tasks, orphaned tasks, and history belonging to conversations with active or unverified tasks are retained. Tasks with an ownership lock or a surviving sandbox process are also skipped.

Cleanup removes task database records and log directories. Deleted job IDs cannot be queried or followed up. A conversation directory remains while other history references exist, allowing follow-ups through those records. Removing the last reference deletes the managed directory and returns its ID in `deleted_conversation_ids`. External user dsh configuration is preserved.

Synchronous `run` also uses task records and workers, with the same cleanup rules. Ctrl-C requests cancellation. Cleanup runs in workers after saving results, without blocking submission. A synchronous result may show `cleanup_status: pending`; query `status JOB_ID` for the final outcome. Unrecorded directories are retained; handle them manually only after confirming no process uses them. Managed directories without history proving ownership are also retained.

Cleanup marks tasks `purging` before deleting logs and eligible conversation directories. Follow-ups in the same conversation are blocked during removal. Failed file deletion preserves records and returns failure details for retry. Partially deleted conversation directories remain `purging` to prevent resuming damaged conversations. Run `cleanup` again to recover after interruption. Retention limits count only history that is safe to remove; locked records and active conversations do not consume the retained count.

Only one actual cleanup process may run per state directory. A second process returns `cleanup_active` and skips removal. The log directory's `.cleanup.lock` is retained for process locking; do not delete it while the tool is running. Replaced directories or unverifiable deletion boundaries cause records to be retained with a skip/error report.

## Sandbox scope and uninstall

New tasks can read project and ordinary host files, write managed conversation and task temporary directories, and access the network. The sandbox protects project writes; it does not isolate accounts, credentials, or networks. Overlapping working/writable directories are rejected. Sandbox startup failure stops execution without an unsandboxed fallback. Inner dsh retains `read-only`; the outer Codex OS sandbox covers dsh and all descendants. Local nested-sandbox/PTC probes have verified compatibility. PTC starts a fresh Node runtime per call; model text/thinking appear after step commitment, not as live tokens.

On Unix, cancellation fails and blocks follow-ups if the worker and sandbox group leader have exited but surviving child processes cannot be identified reliably. Manual process cleanup is required; there is currently no separate supervisor.

First run `dsh-delegate-rs uninstall` to stop and verify tasks in the selected state, remove matching MCP registration, and restore managed rules. Then remove the locally installed executable with `cargo uninstall dsh-delegate-rs`. If task termination cannot be verified or managed rules were edited, integration removal stops. Configuration, history, and dsh settings are retained; data cleanup is a separate decision.

Application log sizes are checked every 100 milliseconds. Exceeding the limit terminates the task; retained logs are truncated and redacted afterward. Managed conversation file sizes are checked before startup, roughly every 2 seconds during execution, and after termination. Exceeding `profile_bytes` terminates the entire sandbox process group. Scans do not follow symbolic links and fail tasks if a safe scan is impossible. Cancellation, timeout, and existing execution failures retain their original status/diagnostics, with final scan errors added separately.

Each conversation scan has a 2-second deadline. Only a scan's own deadline triggers one complete retry with fresh counters. Explicit plugin links are allowed only when both path and target match; source package sizes are not traversed or counted. Persistent timeouts, read errors, other links, and special files still fail the task.

Monitoring is not a filesystem quota; limits can be exceeded between checks. `profile_bytes` covers managed conversation directories rather than total disk usage across tasks.

If the operating system cannot confirm termination of the entire sandbox process group within the deadline, the task fails. A `tmp/UNVERIFIED_PROCESS` marker indicates that original logs remain in the private directory and must not be considered redacted. Stop and verify related processes before handling logs or cleanup. Uninstall rejects task directories it cannot verify.

Uninstall cancels tasks in the selected `state_dir`, including CLI tasks, even if the current Codex directory has no Rust MCP registration. Codex directories sharing state also share the task pool and cleanup policy; use separate state directories for isolation. New submissions are blocked during shutdown checks. Retry uninstall later if a task is being submitted or started.

## Maintainer release workflow

`npm/`, `scripts/release.py`, and publication tooling support six-platform binary/npm packaging, checksums, and immutable-version reconciliation. Before publishing, verify real repository metadata, package/account permissions, registry authentication, and results on each target platform.

The repository is `JIAFALSEDREAM/dsh-delegate-rs`. Pushing a `vMAJOR.MINOR.PATCH` tag contained in the default branch's history triggers Release: validate versions and repository metadata, build and test six platforms, generate archives and `SHA256SUMS`, then publish GitHub Release, the npm entry and six platform packages, and the crates.io crate.

npm uses Trusted Publishing (GitHub OIDC); configure the entry and six platform packages to trust this repository's `release.yml`. crates.io uses the repository secret `CARGO_REGISTRY_TOKEN`. GitHub Release uses the built-in `GITHUB_TOKEN`. Configure these identities in the relevant services; keep credentials out of source.

Version preparation changes only this package's Cargo/npm versions, without dependency changes or commits. Validation rejects mismatched repository metadata and stable tags outside default-branch history. Scripts reject unrelated dirty files; published artifacts are reused only after integrity checks. Publication across channels is not atomic.

Local checks without publication:

```sh
node --test scripts/test-launcher.cjs scripts/test-publish-npm.cjs
python3 -m unittest discover -s scripts -p 'test_*.py' -v
cargo check --locked
```

CI retains the six-platform matrix for ordinary tests and native sandbox checks. Real headless doctor runs only on Linux with pinned dsh installed and makes no model call. Other platforms still need actual dsh startup/authentication validation.
