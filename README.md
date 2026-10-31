# Kakera AVG Engine

Rust 与 QuickJS 驱动的 Windows x64 AVG（视觉小说）播放器。剧情、界面、样式和交互优先用 XML 与 JavaScript 描述；Rust 宿主只提供经过严格校验的受约束能力，只有现有 XML/JS 组合无法满足需求时才扩展。每个场景（Frame）是一份可独立恢复的完整显示状态。

- **剧情与脚本**：帧式剧情、变量/条件/选项、历史检查点、自动播放、逐字显示与快进；条件、更新和生命周期在隔离 QuickJS 中执行，Rust 验证候选后原子提交。
- **XML/JS 界面**：schema 3 UI、自定义皮肤与页面栈、按钮路由、动画与 UI 视频、加载期组合标签、通用分页、可配置快捷键与配色。
- **媒体与呈现**：BGM、语音、音效和视频独立音量与总静音、图片效果、立绘动作、Live2D、D3D11 与 CPU fallback、固定 16:9 窗口与无边框全屏。
- **存档与用户数据**：手动/自动槽、截图存档、CG 画廊、已读记录、隐藏生命周期交接槽；偏好文件独立于剧情槽。
- **工程与发布**：严格 XML 校验、多文件剧情/UI、工程热更、KPAK 2 流式资源包与 KPAK 1 兼容、确定性打包和发布自检。

当前应用版本为 v0.11.2；使用与格式说明见下方 [文档](#文档)。

## 构建

需要 Rust stable（最低 1.88）、MSVC C++ 工具链和 Windows 系统媒体组件；依赖固定在 `Cargo.lock`。

```powershell
cargo build --locked --release --bins
```

产物为 `target/release/kakera-avg.exe`（播放器）和 `target/release/kakera-pack.exe`（打包器）。从源码运行 demo：

```powershell
cargo run --locked
cargo run --locked -- --project games/demo --save-dir target/dev-saves
cargo run --locked -- --project games/demo --renderer cpu --no-audio
```

本地发布目录用 `./tools/package.ps1`，默认输出 `target/package/kakera-avg-v0.11.2`；发布包、工程模式和调试的完整说明见 [工程、发布与窗口模式指南](doc/project-release-pack-guide.zh-cn.md)。

## 测试

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked -- --check
```

`--check` 会完整校验剧情、UI、JS、资源和引用后退出，不打开窗口。自动化检查不能代替 GUI、音频设备或主观听感验收；已验证范围和未执行项见 [验证与限制](doc/limits.zh-cn.md)。

## 许可证

本项目使用 MIT 许可证，见 [LICENSE](LICENSE)，Copyright © 2026 dort0306。

发布包同时分发第三方许可：`licenses/`（Fluent、QuickJS、rquickjs、zstd、winit、windows-rs 等）、Live2D Cubism Core 的许可与再分发清单（`runtime/live2d/`）、字体和 demo 素材许可（`games/demo/assets/licenses/`）。这些文件必须随发布包保留，字体不依赖开发机。

## 文档

- [PLAN.md](PLAN.md)：版本规划。
- [版本说明](doc/release-notes.zh-cn.md)：各版本已实现的内容。
- [播放器使用指南](doc/player-guide.zh-cn.md)：启动、剧情运行、存档与用户数据。
- [数据契约](doc/data-contracts.zh-cn.md)：story/UI/save/资源包与偏好文件格式。
- [XML/JS API](doc/xml-js-api.zh-cn.md)：作者接口、绑定、预算与限制。
- [工程、发布与窗口模式指南](doc/project-release-pack-guide.zh-cn.md)：工程、打包、发布与调试。
- [验证与限制](doc/limits.zh-cn.md)：检查结果、资源边界和未支持能力。
- [KPAK 格式](doc/kpak-format.zh-cn.md)：KPAK 1/2 二进制契约。
- [历史文档](doc/archive/)：旧版本说明与归档手册。
