# 国际化可运行示例源文件

在仓库根运行 `./tools/prepare-localization-demo.ps1`，将本目录与 demo 的字体、占位音频、原许可复制到新建的 `target/localization-demo`，然后：

```powershell
cargo run --locked -- --project target/localization-demo --check --no-live2d
cargo run --locked -- --project target/localization-demo --save-dir target/localization-saves --no-live2d
```

生成脚本拒绝覆盖已有目录；指定 `-Output` 可以另建工程。原 demo 不开启国际化。

文本中文/英文只是作者内容示例；语音 zh-CN/ja-JP 使用不同的已有 WAV **占位映射，不是中文/日语配音**。正式宿主提示始终只有中文。选择页用 repeat/绑定/普通 JS 编写，两个草稿独立，应用同批写 language.json，取消不写文件；空草稿代表跟随系统。无 key 的按钮刻意保留字面量，演示渐进接入。

本目录单独缺少素材，必须先生成；生成后的工程经同一 ContentSource 读取全部文件，可以直接打 KPAK 1/2。不要将此目录当成含素材的完整发行包。
