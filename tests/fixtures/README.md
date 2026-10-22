# 音频测试素材

`cue.ogg` 来自本项目 `examples/generate_demo_audio.rs` 合成的 `games/demo/assets/audio/lin.wav`，按 CC0-1.0 提供，不含真人录音。

生成步骤（ffmpeg 是维护测试素材的可选工具，运行游戏/测试不需要它）：

```powershell
cargo run --locked --example generate_demo_audio
ffmpeg -hide_banner -loglevel error -i games/demo/assets/audio/lin.wav -c:a libvorbis -q:a 3 tests/fixtures/cue.ogg
```
