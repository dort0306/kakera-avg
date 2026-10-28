# UI 演出与角色音效验收

日期：2026-10-28；基于 v0.8.4 的增量，story 8/UI 3/save 8/KPAK 1/2 当时不变。v0.9.0 起存档为 9，frame.video 已移除，故本记录中剧情视频相关 GUI 结果只作历史参考。

## 实现与边界

见 [落地计划](ui-presentation-implementation-plan.zh-cn.md) 与 [API 第 31 节](xml-js-api.zh-cn.md#31-ui-演出过场视频与角色音效)。v0.9.0 起剧情视频已删除，Frame.video 不再存在，剧情插播也改用 UI sequence + 生命周期交接（第 32 节）；以下记录保留 v0.8.4 当时“frame/JS 剧情视频”与 UI sequence 并存的历史验收边界。没有向剧情脚本开放随机、时钟或 IO。新增宿主能力集中于 interface/presentation、app/ui_media 和音频/视频呈现边界，其余示例用 XML/JS。

支持音效单资源/带权候选组，先按作者禁用/玩家角色与资产偏好过滤，再随机选择；空集静音，直接引用或另一组均不能绕过禁用。玩家 ui-audio.json 独立、严格完整字段与预算、原子替换，不随剧情读档回滚，不打包/监控。所有声音命令先验证，再提交状态后播放；偏好写入失败不提交同批 UI 赋值。演出禁止混入其他剧情/IO 操作。

UI 透明度继承到控件和子节点，支持 screen.sequence 自动触发和 ctx.sequence 显式触发。菜单暂停剧情，但不暂停本页演出；失焦/提示/调试冻结演出。结束与点击/空格跳过只执行一次路由，跳过不补播未来声音，输入不穿透剧情。视频与剧情视频互斥，仍至多一个有界解码线程。演出不入存档，不支持中途保存、并行轨道或任意完成 JS 回调。热更延后到演出结束，设备重建取消，运行失败取消并保留剧情。

## 自动检查

- cargo fmt --check、cargo clippy --locked --all-targets -- -D warnings、cargo test --locked、cargo run --locked -- --check、git diff --check 通过。
- 完整回归：164 passed / 0 failed / 2 ignored。SDK 与大包显式用例本轮未另行运行。最终补充 ID/偏好预算及错误分支修正后，对 lib/ui_presentation/ui_v04 补跑 43 项，全部通过，并重新检查 demo。
- 新增 tests/ui_presentation.rs 覆盖严格解析、跨 include 前向引用/原位置、资源/角色/目标/预算拒绝、权重边界、随机前过滤、全排除不抽样、角色与直接引用一致、跨组作者禁用、真实 JS 类型化命令、生命周期拒绝、偏好重载/坏字段/原子失败。
- app/ui_media 单元测试覆盖自动淡入、失焦冻结、跳过最终态和单次完成、页面退出清理、存档受阻、混合剧情事务拒绝、偏好失败时局部状态/剧情回滚、真实 MP4 准备、跳过返回及剧情视频互斥。
- demo 当前 30 幕、4 变量、3 JS、13 UI 页面、2 视频、1 Live2D 模型。

Windows 原子替换与真实媒体测试在沙箱外执行；最初沙箱内偏好保存出现权限错误，没有把该环境失败作为产品通过记录。GUI 音频设备与主观听感分开记录。

## 真实窗口

使用独立 target/ui-media-gui-saves，未操作作者原有存档。

- D3D11 硬件窗口：1280×720 客户区、中文布局、进入演出页自动淡入正常；真实 intro MP4 覆盖层有画面并自然结束回原页；credits 播放后空格返回 HUD，剧情仍在原 question。首次较慢操作在视频已经自然结束后才按空格，触发了正常剧情推进，因此另做即时跳过验证；不将第一次操作记为跳过通过。
- 角色开关与单条资源排除实际点击后写入 ui-audio.json；完全退出后使用 CPU 窗口与同一独立目录重启，两个开关状态仍保持。CPU 窗口检查中文、页面淡入、单音效与随机组按钮、淡出/淡入与鼠标跳过；没有错误提示，剧情仍保持 opening。
- 工程窗口将演出入口从工程状态浮层下移开，真实子 UI XML 热更后可见且保留当前剧情。
- 两次音频运行均建立实际输出流：2 声道、48000Hz、F32；实际点击单音效/随机组按钮。未做主观听感验收，也未声称从截图证明具体声波或角色声线正确。排除规则由确定性候选测试证明。

本轮未人工穷举所有显示器/DPI/全屏、UI 视频失焦音画同步、全部存读档矩阵或 SDK/超大包；这些项目不能由本轮自动测试替代。此前版本验收记录保持历史原文。

## 两格式发布重载

实际 CLI --format 1/2 --level 1 均成功，UI include 已汇总且源文件未改写。KPAK 1：48 文件/24714480 字节；KPAK 2：48 文件/24718374 字节，原文均为 27301683 字节。新增 UI 视频/音效资源由实际 ContentSource 准备与完整校验。

将最终 debug 播放器、固定 Core 和两个包复制到新建系统临时目录（没有游戏散资源），显式等待两个 --package --check 进程，退出码均 0，均报告上述 30 幕/13 页/2 视频/1 模型。未将 debug 验证目录宣称为优化版正式发行包，未执行公开发布。

日志：target/ui-media-full-tests.log、ui-media-focused-tests.log、ui-media-clippy.log、ui-media-check.log、ui-media-pack1.log、ui-media-pack2.log、ui-media-outside1.log、ui-media-outside2.log、ui-media-gui-error.log、ui-media-cpu-error.log。临时验证位置保存在 target/ui-media-outside.path。无 Git 提交或推送。
