# 样例素材

- `ui/dialogue.png`、`ui/panel.png`、`ui/button.png`、`ui/button-hover.png`、`ui/button-disabled.png`：本项目通过 `examples/generate_ui_assets.rs` 生成的界面占位皮肤，按 CC0-1.0 提供；运行 `cargo run --locked --example generate_ui_assets` 可重新生成。游戏作者可替换图片并在 `ui.xml` 调整位置、大小和基本颜色。

- `audio/room.wav`、`audio/knock.wav`、`audio/lin.wav`：本项目通过 `examples/generate_demo_audio.rs` 合成的低音量音乐、敲门声和语音通道提示音，按 CC0-1.0 提供。`lin.wav` 是合成提示音，不是角色配音；运行 `cargo run --locked --example generate_demo_audio` 可重新生成。

- `bg/room.png`、`characters/lin.png`：本项目通过 `tools/generate-demo-assets.ps1` 绘制的几何占位图，按 CC0-1.0 提供，可自由复制、修改和分发。CC0 说明：https://creativecommons.org/publicdomain/zero/1.0/ 。
- `fonts/chinese.otf`：Noto Sans CJK SC Regular，版本 Sans 2.004，版权归字体原作者，采用 SIL Open Font License 1.1，完整许可见同目录 `OFL.txt`。
- 字体原文件：https://raw.githubusercontent.com/notofonts/noto-cjk/Sans2.004/Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf
- 字体许可证：https://github.com/notofonts/noto-cjk/blob/Sans2.004/LICENSE

分发样例时必须连同字体许可证一起复制。占位图不代表最终美术质量。

- `video/intro.mp4`、`video/credits.mp4`：通过 `tools/generate-demo-media.ps1` 生成的测试色条、项目字体标题和低音量合成音，按 CC0-1.0 提供；仅为解码/播放演示，不是商业片头素材。ffmpeg 是生成工具，运行播放器不依赖它。

- `live2d/Haru/`：来自用户提供的 CubismSdkForNative-5-r.5 的官方 Haru 样例，保留完整模型目录。此素材不是 CC0；SDK、模型及样例使用条款见 [LIVE2D-SDK-LICENSE.md](LIVE2D-SDK-LICENSE.md)。Windows Core 在宿主 runtime/live2d 中，其独立许可与可再分发文件清单随项目/本地包保留。当前集成为本地开发与验证，未公开发布。
