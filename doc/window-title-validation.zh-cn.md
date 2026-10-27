# 可配置窗口标题验收

日期：2026-10-26。依据 [策略](window-title-implementation-plan.zh-cn.md)，应用仍为 v0.8.2；story 8/UI 3/save 8、KPAK 1/2 不变。

## 实现

- ui.xml 根新增 title 和 title-bind，互斥；默认 Kakera AVG，动态标题仅绑定已声明 UI string。JS 复用 ctx.setUi，可在 on.ui 或 game.enter 内修改，没有新增脚本窗口句柄/IO。
- 标题为 1–256 个 Unicode 标量值，禁止纯空白与控制字符。初始 XML 错误有文件位置；运行时整批验证最终草稿，失败不提交 UI、标题或同批剧情/IO。
- 已校验标题缓存在 UI State；Windows 启动使用配置初值，App 提交后在宿主更新边界同步，仅改变时调用原生窗口 API，不附加 D3D11 后缀。另接入非 Windows 的 eframe 标题命令，未做跨平台实机验收。
- 标题不进入 Model/存档/settings。启动/热更按 UI 初值重建；读档/重开沿用当前 UI 变量并执行 game.enter，可按场景重算标题。demo 已增加场景标题与手动切换按钮。API 第 28 节、README、指南和 AGENTS 已同步；发布脚本沿用现有 UI 文件打包，无需新增参数。

## 检查结果

| 检查 | 结果 |
| --- | --- |
| cargo fmt --check | 通过 |
| cargo clippy --locked --all-targets -- -D warnings | 通过 |
| cargo test --locked | 139 通过，2 个显式用例默认忽略 |
| cargo run --locked -- --check | 通过；30 场景、4 变量、3 JS、12 UI 页、2 视频、1 模型 |
| release 构建与 package.ps1 | 通过；KPAK 2，70 文件、原文 27679553 字节、资源包 23801345 字节，压缩级别 3 |
| 源码外发布副本 --check | 退出码 0，使用程序旁 KPAK 2 与 Core，无散装游戏目录 |
| git diff --check | 通过 |

新增 tests/window_title.rs 的 4 项测试覆盖默认/静态/绑定、中文/实体/引号/emoji、256 字符边界、超长/空白/控制字符、错误绑定前缀/缺失变量/非字符串、XML 文件位置、真实 QuickJS 修改、game.enter 命令、同批最终值与失败回滚、UI 重建及 KPAK 1/2 独立重载。完整测试保留当前 schema 拒旧输入、事务/原子存档、窗口比例与媒体等既有回归。

## 实际 Windows 窗口

使用 computer-use 操作隔离的 target/window-title-project 副本与 target/window-title-saves 数据，原 demo 源文件没有为验收临时改写。锁屏期间停止 UI 操作，用户确认解锁后继续。

- 原生窗口标题显示“午后物语 · opening”，没有引擎/后端附加后缀；中文、16:9 客户区和对话背景正常。
- 效果演示 → 点九 / 图集 → 切换窗口标题，原生标题改为“自定义标题 · 中文 & JS”，再点恢复“午后物语 · opening”；菜单状态及场景未改变。新增按钮没有文本溢出。
- 修改副本 XML 为静态 title="静态标题 · XML"，实际热更后原生标题生效；随后将 title 置空，工程状态显示“更新失败”，原生标题与旧场景保留。
- 恢复有效的 title-bind 配置后，标题回到“午后物语 · opening”，剧情位置保留；验收窗口正常退出。

本轮独立发布目录为 target/package/kakera-avg-v0.8.2-window-title，保留上一轮发布目录。源码外副本位置在 target/window-title-external-path.txt。包含当前手册及字体/模型/Core/依赖许可证；没有公开发布、Git 提交或推送。

本轮 GUI 使用 --no-audio，未复测设备音频或主观听感；未重新进行完整跨进程手动/自动读档、末页 200 槽、Ctrl、视频暂停/结束、CPU/全屏/多显示器 DPI 的人工矩阵。存档与控制流代码未改，相关自动回归通过；历史 GUI 验收不能当作本轮复测。

日志位于忽略的 target/window-title-focused.log、window-title-clippy.log、window-title-tests.log、window-title-check.log、window-title-package.log、window-title-package-check.log。原子写包/媒体检查使用普通宿主权限；沙箱中原子替换报拒绝访问不代表应用失败。构建曾出现增量缓存硬链接降为复制的环境提示，不影响检查结果。
