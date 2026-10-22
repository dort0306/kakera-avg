# Kakera AVG Rust

基于 `doc/rust-mvc-plan.zh-cn.md` 的最小 Windows 桌面 AVG 播放器。一句台词对应一个具有稳定 ID 的完整场景；XML 定义内容与流转，Rust 控制器决定下一场景。

已实现四场景双分支样例、背景与三个立绘槽位、中文整句显示、按钮/空格推进、明确结尾、单槽存读档、严格内容校验与资源缓存。游戏区域固定 16:9，窗口变化时等比缩放并留黑边。

## 快速启动

需要 Rust stable（最低 1.88）与 Windows MSVC C++ 构建工具；通过 rustup 安装 Rust 时选择 MSVC 工具链。首次构建需联网下载 Cargo 依赖。仓库包含 `Cargo.lock`，请使用 `--locked`。

```powershell
cargo run --locked
# 指定游戏目录
cargo run --locked -- --game D:/codes/kakera-avg-rust/games/demo
# 无窗口校验 XML、图片解码与中文字符覆盖
cargo run --locked -- --check
```

默认先查找可执行文件旁的 `games/demo`，然后回退到编译时的源码样例目录。显式 `--game` 的相对路径相对于命令行工作目录；XML 中的素材路径始终相对于该游戏目录。发布目录可以整体拷贝到其他位置运行。

原始 `doc/rust-mvc-example` 仅是数据契约，没有素材。可直接运行的完整样例位于 `games/demo`，不应把原始示例当成完整游戏目录。

## 操作与存档

- 普通场景：点击“继续”或按空格。
- 选项场景：点击选项；空格不选择默认分支。
- 结尾：显示“已结束”，可重新开始或读档。
- 右上角“保存”覆盖当前故事的唯一存档槽；“读档”显式恢复，启动时不会自动加载。
- 错误与成功提示需点击“关闭”；提示打开时不接收剧情输入。

Windows 存档位置为 `%LOCALAPPDATA%\kakera\kakera-avg\data\<story_id>\save.json`；以程序启动日志与保存成功提示中的实际路径为准。存档不写入游戏目录。JSON 包含 `schema_version`、`story_id`、`content_version`、`frame_id`，全部验证通过后才替换当前会话。没有存档、坏 JSON、错误故事/版本、未知场景都会报错并保留当前状态。

写入使用同目录临时文件、`sync_all` 与 `tempfile::persist` 原子替换。Windows 上使用底层平台替换语义，不先删除旧存档；不承诺所有文件系统与断电情况下的持久性。

## 模块边界

```text
src/model.rs       纯数据：Story、Frame、Visual、Session、Route
src/controller.rs  纯函数 reduce：用户输入 → 新 Session
src/loader.rs      XML 解析、引用/结构校验、展开视觉预设
src/save.rs        存档校验与原子文件替换
src/assets.rs      启动时解码图片、检查字体、安装字体及缓存纹理
src/view.rs        只读场景 → egui 画面与 UiIntent
src/app.rs         装配与生命周期，处理剧情和存读档意图
src/main.rs        命令行、路径选择、启动与错误退出
games/demo/        可运行 XML、占位 PNG、中文字体、许可证
tests/             场景/加载器、存档、资源集成测试
tools/             Windows 占位图生成与打包脚本
```

当前仓库没有旧 C++ 源码，因此直接在根目录创建单 crate，不额外嵌套 `kakera-rs/`。GUI 固定使用 eframe/egui **0.33.3** 与 wgpu；XML 使用 roxmltree **0.21.1**。选择可编译的稳定组合，完整依赖版本以锁文件为准。

## 编辑剧情

以 `games/demo/story.xml`、`flow.xml` 为契约：

- 根节点分别为 `<story schema="1" id="..." content-version="1">` 和 `<flow schema="1" story="..." entry="...">`。
- `assets` 定义 image/font；至少配置一份中文字体。支持 PNG/JPEG，单边图片上限 8192 像素。所有路径使用游戏目录内的相对路径，不接受 `..`、绝对路径或指向目录外的链接。
- `characters` 定义人物 ID 和姓名，`visuals` 定义可复用的背景与完整立绘列表，位置为 `left/center/right`，同位置不可重复。
- `frames` 定义场景 ID、可选 visual/speaker、一个非空 `<text>`，以及可选的非空 options。省略 visual 表示无背景无立绘，省略 speaker 表示旁白。没有隐含继承。
- 每场景必须恰好对应一个 `next`、`choice` 或 `end`；选项 ID 与 choice/on 路由必须一一匹配。
- `<text>` 与 `<option>` 只接受普通 XML 文本（可用 `&amp;` 等实体），不接受内嵌标签；保留空格与换行，XML 标准将 CRLF 规范化为 LF。避免在文本标签内加入非预期缩进。
- 不可达场景给警告；循环合法。未知元素、属性、命名空间、重复 ID、缺失引用会阻止加载，XML 错误报告文件与行列。启动错误输出到终端并以非零状态退出，运行中错误显示在窗口中。
- 长台词超出固定对话区域时显示开发提示，并允许滚动阅读；正式内容应拆句。选项过多可滚动，按钮文本可换行。

保存位置依赖稳定场景 ID；改变内容兼容性时应递增 `content-version`。本版不迁移旧存档。

## 检查与发布

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked -- --check
cargo build --locked --release
# 打包到新目录（默认 target/package/kakera-avg，不覆盖已有目录）
./tools/package.ps1
```

发布时将 `kakera-avg.exe` 和整个 `games/demo`（包括许可证）放在一起；双击程序或在终端运行。也可使用 `--game <目录>` 加载其他游戏。程序没有自动依赖系统中文字体，样例使用随包提供的 Noto Sans CJK SC；素材来源和许可见 `games/demo/assets/licenses/README.md`。占位 PNG 可执行 `./tools/generate-demo-assets.ps1` 重新生成。

自动测试覆盖两条分支、完整场景还原、错误引用/路由/重复 ID、普通文本语义、循环与不可达警告、存档往返及覆盖、坏档拒绝、资源缺失与解码，以及 Windows 文件被锁时旧档保留和临时文件清理。

2026-10-23 在 Windows 上完成验收：15 项集成测试、格式检查、Clippy 无警告、发布构建与 `--check` 均通过。实际窗口验证了中文首帧、空格推进、选项场景空格无效、两个分支结尾、拒绝分支清除人物与姓名，以及在 question 保存后完全退出、从便携包重启并恢复选项。最大化窗口验证了 16:9 等比缩放与黑边；从其他工作目录执行便携包 `--check` 通过。严格 Windows 沙箱可能拦截临时文件替换；本次存档覆盖与锁定失败测试在正常桌面权限下通过。

本版暂不包含逐字动画、音频、转场、快进、回滚、多槽、变量、脚本解释器或编辑器。首发验证范围为 Windows 桌面；其他平台尚未验收。
