# Kakera AVG Rust · v0.9.1

Rust、QuickJS、XML/JavaScript 驱动的 Windows x64 AVG 播放器。界面、布局和交互优先用 XML/JS；只有现有能力无法满足需求时才扩展受约束的 Rust 宿主能力，这一规则已写入 AGENTS.md。

窗口支持固定 **16:9 客户区缩放**，拖动窗口边/角不产生比例黑边；设置页可切换无边框全屏，关闭后恢复进入前窗口大小和位置。全屏开关用 XML toggle 和 `ctx.setting("fullscreen", bool)`，默认窗口化，独立设置跨进程保存。非 16:9 显示器全屏仍保持等比画面并留边；不拉伸或裁掉 UI。

v0.8.1 新增面向大体积游戏的 **KPAK 2 流式资源包**，默认打包为 2；播放器保留 KPAK 1 小包兼容。资源包版本独立于剧情/UI/存档，不恢复旧 story/flow/存档解析器。

v0.8.2 新增源 `.9.png` 点九背景、共享图集区域和打包 UUID。资源/UI 使用专用 `<nine-patch>`；普通图片通过 `region="x,y,w,h"` 或 `region-bind="ui.xxx"` 选取图集，JS 复用 `ctx.setUi` 切换，不重新上传图片。demo：效果演示 → 点九 / 图集。

frame.id 可省略，省略时写稳定 `name` 供 entry/JS 引用；打包候选生成 UUID v4 并保留名字映射，源 XML 不改写。随机 ID 会随独立构建改变；要保持跨构建档案契约请显式固定 id，否则提升 content-version。细节见 API 第 26–27 节。

窗口标题在 ui.xml 根配置 `title="我的游戏"`，或用 `title-bind="ui.windowTitle"` 绑定 UI string，再通过 `ctx.setUi("windowTitle", "新的标题")` 修改。默认 Kakera AVG；标题最多 256 个 Unicode 字符，非空且无控制字符，整批验证后提交，不进入剧情档/设置。demo 在“效果演示 → 点九 / 图集”提供切换按钮。详见 API 第 28 节及 [标题功能验收](doc/window-title-validation.zh-cn.md)。

v0.8.3 新增工程调试 API：外部工具可查询运行状态、设置场景断点、暂停/继续和受约束步进。打包改为收集引擎实际依赖、约定许可证和显式附加文件，未引用的设计稿、AI 权重或编辑器元数据不自动入包。两项能力不增加游戏脚本的网络或文件权限。

编辑器仍处于规划阶段，AI 改为用户可配置的通用 Chat Completions 兼容 API，可连接自部署或远程服务，不固定模型。比较后建议优先实现 VS Code 插件，提供“一幕一个 frame”的可视编辑、资源管理和 DAP 适配；详细方案见 [编辑器文档](doc/editor/README.zh-cn.md) 与 [平台比较](doc/editor/09-editor-platform-comparison.zh-cn.md)。

