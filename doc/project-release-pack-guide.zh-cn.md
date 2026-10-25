# v0.8.1 工程、发布与窗口模式指南

适用版本 0.8.1；唯一数据契约 story 8 / UI 3 / save 8；资源包新增 KPAK 2，保留 KPAK 1 小包兼容。旧剧情/UI/存档格式解析与旧档导入已删除，历史指南在 [v0.7.3 归档](archive/v0.7.3-project-release-pack-guide.zh-cn.md)。

## 工程运行

```powershell
cargo build --locked --release --bins
./target/release/kakera-avg.exe --project games/demo --check | Out-Host
./target/release/kakera-avg.exe --project games/demo --save-dir target/v081-development
```

--project 指定工程目录，默认源码启动为 games/demo。固定 story.xml，通过每份 XML 的 next.src 串联，末份 next end="true"；interface、scripts、assets 等路径以游戏根为准，不随章节目录变化。必须提供 UI 和全部 JS 注册；不读取 flow.xml，不运行旧 schema。

工程使用原生通知/去抖和固定快照候选准备。编辑 XML/JS/素材后完整验证当前变量/历史/条件/进入计划和资源，再原子替换。坏编辑保留旧会话/画面/声音；查看“工程状态”。不兼容候选只允许显式从入口重载，story_id 改变需重启。成功热更不写自动档/重复 CG 解锁，不重播瞬时动作。

## 窗口与设置

客户区固定 16:9，拖边/角同步约束；默认 1280×720，最小 640×360 逻辑尺寸，物理像素为 16×9 整数单位。标题栏和边框不参与游戏比例，最大化按钮禁用。

demo 设置页“全屏显示”由 XML/JS 调用 ctx.setting("fullscreen", bool)，保存成功后切当前显示器无边框全屏；关闭恢复进入前的大小和位置。fullscreen 默认 false，完整 settings.json 独立保存，重启按它恢复模式。非 16:9 显示器全屏仍等比留边，不改变系统分辨率。位置不跨进程保存。

每个新功能优先通过已有 XML/JS 组合；只有缺少宿主能力时才改 Rust，不按游戏页面新增原生窗口。

## 资源包与便携目录

```powershell
cargo run --locked --bin kakera-pack -- games/demo target/v081-publication/game.kpak
cargo run --locked --bin kakera-pack -- games/demo target/v081-publication/game.kpak --force --level 19
cargo run --locked -- --package target/v081-publication/game.kpak --cubism-core runtime/live2d/windows-x64/Live2DCubismCore.dll --check
./tools/package.ps1
```

kakera-pack 两个位置参数先于选项。支持 --format 1|2（默认 2）、--force、--level 1..22（默认 19）、--cubism-core DLL、--no-live2d。没有播放器的 --check/--renderer/--save-dir 参数；不带参数打印用法并失败退出。禁用 Core 校验不表示包中模型永久禁用，播放器仍须配置 Core 或显式 --no-live2d。

打包先验证完整源快照，合并 next 链到包内 story.xml，只移除链上的源章节；UI/JS 和其他文件不合并、不改磁盘源工程。确认指纹一致后编码、自检/独立加载，再原子写出；输出须在工程根外，已有输出没有 --force 就拒绝。KPAK 按逻辑路径排序、同字节去重、Zstandard 压缩与 SHA-256 损坏检测，未加密。2 使用独立 1MiB 原文块，1 保持整包 solid。素材不有损转码，不做引用裁剪；未被排除的普通文件也会收集。

发布脚本默认 target/package/kakera-avg-v0.8.1，支持 -Project、-Output、-CompressionLevel、-Format 1|2（默认 2）。目标已存在（包括空目录）就拒绝，失败目录保留诊断；本地操作不公开上传。输出结构：

```text
kakera-avg-v0.8.1/
  kakera-avg.exe
  game.kpak
  Live2DCubismCore.dll
  licenses/
  README.md
  doc/
```

