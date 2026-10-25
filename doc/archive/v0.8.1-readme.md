# Kakera AVG Rust · v0.8.1

Rust、QuickJS、XML/JavaScript 驱动的 Windows x64 AVG 播放器。界面、布局和交互优先用 XML/JS；只有现有能力无法满足需求时才扩展受约束的 Rust 宿主能力，这一规则已写入 AGENTS.md。

窗口支持固定 **16:9 客户区缩放**，拖动窗口边/角不产生比例黑边；设置页可切换无边框全屏，关闭后恢复进入前窗口大小和位置。全屏开关用 XML toggle 和 `ctx.setting("fullscreen", bool)`，默认窗口化，独立设置跨进程保存。非 16:9 显示器全屏仍保持等比画面并留边；不拉伸或裁掉 UI。

v0.8.1 新增面向大体积游戏的 **KPAK 2 流式资源包**，默认打包为 2；播放器保留 KPAK 1 小包兼容。资源包版本独立于剧情/UI/存档，不恢复旧 story/flow/存档解析器。

## 当前数据契约

| 对象 | 当前值 |
| --- | --- |
| story | schema 8；固定 story.xml，单/多文件均须显式 next |
| XML UI | schema 3，必需，布局/页面/设置/菜单均由 XML/JS 定义 |
| 手动/自动档 | schema 8；demo content-version=8 |
| KPAK / profile / read | KPAK 2（流式）与 1（小包兼容）；profile/read 为 1 |

从 v0.8 起删除旧 story/flow 解析、声明式路由、无 XML UI 的兼容菜单、旧档导入、默认旧执行器和旧设置字段补齐。旧内容/槽位明确拒绝，不自动迁移或覆盖；旧格式只在 [v0.7.3 归档](doc/archive/v0.7.3-xml-js-api.zh-cn.md) 与历史 doc 中说明。settings.json 必须包含完整字段（含 skip_all、fullscreen）；坏文件提示后使用默认值。save 8 的 screenshot 字段必需，可为 null（无窗口工具快照），图像坏时拒绝。

## 启动

需要 Rust stable（最低 1.88）、MSVC C++ 工具及 Windows 系统媒体组件，依赖固定在 Cargo.lock。

```powershell
cargo run --locked
cargo run --locked -- --project games/demo --save-dir target/v081-dev-saves
cargo run --locked -- --project games/demo --renderer cpu --no-audio
cargo run --locked -- --check
cargo run --locked -- --check --renderer d3d11
```

打开设置页勾选“全屏显示”即可切换；窗口化默认 1280×720、最小 640×360。窗口位置在本次运行内恢复，不跨进程保存。工程监控 XML/JS 和素材，完整快照/脚本/资源/当前历史验证成功才替换；坏编辑保留旧状态。契约不兼容必须显式从入口重载，story_id 改变需重启。

## 本地打包

```powershell
cargo run --locked --bin kakera-pack -- games/demo target/v081-game.kpak --format 2
# 小包仍可显式生成 KPAK 1
cargo run --locked --bin kakera-pack -- games/demo target/v081-small.kpak --format 1
cargo run --locked -- --package target/v081-game.kpak --cubism-core runtime/live2d/windows-x64/Live2DCubismCore.dll --check
./tools/package.ps1
```

脚本默认生成 target/package/kakera-avg-v0.8.1，已存在则拒绝覆盖。包只含播放器、game.kpak、程序旁 Core、字体/模型/依赖许可和手册，不复制游戏散文件或用户数据。剧本链合并为包内 story.xml；KPAK 1/2 确定性、去重、Zstandard 和 SHA-256 检错，无加密；2 为独立分块，1 为 solid。输出必须在工程外，只有显式 --force 才原子替换资源包。

发布程序自动发现 exe 旁 game.kpak，只读包、不启动监控；坏包失败，不回退散文件。Windows 发布无控制台，启动错误用原生对话框。PowerShell 自动化应以管道或 Start-Process -Wait 等待 GUI 子系统的 --check 返回。

## 大体积资源与内存边界