UI XML 支持根级 `<include src="ui/settings.xml" />`，可将状态、画廊和完整页面拆到 `<ui schema="3">` 子文件。路径相对游戏根目录，支持嵌套引入；全局统一校验，拒绝重复/循环与 ID 冲突。KPAK 1/2 打包自动汇总到一个完整 UI 入口（通常 ui.xml），不带子 UI 碎片，不改源工程。demo 已按页面拆分；写法、预算与限制见 [API 第 7.4 节](doc/xml-js-api.zh-cn.md#74-外部-ui-xml-引入与打包合并)，结果见 [UI 拆分验收](doc/ui-split-validation.zh-cn.md)。

按钮支持 `route="open:页面ID"` 与 `route="close"`，简单页面导航无需 JS 回调；与 event 互斥，复用原页面栈及事务校验。见 [API 第 30 节](doc/xml-js-api.zh-cn.md#30-v084-按钮-ui-路由) 和 [v0.8.4 验收](doc/v0.8.4-validation.zh-cn.md)。

UI 演出支持自动/JS 触发的顺序淡入淡出、UI 视频覆盖层及单次结束路由，点击或空格可跳过。HUD 的“演出与角色音效”页提供片头返回原页、片尾返回剧情界面、单音效/随机音效与角色开关示例。音效候选先过滤再随机，全排除静音；偏好独立保存在 `ui-audio.json`，不随读档回退，仅控制 UI 音效。演出进度不入档；同时只支持一个 UI 演出视频，首期不支持并行轨道。详见 [API 第 31 节](doc/xml-js-api.zh-cn.md#31-ui-演出过场视频与角色音效)、[计划](doc/ui-presentation-implementation-plan.zh-cn.md) 与 [验收](doc/ui-presentation-validation.zh-cn.md)。

v0.9.0 将视频统一到 UI，frame 不再接受 video。新增 on.frameEnter/Exit、on.screenOpen/Close 与 ctx.present/resume，可在离幕时自动播放 UI 视频，结束后续接指定幕。隐藏 flow-checkpoint.json 独立保存剧情/待续点和 UI，既不显示在玩家槽位页，也不占普通自动槽；重启恢复演出从零播放。save 9、demo content-version=9，旧档拒绝且不迁移。见 [API 第 32 节](doc/xml-js-api.zh-cn.md#32-v090-幕与页面生命周期隐藏交接槽)。

## 当前数据契约

| 对象 | 当前值 |
| --- | --- |
| story | schema 8；固定 story.xml，单/多文件均须显式 next |
| XML UI | schema 3，必需；可多文件 include，打包合并，布局与交互仍由 XML/JS 定义 |
| 手动/自动档 | schema 9；demo content-version=11 |
| KPAK / profile / read | KPAK 2（流式）与 1（小包兼容）；profile/read 为 1 |

从 v0.8 起删除旧 story/flow 解析、声明式路由、无 XML UI 的兼容菜单、旧档导入、默认旧执行器和旧设置字段补齐。旧内容/槽位明确拒绝，不自动迁移或覆盖；旧格式只在 [v0.7.3 归档](doc/archive/v0.7.3-xml-js-api.zh-cn.md) 与历史 doc 中说明。settings.json 必须包含完整字段（含 skip_all、fullscreen）；坏文件提示后使用默认值。save 9 的 screenshot 字段必需，可为 null（无窗口工具快照），图像坏时拒绝。

## 启动

需要 Rust stable（最低 1.88）、MSVC C++ 工具及 Windows 系统媒体组件，依赖固定在 Cargo.lock。

```powershell
cargo run --locked
cargo run --locked -- --project games/demo --save-dir target/v083-dev-saves
cargo run --locked -- --project games/demo --renderer cpu --no-audio
cargo run --locked -- --check
cargo run --locked -- --check --renderer d3d11
```

启动先显示 XML 初始 UI，demo 可选择“从午后开始”“另一条故事线”或载入存档。`story entry="opening secret-entry"` 声明多个起始幕；首次通过 `ctx.resume("opening")` / `ctx.restart("opening")` 显式选择，或 `ctx.load` / `ctx.loadAuto` 读档，不自动进入列表第一幕。选择前没有剧情生命周期、剧情音频、自动档和已读副作用。运行中 `resume` 沿用续接变量/历史，`restart(frame)` 清空剧情进度；多起始幕不允许无参 restart。布局、路线显隐和解锁条件仍由 XML/JS 组合。详见 [多起始幕策略](doc/multiple-entries-implementation-plan.zh-cn.md)。demo content-version=11，旧剧情槽明确拒绝，共享 profile 不撤销。

打开设置页勾选“全屏显示”即可切换；窗口化默认 1280×720、最小 640×360。窗口位置在本次运行内恢复，不跨进程保存。工程监控 XML/JS 和素材，完整快照/脚本/资源/当前历史验证成功才替换；坏编辑保留旧状态。契约不兼容必须显式从入口重载，story_id 改变需重启。

## 本地打包

```powershell
cargo run --locked --bin kakera-pack -- games/demo target/v083-game.kpak --format 2
# 小包仍可显式生成 KPAK 1
cargo run --locked --bin kakera-pack -- games/demo target/v083-small.kpak --format 1
cargo run --locked -- --package target/v083-game.kpak --cubism-core runtime/live2d/windows-x64/Live2DCubismCore.dll --check
./tools/package.ps1
```

脚本默认生成 target/package/kakera-avg-v0.9.1，已存在则拒绝覆盖。包只含播放器、game.kpak、程序旁 Core、字体/模型/依赖许可和手册，不复制游戏散文件或用户数据。剧本链合并为包内 story.xml；KPAK 1/2 确定性、去重、Zstandard 和 SHA-256 检错，无加密；2 为独立分块，1 为 solid。输出必须在工程外，只有显式 --force 才原子替换资源包。

发布程序自动发现 exe 旁 game.kpak，只读包、不启动监控；坏包失败，不回退散文件。Windows 发布无控制台，启动错误用原生对话框。PowerShell 自动化应以管道或 Start-Process -Wait 等待 GUI 子系统的 --check 返回。

可选工程根清单 `kakera.package.json`：

```json
{"schema":1,"include":["extras/credits.txt"]}
```

默认只收集 XML/JS 及已声明资源的实际依赖（包括模型辅助文件），另含 `assets/licenses/`、`licenses/` 的普通文件。清单只接受游戏内明确文件，不支持目录或通配；清单自身不入包。附加文件由作者负责选择，不应填写密钥或权重。完整规则见 API 第 29 节。

## 工程调试

```powershell
([guid]::NewGuid().ToString('N') + [guid]::NewGuid().ToString('N')) |
  Set-Content -Encoding ascii target/debug-token.txt
./target/debug/kakera-avg.exe --project games/demo --save-dir target/debug-saves `
  --debug-listen 127.0.0.1:19200 --debug-token-file target/debug-token.txt --no-audio
```

必须显式指定工程、隔离数据目录和令牌文件，仅允许 loopback；不能与 --package/--check 混用。启动即暂停并关闭本次自动保存。协议为经过认证的 TCP JSON Lines v1；场景断点在候选全部验证成功、提交之前命中，继续只提交一次。不提供 JS 行级调试或任意变量修改。请求格式、修订与重试规则见 [播放器调试 API](doc/editor/10-player-debug-api.zh-cn.md)。

## 大体积资源与内存边界

KPAK 2 打开时只读取 80 字节头和尾部索引，按资源/块读取并解压，固定 1MiB 原文块；每个流仅保留当前块。索引及每块均有 SHA-256，块校验成功后才交给消费者。普通启动按需验证；`--check` 和打包自检扫描所有唯一块，能发现未使用资源的损坏。SHA-256 检错不提供签名或来源认证。

| 容器 | 包与逻辑总量 | 单文件 | 文件数 | 读取方式 |
| --- | --- | --- | --- | --- |
| KPAK 2 | 1TiB | 64GiB | 100000 | 常驻受限索引，独立分块按需读取 |
| KPAK 1 | 512MiB | 128MiB | 10000 | 整包解压到内存 |

KPAK 2 索引最多 128MiB，最多 1048576 唯一块；具体二进制契约见 [格式规范](doc/kpak-format.zh-cn.md)。这些是容器硬上限，不代表已经以 1TiB 实机验收。

工程热更继续校验完整快照；打包使用私有暂存和流式指纹，只复制并复查发布依赖、附加文件及清单。两者都需要足够临时磁盘空间。`ContentSource.read` 仍限制 128MiB，流式 `open/copy_to` 最大 64GiB。视频流式复制到 Media Foundation 的临时文件，不再构造整视频 Vec；仍有 16 段、1920×1080、60fps、300 秒和 PCM 缓存预算。没有直接从包 seek 播放或网络流。

当前播放器仍在准备阶段解码所有已声明的图像/字体/音频/模型并准备视频，原解码缓存预算保持；v0.8.1 消除了整包/整视频字节缓冲，并未实现所有资源随场景进入才解码。大游戏启动仍有资源校验与视频临时文件复制时间。

## 剧情、媒体与持久化

每句是独立完整 Frame；纯 Controller 通过隔离 QuickJS 计划验证并原子提交变量/目标/历史。按 Unicode 字素逐字，继续先跳转场、再补全、再推进；选项不默认选择。支持 Ctrl 只读/全部快进、自动播放、历史、CG、图片效果和立绘动作。菜单、提示、失焦和截图等待正确仲裁。

200 手动槽每页十个，自动槽十个独立轮换。剧情槽存位置、变量和历史检查点，加载完整重放；保存时间与 320×180 PNG 缩略图在同一 JSON 原子替换。settings.json/profile.json/read.json/ui-audio.json 独立于槽位，不随读档回退，排除出包与监控。幕与页面交接使用固定隐藏 flow-checkpoint.json（schema 1，最多 2MiB），不显示在槽位页、不占普通自动槽、不受自动保存开关影响，成功返回后写 inactive 墓碑。已读最多 100000 条，文件 2MiB，超限不破坏旧文件；快进不开放 JS 已读查询。

Windows 原生 D3D11 负责窗口、滤镜与 Live2D 网格/蒙版合成，GPU 结果直接注册原生纹理；CPU fallback 保留，窗口仍由 D3D11/WARP 呈现。视频使用 Windows Media Foundation，Core 参数/形变仍为 CPU。GPU 失败报告并回退，设备失效最多一次 WARP 重建。

demo 为 30 场景/4 变量/3 JS/15 页/2 视频/1 模型。Haru/Core 来自用户本地 SDK，字体、模型和 Core 原许可随本地包保留，未公开上传；Core 只从受信任宿主配置/程序旁加载，发布禁止回退源码 runtime。资源统一走 ContentSource，游戏 JS 无文件/网络/原生库/时钟/随机/异步能力。

## 验证与限制

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked -- --check
```

当前策略见 [v0.9.0 落地文档](doc/v0.9.0-implementation-plan.zh-cn.md)，实际结果见 [v0.9.0 验收](doc/v0.9.0-validation.zh-cn.md)。完整契约见 [XML/JS API](doc/xml-js-api.zh-cn.md)，制作便携目录见 [工程/发布指南](doc/project-release-pack-guide.zh-cn.md)。旧版结果在历史文档中单独保留，不能当成本轮复测。

首发 Windows x64。未提供 Vulkan/Metal 原生后端、跨平台视频、任意 JS tick、CSS/DOM/富文本/回滚、网络流/seek/精确音画同步、Live2D physics3 求解/完整动作混合/自动口型/高级 offscreen。多显卡、高 DPI 跨屏、实际驱动丢失、设备声音与主观听感分别以验收记录为准。

发布目录中的源码引用链接供仓库开发环境查阅；本地发行目录不携带 Rust 源码或游戏散文件。

v0.9.1 修复末幕 frameExit：文字完成后手动继续触发一次，UI 演出可恢复末幕或显式返回其他幕；自动播放与 Ctrl 仍在结尾停止。普通读档/重开重置退出状态。数据契约保持 story 8/UI 3/save 9。

修复策略与本轮结果见 [v0.9.1 计划](doc/v0.9.1-implementation-plan.zh-cn.md) 和 [v0.9.1 验收](doc/v0.9.1-validation.zh-cn.md)。

UI 唤起剧情：控件 on.ui 或自动演出 screenClose 中用 `ctx.resume("幕ID或name")` 指定起始幕，用 `ctx.resume()` 从磁盘隐藏槽 continuation 恢复（包括 inactive）。首次无槽须指定起始幕；指定幕沿用变量/历史，不清空变量。成功唤起后同步隐藏槽，失败保留原状态。布局仍使用 XML，交互仍使用 JS。

专用 CG：在 assets 声明 `<cg id="作者标记" src="…"/>`，visual 用 `cg="作者标记"` 显示完整 CG，不与背景/人物混用。专用资源自动加入画廊，`game.cgId` 提供当前标记；在现有 game.enter 回调中 `if (ctx.game.cgId) ctx.unlock(ctx.game.cgId)` 可在进入时解锁。`cg.progress` 显示共享解锁数量，profile.json 独立于各剧情槽，同标记只计一次。demo 增加纯 CG 结尾示例，content-version 升为 10，旧剧情槽明确拒绝；save 9/profile 1 不变。详见 [CG 策略](doc/cg-resources-implementation-plan.zh-cn.md)。
CG 的自动检查与实际窗口结果见 [CG 验收记录](doc/cg-resources-validation.zh-cn.md)。

多起始幕已完成 D3D11/CPU 真实窗口复验：双路线选择、第 200 槽跨进程加载、变量/历史、自动档恢复及 UI 视频返回通过，并修复主菜单切 HUD 曾被误判为交接而暂停台词的问题。当前完整回归 189 项通过；设备音频和听感未验收，详见 [GUI 复验记录](doc/multiple-entries-validation.zh-cn.md#gui-复验与修复2026-10-29)。
