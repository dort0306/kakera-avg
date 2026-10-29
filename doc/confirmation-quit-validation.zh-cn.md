# 自定义确认弹窗与 UI 退出验收

日期：2026-10-29。应用 v0.9.1，story 8/UI 3/save 9/content 11/KPAK 1/2 均不变。依据 [实现策略](confirmation-quit-implementation-plan.zh-cn.md)，用法见 [API](xml-js-api.zh-cn.md#自定义确认弹窗与退出游戏)。

## 实现

- 确认页仅使用已有 XML/JS：模态 screen、RGBA 遮罩/面板、可见性绑定、标题/正文/按钮文字变量和按钮皮肤。标题可省略；demo 的退出确认有标题、返回主菜单确认无标题。取消使用 route=close，业务操作由固定 JS 分支执行，无 eval、原生固定弹窗或新组件框架。
- 新增无参 ctx.quit()，纯 Command/Operation::Quit 经完整 UI 事务校验后才设置 App 瞬时请求。严格拒绝额外 JSON 字段、重复退出以及与剧情/存读档/设置等操作混合；game.enter 与幕/页面生命周期禁止自动退出。
- 原生宿主经 event_loop.exit 正常关闭，先调用现有 shutdown；eframe 使用根窗口 Close。不会 process::exit，不执行剧情或页面退出回调，不消费 active 隐藏交接槽。取消和失败保留原 UI/剧情；Model/存档不增加窗口或退出标记。

## 自动检查

cargo fmt --check、cargo clippy --locked --all-targets -j 1 -- -D warnings、cargo test --locked -j 1、cargo run --locked -- --check、git diff --check 均通过。完整回归 **193 passed / 0 failed / 2 ignored**；默认忽略的 SDK/600MiB 压力用例本轮未另行运行。demo 为 31 幕/4 变量/3 JS/17 UI 页/2 视频/1 模型。

新增四项测试覆盖自定义遮罩/面板 alpha、可选标题、真实 egui 按钮/底层阻挡/取消/确认、严格退出参数、操作互斥、整批错误回滚、未开始退出、frameEnter/Exit 与 screenOpen 禁止退出，以及 active 隐藏槽退出前后字节不变、下次仍可恢复。相关测试 47 项库测试、16 项 UI 集成测试均通过。日志在 target/confirmation-quit-tests.log、confirmation-quit-clippy.log、confirmation-quit-check.log；测试临时目录为 target/test-tmp，本机调试测试使用允许 loopback 监听的执行环境。

## 实际窗口

Computer Use 实际操作，测试存档隔离在 target/confirmation-quit-gui-20261029-205315。没有读取或覆盖玩家原有存档。

1. D3D11 工程主菜单点击“退出游戏”，弹出带金色标题、正文、不同按钮皮肤和半透明背景的确认页。中文完整、面板边缘和按钮正常，窗口截图 1282×751，客户区为 1280×720。
2. 弹窗打开时点击底层“退出游戏”不会重复打开页面或触发操作；点击“取消”返回同一主菜单，没有开始剧情。
3. 进入 opening 后点击“重新开始”，确认页没有标题，正文正常折成两行；确认“返回主菜单”后根页切换成功，再选择另一条故事线正常逐字显示。
4. HUD 的退出按钮打开同一自定义确认页；点击确认后窗口正常关闭、进程退出码 0。磁盘 read.json 含 opening 与 alternate.opening，settings.json 已补写，原自动档保留；没有创建手动槽。
5. 使用同一隔离目录以 --renderer cpu 重新启动工程，主菜单/确认页显示正常；确认退出后退出码 0。没有额外建立剧情会话或自动档。
6. 正常 Windows 执行环境中，从 KPAK 2 发布模式以 CPU 后端启动，主菜单不显示工程窗，自定义退出确认与正常关闭均通过，退出码 0。最初在执行沙箱中的发布 GUI 初始化曾报 0xC0000008，尚未进入 UI；解除执行沙箱后同一播放器/包/存档复验成功。该环境失败未计为通过，也未据此修改发布或安全策略。

GUI 日志：target/confirmation-quit-gui.log、confirmation-quit-cpu-project.log、confirmation-quit-package-normal.log；退出码由对应等待进程工具结果记录。所有本轮交互验收窗口均经确认按钮正常退出。

## 两种资源包

真实 CLI --format 1/2 --level 1 均成功。两者各 48 文件，原文 27306405 字节；KPAK 1 为 24718325 字节、KPAK 2 为 24719789 字节。新增确认 XML 合并进原 UI 入口，JS 和现有皮肤/字体保留，源工程不改写。

新建源码外临时目录仅放播放器、Core 和两个包，两个 --package --check 均退出 0，均报告 17 UI 页。输出在 target/confirmation-quit-pack1/2.log、confirmation-quit-portable1/2.log；临时目录位置记录于 target/confirmation-quit-portable.path，执行沙箱结束后临时目录可能被系统清理。

## 未验证范围

本轮使用 --no-audio，未复验音频设备流和主观听感；未重做全部 DPI/跨显示器/全屏、200 槽存读档、媒体暂停/自然结束/跨进程交接或真实驱动丢失矩阵。active 交接退出及退出后的恢复主要由文件/App 测试覆盖，不冒充全部媒体 GUI 验收。非 Windows eframe 关闭路径未做实机 GUI 检查。没有生成优化版正式发行目录、公开发布、Git 提交或推送。
