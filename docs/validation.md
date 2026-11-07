# 验证指南

## 自动检查

在仓库根目录执行：

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
node --test scripts/test-launcher.cjs scripts/test-publish-npm.cjs
python3 -m unittest discover -s scripts -p 'test_*.py' -v
```

原生沙箱和真实 dsh 探针需要安装可用的 Codex CLI、dsh 和 Node.js；普通测试默认跳过这些环境相关检查：

```sh
cargo test --test sandbox native_sandbox_preserves_fixture -- --ignored
cargo test --test maintenance native_doctor_ -- --ignored
dsh-delegate-rs doctor
```

`doctor` 不调用模型。它验证外层沙箱对项目只读、临时目录可写，以及内层 dsh `read-only` 下的 PTC Node 读取、写入拒绝和 control-pipe RPC。

## 真实模型验收

先按 [安装指南](../INSTALL.md#配置) 配置 provider 和认证，在只读测试仓库中执行以下步骤：

1. 提交一条要求文件与行号证据的调查，确认任务 succeeded、报告非空。
2. 检查任务的 `tmp/dsh.log`：模型工具调用应为 `run_code`，最终 reason 为 completed、进程退出码为 0。
3. 用 `followup` 追问先前结论，确认复用 session 和上下文。
4. 在工具执行中请求取消，确认最终 cancelled 且 worker 已停止。
5. 通过原任务继续追问，确认取消后仍可恢复会话。

任务 ID、会话 ID、报告、日志、凭据文件与本机配置属于验收环境数据，不作为发布内容或测试夹具。

## 已验证范围

Linux 环境已通过上述自动检查、原生沙箱探针和真实模型调查/追问/取消/恢复验收。使用版本为 dsh 0.2.2-rc.1、Node.js 26.13.0、Codex CLI 0.166.0；真实模型验收使用可选 CommandCode provider 0.17.2 的 `deepseek/deepseek-v4.1-flash` 路由。

macOS、Windows、其它架构和其它 provider 仍需相应环境验证。CI 的平台矩阵不等于全部平台的 dsh 启动或模型认证均已验收。版本升级后应按 [集成契约](dsh-contract.md) 重新验证。