脚本复制固定宿主 runtime 的 Core 与原许可/再分发清单、游戏字体/模型许可和依赖许可，不复制 games 散文件、用户档或本地缓存。kakera-pack.exe 留在 target/release 供作者使用。

## 发布启动与可信宿主配置

```powershell
./kakera-avg.exe --check | Out-Host
./kakera-avg.exe --save-dir D:/AVG-Saves/MyGame
```

程序自动发现 **exe 旁** game.kpak（不是当前目录），发布只读包，无工程监控/浮窗。坏包报错，不回退目录。--package 与 --project 互斥；修改工程不会影响发布进程，换包后重启。

Core 查找顺序：--cubism-core → CUBISM_CORE → exe 旁 Live2DCubismCore.dll；只有工程/打包额外回退源码固定 runtime/live2d/windows-x64。显式坏路径直接失败；游戏 XML/JS/包不能选择任意原生库。

--renderer auto/d3d11/cpu：默认硬件 D3D11，失败可 WARP/CPU；显式 d3d11 要求硬件设备，运行时错误仍保护性回退。cpu 仅替换像素算法，窗口仍为 D3D11。--no-audio 禁设备但仍检查资源，--no-live2d 显式用静态 fallback，仍检查模型资源结构。

Windows 播放器为 GUI 子系统：发布无控制台，启动失败显示原生错误框；--check/--help 只附着已有控制台并保留重定向。PowerShell 直接调用可能异步返回，自动化用管道或 Start-Process -Wait 取退出码。kakera-pack 为控制台工具。

## 选择包格式与流式边界

默认生成 KPAK 2；`kakera-pack games/demo target/small.kpak --format 1` 保留小包输出，发布脚本也支持 `-Format 1`。播放器按包头自动识别，不需额外播放参数；未知版本或坏包明确失败，不回退散文件。KPAK 1 的旧包可直接读取，包内内容仍须符合 story 8/UI 3/save 8。

2 打开时只读取头和索引，访问时验证并解压对应块，每个流只缓存当前 1MiB 原文块；`--check` 完整扫描唯一块。没有访问的坏块不保证在普通启动时发现。格式与损坏检测约定见 [KPAK 1/2 格式](kpak-format.zh-cn.md)。

工程候选和 KPAK 2 打包使用私有临时磁盘快照，不全量放入 RAM；仍需复制源字节并有足够磁盘空间，提交前全量重算源指纹。KPAK 1 打包保留 512MiB 内存快照限制。视频流式复制到后端临时 MP4，不先构造整文件 Vec；已声明图像/音频/模型仍按既有预算预先解码，视频仍有尺寸/时长/音频缓存上限。不是所有解码资源的场景级惰性加载，也没有包内直接视频 seek 或网络流。

## 用户数据和限制

--save-dir 直接使用指定目录，不额外附 story_id。工程与发布默认同 story_id 会共享数据，验收建议隔离。当前手动 slot-1..200.json、自动 auto-1..10.json 为 save 8，加载完整重放历史与变量；旧格式或内容版本不匹配明确拒绝。设置、CG、已读分别在 settings.json/profile.json/read.json，不被槽覆盖；settings 必须完整包含当前字段，无缺字段迁移。screenshot 字段必需，可 null（工具快照）。

用户 JSON、.git/.idea/.vscode/target/saves/node_modules 和备份/临时文件不入包与工程监控；游戏内容勿使用保留用户文件名。路径统一 /，禁止绝对/盘符/../反斜杠/大小写碰撞/越界链接。KPAK 2 包及逻辑总量最多 1TiB、单文件 64GiB、100000 文件、索引 128MiB；KPAK 1 仍为包 512MiB、单文件 128MiB、10000 文件；剧本链最多 128 文件、源和合并 XML 各 16MiB。

数据检查不等于 GUI 或听感验收，结果和未覆盖项见 [v0.8.1 验收](v0.8.1-validation.zh-cn.md)。完整创作契约见 [API 手册](xml-js-api.zh-cn.md)。
