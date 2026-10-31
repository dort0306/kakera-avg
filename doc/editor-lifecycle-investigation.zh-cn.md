# 编辑器生命周期续接超时排查

日期：2026-10-30；引擎与相邻编辑器工作区均为 v0.10.1。

## 原始报告与结论

用户提供的 `../kakera-editor-vscode/.test-work/sync-integration-serial.log` 与 `sync-lifecycle.log` 确实记录了失败：`continue` 在十秒内仍处于 opening，UI 栈为 hud/cut，trust=0，调试状态 running，无宿主提示或 last_error。原失败记录保留，不能用本轮通过改写历史结果。

本轮使用编辑器原有 `resources/tools/win32-x64/kakera-avg.exe`（SHA-256 `2D3D30D69628600F6EEA491FE81AAA64EF5EC694DC9C7B0E9C4E9AB350888F1B`），独立调用原 `verifyLifecycleEditor`，continue/frame/page 均通过；随后在编辑器目录真实执行 `npm run test:integration`，退出 0，包含生命周期、本地化、条件/i64、UI 编辑器、两种 KPAK 和独立宿主包检查。隔离结果目录为 `../kakera-editor-vscode/.test-work/native-9HeAL3`。此次通过发生在任何引擎运行代码修改或二进制替换之前。

目前没有确认引擎缺陷，也没有更改生产运行逻辑。检查发现，UI 演出按既有契约在窗口失焦、宿主提示或调试暂停时冻结；菜单的剧情暂停不冻结 UI 演出。调试 `resume` 仅解除调试暂停，不会取得操作系统窗口焦点。原日志没有焦点、sequence 是否活动及演出进度字段，因此失焦是可能解释，尚不能确认是历史失败根因。测试中的十秒墙钟超时不等于获得了十秒有效演出时间。

## 回归范围

新增 `app::lifecycle::tests::screen_sequence_naturally_resumes_after_focus_and_debug_pause`：只用 screen.sequence 属性触发演出，不调用 ctx.sequence、不以跳过代替自然完成。覆盖无参 continuation、显式幕、串联第二页面；调试暂停与失焦保留旧 Session/活动隐藏槽，重新聚焦不补算暂停时间，完成后变量仅加一次、历史可重放、隐藏槽 inactive，后续 tick 不重复提交。时钟注入辅助仅在 cfg(test) 编译。

测试使用临时工程与独立存档，不改玩家档案。首次沙箱内运行遇到 Windows 原子替换拒绝访问，随后在允许本机 TCP 与原子替换的环境完整重跑，通过 271 项、失败 0 项、忽略 2 项（SDK 与 600MiB 压力用例）。`cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked -j 1`、`cargo run --locked -- --check` 与本次涉及文件的 `git diff --check` 通过。demo 校验为 31 幕、4 变量、3 JS、19 UI 页面、2 视频、1 模型。

`cargo build --locked --release --bins` 通过，产物为 `target/release/kakera-avg.exe` 与 `target/release/kakera-pack.exe`。再以新 release 播放器执行编辑器原始 `verifyLifecycleEditor`，continue/frame/page 全部通过，退出 0。独立调用的打包测试入口仅在本仓库 `target/lifecycle-investigation/run.cjs`，测试工程也位于该目录；没有改写编辑器测试源码或替换其原有工具二进制。

本轮没有人工穷举所有窗口焦点切换、锁屏、视频/跨进程恢复或设备听感，自动 fade 与调试进程检查不能替代这些验收。未修改编辑器源代码，未提交或推送 Git。
