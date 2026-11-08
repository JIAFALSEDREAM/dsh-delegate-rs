# dsh-delegate-rs

[简体中文](README.md) | English

[![npm version](https://img.shields.io/npm/v/dsh-delegate-rs?style=flat-square&logo=npm)](https://www.npmjs.com/package/dsh-delegate-rs)
[![crates.io version](https://img.shields.io/crates/v/dsh-delegate-rs?style=flat-square&logo=rust)](https://crates.io/crates/dsh-delegate-rs)
[![GitHub release](https://img.shields.io/github/v/release/JIAFALSEDREAM/dsh-delegate-rs?style=flat-square&logo=github)](https://github.com/JIAFALSEDREAM/dsh-delegate-rs/releases/latest)
[![CI status](https://img.shields.io/github/actions/workflow/status/JIAFALSEDREAM/dsh-delegate-rs/ci.yml?branch=main&style=flat-square&logo=githubactions&label=CI)](https://github.com/JIAFALSEDREAM/dsh-delegate-rs/actions/workflows/ci.yml)
[![Rust 1.88+](https://img.shields.io/badge/Rust-1.88%2B-orange?style=flat-square&logo=rust)](INSTALL.en.md#prerequisites)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)

Delegate read-only repository investigations to DSH CLI (`dsh`), with project files protected by the Codex operating system sandbox. Run investigations from the command line, or use the Codex MCP server to submit background tasks, retrieve results, and ask follow-up questions.

Useful for tracing entry points and call chains, locating configuration, counting files, and comparing implementations. The tool calls your existing dsh installation. Model hosting and API protocol conversion are managed separately.

The default route matches dsh-base: **`deepseek-official/deepseek-flash`**. A managed overlay pins the model for every investigation and follow-up. Execution requires `dsh-headless` and the **PTC per-call Node runtime**.

## Quick start

### 1. Prepare dependencies

- **DSH CLI**: provides `dsh-headless`, JSON events, and launcher `--patch`; locally checked version: `0.2.2-rc.1`.
- **Codex CLI**: support for named permission profiles. The locally verified version is `0.166.0`.
- **A working Codex sandbox**: the tool calls `codex sandbox`; Codex manages the platform backend. `install` checks the actual read/write boundaries.
- **Node.js 24.9.0+**: required by upstream dsh. Building this tool also requires Rust 1.88+ and a C toolchain.

See the [installation guide](INSTALL.en.md#prerequisites) for prerequisites. This package does not install dsh or Codex CLI.

### 2. Install and connect to Codex

```sh
npm install -g dsh-delegate-rs
dsh-delegate-rs init
dsh-delegate-rs install
```

npm selects the binary for your platform; Rust is not required. After `init` creates configuration, set the model and authentication below before running `install`. The installer checks dependencies and the sandbox, registers MCP, and adds managed AGENTS rules. Start a new Codex session afterward to use `dsh_submit` and the other tools. Use `doctor` to check the runtime without registering MCP.

You can also download a binary from [GitHub Releases](https://github.com/JIAFALSEDREAM/dsh-delegate-rs/releases), or run `cargo install dsh-delegate-rs --locked`. The release workflow targets x86_64 and ARM64 on Linux, macOS, and Windows; see the actual validation scope below. Build from source with `cargo install --path . --locked`. See the [installation guide](INSTALL.en.md) for details.

### 3. Verify a real investigation

```sh
dsh-delegate-rs run --cwd /absolute/path/to/repo --task 'Trace task scheduling entry points and call chains, with file and line references'
```

Installation checks and `doctor` require dsh but do not call a model. This step calls the model to verify API authentication and model access. dsh emits `text` / `thinking` when a step is committed, rather than streaming live tokens.

## Everyday use

### Command line

`run` waits for completion and returns the result. For background tasks, use `submit`, then query or follow up with the returned job ID:

```sh
dsh-delegate-rs submit --cwd /absolute/path/to/repo --task 'Locate configuration loading, with file and line references'
dsh-delegate-rs wait JOB_ID --seconds 50
dsh-delegate-rs followup JOB_ID 'Check the configuration override order'
```

A follow-up creates a new job ID and reuses the original conversation. Turns in one conversation are serial; the previous worker must be confirmed stopped before resuming. Independent workers continue after the MCP server closes. A `wait` timeout does not cancel the task.

For longer instructions, use `--task-file /absolute/path/task.txt` instead of `--task`. Export a complete report with:

```sh
dsh-delegate-rs report JOB_ID --output /absolute/path/new-report.txt
```

Exports never overwrite an existing file. See [running and cleanup](INSTALL.en.md#run-and-clean-up) for more commands.

### Codex MCP

| Tool | Purpose |
| --- | --- |
| `dsh_submit` | Submit a read-only investigation and immediately return a job ID |
| `dsh_status` | Query task status and results |
| `dsh_wait` | Wait for a result; the task continues if the wait times out |
| `dsh_followup` | Continue the original conversation with a new job |
| `dsh_cancel` | Request termination of the task's local process tree |
| `dsh_list` | List recent tasks and recover job IDs |

Provide a precise investigation scope and an absolute repository path. Always check the returned status: failed, incomplete, or timed-out investigations must not be treated as completed. Retrieve detailed results with `dsh_status` or `dsh_wait`.

`dsh_submit` / `dsh_followup` accept optional `timeout` (1–7200 seconds) and `checks` (up to 32 items, 1–500 characters each). CLI `run` / `submit` / `followup` use `--task-timeout 120 --check 'Verify outside receipt'`; repeat `--check` for more items. Omitted budgets use the existing configuration; follow-ups inherit the previous budget but specify their own checklist. Each check needs evidence or a reason it is not applicable; omissions yield `incomplete`. This checks reported investigation coverage, not evidence correctness.

Committed assistant text checkpoints retain the latest 8000 characters, excluding thinking. If timeout, cancellation or failure leaves no final report, the report interface returns the checkpoint with `report_partial: true` while preserving the original terminal status. No checkpoint exists before the model emits text. Prompts reserve the final quarter of the budget for reporting; the hard deadline remains enforced.

When a compatible session projection cache exists, terminal results include `usage` (uncached input, output, cache reads/writes and `totalTokens`), `usage_scope: conversation_cumulative`, `usage_source: session_projection_cache` and `usage_checkpoint_seq`. This cumulative snapshot may lag the session; do not sum it across follow-ups. Missing or incompatible caches do not produce zero counters.

CLI and MCP status/wait interfaces return complete reports with `report_truncated: false` whenever a report is present. `dsh_list` continues to return compact status entries. Delegated prompts request conclusions, evidence, uncertainty, and concise writing without a report character limit. The legacy `report_chars` setting and `--report-chars` option are accepted and ignored; they no longer affect reports or prompts. Complete reports remain stored in the database and available through `report` exports. Total stdout protocol output is limited to 4,000,000 bytes; `log_bytes` also defaults to 4,000,000 bytes. Byte-limit violations fail the task, so complete reports are not unlimited. Task deadlines and the read-only sandbox remain in effect.

For every investigation and follow-up, the host loads global instructions from `$CODEX_HOME` (default `~/.codex`) and directory instructions from the filesystem root through `cwd`. Each directory prefers `AGENTS.override.md` over `AGENTS.md`. Standalone `@file` references are expanded, supporting relative, absolute and `~/` paths; duplicate files and cycles are loaded once. The contents and source paths are injected so the investigator need not rediscover these rules. Additional local rules still apply when entering new subdirectories. Instructions are limited to 64 KiB and 32 files; read failures and limit violations produce an explicit error.

## Configuration and authentication

Authentication environment variables are inherited by default. You can also load an existing environment file:

```sh
dsh-delegate-rs --dsh-env-file /absolute/path/api.env run --cwd /absolute/path/to/repo --task 'Locate the configuration entry point'
```

Existing environment variables take precedence over values in the file. Environment file contents are not written to task records. Place override options such as `--provider`, `--model`, `--effort`, `--dsh`, `--codex`, and `--state-dir` before the subcommand.

To create a configuration file, run `dsh-delegate-rs init` and edit the path it prints. The default is in your system's user configuration directory, usually `~/.config/dsh-delegate-rs/config.toml` on Linux. Run `register` again after configuration changes to update the snapshot used by MCP.

The default model route matches dsh-base:

```toml
provider = "deepseek-official"
model = "deepseek-flash"
# effort = "high" # Use only a nonempty value accepted by the provider/model
```

`--provider`, `--model`, and `--effort` configure this tool. Each invocation writes provider/model and any explicit effort to the `agent-default-model` row in `managed.yml`; dsh itself has no `--model` or `--effort` options. Omitted effort uses provider/session defaults. Set it explicitly to pin it; no common effort set is promised across providers. An unavailable model fails the task without automatic fallback.

The default DeepSeek API uses `DEEPSEEK_API_KEY`, provided through the launch environment or an existing `dsh_env_file`. Headless does not read interactive credentials by default. This tool can explicitly read selected string refs from an existing version-1 dsh credential store and pass them to the child in memory, without copying or migrating the source file.

CommandCode's **DeepSeek V4.1 Flash (latest) (CC)** has passed real-model acceptance. Replace these paths with your actual absolute paths:

```toml
provider = "commandcode"
model = "deepseek/deepseek-v4.1-flash"
dsh_credentials_file = "/home/you/.dsh/.credentials.yaml"
dsh_credential_refs = ["COMMANDCODE_API_KEY"]
dsh_plugins = ["/home/you/.dsh/profiles/web/node_modules/@mars-sea/dsh-commandcode-provider"]
dsh_patches = ["/absolute/path/dsh-delegate-rs/examples/commandcode.yml"]
```

Configuration contains paths and ref names, never keys. Precedence is inherited environment > `dsh_env_file` > selected credential refs, including existing empty values. Explicit plugins use managed links, and provider overlays precede mandatory PTC/read-only settings. See [configuration](INSTALL.en.md#configuration) for details.

Use `dsh-delegate-rs doctor` to check configuration, dependencies, and actual sandbox read/write boundaries. Verify model authentication with a real investigation.

See the [installation guide](INSTALL.en.md) for all settings, custom Codex directories, and manual MCP registration. Run any API protocol conversion service separately.

## Sandbox and data boundaries

- **Read-only access**: project files and ordinary host files are read-only. New tasks may write only to their managed conversation directory and temporary task directory. A working directory overlapping a writable directory is rejected.
- **Network and credentials**: network access for model calls is allowed. The sandbox restricts file writes; it does not isolate networks or credentials.
- **Startup**: dsh uses inner `read-only` permissions within the outer Codex OS sandbox. Local nested-sandbox/PTC probes have passed. Sandbox startup failure stops the task.
- **Conversation data**: each conversation has a private `DSH_HOME`; follow-ups reuse `state_dir/conversations/<root_id>`. Host GUI credentials, settings, and other products' login files are not copied.
- **Storage limits**: managed conversation directories default to a 128,000,000-byte limit (`profile_bytes`). Explicit external plugins have their managed links and targets verified; source package sizes are not traversed or counted. Other links are rejected. Periodic monitoring terminates tasks that exceed the limit; this is not a filesystem quota.

State is stored in your system's user data directory under `dsh-delegate-rs`, or the directory selected with `--state-dir`. Reports, caches, and private login files may contain sensitive data; keep them out of version control.

Preview cleanup before removing history:

```sh
dsh-delegate-rs cleanup --dry-run
dsh-delegate-rs cleanup
```

Deleted tasks cannot be queried or followed up. Removing the last history reference to a conversation also removes its managed directory. Cleanup only removes managed data with verified ownership. See [cleanup](INSTALL.en.md#run-and-clean-up) and [uninstallation](INSTALL.en.md#sandbox-scope-and-uninstall).

## Support and validation

Actual validation covers Linux. Source and CI include a six-platform build matrix for Linux, macOS, and Windows on x86_64/ARM64; other platforms still need real-environment acceptance. Doctor has verified PTC Node execution, reads, rejected writes, and control-pipe RPC inside the outer Codex 0.166.0 sandbox with inner dsh `read-only`. These checks make no model call; authentication and model acceptance remain separate.

Separate real-model acceptance passed with CommandCode: read-only investigation, same-session follow-up, cancellation during tool execution, and resuming after cancellation. See the [validation guide](docs/validation.md) for commands and compatibility scope.

Run tests from the source repository:

```sh
cargo test --locked
cargo test --test sandbox native_sandbox_preserves_fixture -- --ignored
```

## License

The [MIT license](LICENSE) covers this project's code. Derived from agy-delegate-rs, retaining upstream attribution: `Copyright (c) 2026 agy-delegate-rs contributors`. The tool calls an externally installed Codex CLI and does not bundle its binary or bubblewrap.