KPAK 2 打开时只读取 80 字节头和尾部索引，按资源/块读取并解压，固定 1MiB 原文块；每个流仅保留当前块。索引及每块均有 SHA-256，块校验成功后才交给消费者。普通启动按需验证；`--check` 和打包自检扫描所有唯一块，能发现未使用资源的损坏。SHA-256 检错不提供签名或来源认证。

| 容器 | 包与逻辑总量 | 单文件 | 文件数 | 读取方式 |
| --- | --- | --- | --- | --- |
| KPAK 2 | 1TiB | 64GiB | 100000 | 常驻受限索引，独立分块按需读取 |
| KPAK 1 | 512MiB | 128MiB | 10000 | 整包解压到内存 |

KPAK 2 索引最多 128MiB，最多 1048576 唯一块；具体二进制契约见 [格式规范](doc/kpak-format.zh-cn.md)。这些是容器硬上限，不代表已经以 1TiB 实机验收。

工程和 KPAK 2 打包使用私有磁盘暂存与流式指纹，提交前复查源工程完整指纹；需要足够临时磁盘空间。`ContentSource.read` 仍限制 128MiB，流式 `open/copy_to` 最大 64GiB。视频流式复制到 Media Foundation 的临时文件，不再构造整视频 Vec；仍有 16 段、1920×1080、60fps、300 秒和 PCM 缓存预算。没有直接从包 seek 播放或网络流。

当前播放器仍在准备阶段解码所有已声明的图像/字体/音频/模型并准备视频，原解码缓存预算保持；本版消除了整包/整视频字节缓冲，并未实现所有资源随场景进入才解码。大游戏启动仍有资源校验与视频临时文件复制时间。

## 剧情、媒体与持久化

每句是独立完整 Frame；纯 Controller 通过隔离 QuickJS 计划验证并原子提交变量/目标/历史。按 Unicode 字素逐字，继续先跳转场、再补全、再推进；选项不默认选择。支持 Ctrl 只读/全部快进、自动播放、历史、CG、图片效果和立绘动作。菜单、提示、失焦和截图等待正确仲裁。

200 手动槽每页十个，自动槽十个独立轮换。剧情槽存位置、变量和历史检查点，加载完整重放；保存时间与 320×180 PNG 缩略图在同一 JSON 原子替换。settings.json/profile.json/read.json 独立于槽位，不随读档回退，排除出包与监控。已读最多 100000 条，文件 2MiB，超限不破坏旧文件；快进不开放 JS 已读查询。

Windows 原生 D3D11 负责窗口、滤镜与 Live2D 网格/蒙版合成，GPU 结果直接注册原生纹理；CPU fallback 保留，窗口仍由 D3D11/WARP 呈现。视频使用 Windows Media Foundation，Core 参数/形变仍为 CPU。GPU 失败报告并回退，设备失效最多一次 WARP 重建。

demo 为 30 场景/4 变量/3 JS/11 页/2 视频/1 模型。Haru/Core 来自用户本地 SDK，字体、模型和 Core 原许可随本地包保留，未公开上传；Core 只从受信任宿主配置/程序旁加载，发布禁止回退源码 runtime。资源统一走 ContentSource，游戏 JS 无文件/网络/原生库/时钟/随机/异步能力。

## 验证与限制

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked -- --check
```

当前策略见 [v0.8.1 落地文档](doc/v0.8.1-implementation-plan.zh-cn.md)，实际结果见 [v0.8.1 验收](doc/v0.8.1-validation.zh-cn.md)。完整契约见 [XML/JS API](doc/xml-js-api.zh-cn.md)，制作便携目录见 [工程/发布指南](doc/project-release-pack-guide.zh-cn.md)。旧版结果在历史文档中单独保留，不能当成本轮复测。

首发 Windows x64。未提供 Vulkan/Metal 原生后端、跨平台视频、任意 JS tick、CSS/DOM/富文本/回滚、网络流/seek/精确音画同步、Live2D physics3 求解/完整动作混合/自动口型/高级 offscreen。多显卡、高 DPI 跨屏、实际驱动丢失、设备声音与主观听感分别以验收记录为准。
