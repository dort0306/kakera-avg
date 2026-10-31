# v0.9.0 工程、发布与窗口模式指南

适用版本 0.9.0；唯一数据契约 story 8 / UI 3 / save 9；资源包为 KPAK 2（流式）并保留 KPAK 1 小包兼容。旧剧情/UI/存档格式解析与旧档导入已删除，历史指南在 [v0.7.3 归档](archive/v0.7.3-project-release-pack-guide.zh-cn.md)。Frame 视频已移除，视频只在 UI sequence 中；幕与页面以生命周期回调双向交接，隐藏交接使用 flow-checkpoint.json，详见 API 第 32 节。

## 工程运行

```powershell
cargo build --locked --release --bins
./target/release/kakera-avg.exe --project games/demo --check | Out-Host
./target/release/kakera-avg.exe --project games/demo --save-dir target/v083-development
```

--project 指定工程目录，默认源码启动为 games/demo。固定 story.xml，通过每份 XML 的 next.src 串联，末份 next end="true"；interface、scripts、assets 等路径以游戏根为准，不随章节目录变化。必须提供 UI 和全部 JS 注册；不读取 flow.xml，不运行旧 schema。

工程使用原生通知/去抖和固定快照候选准备。编辑 XML/JS/素材后完整验证当前变量/历史/条件/进入计划和资源，再原子替换。坏编辑保留旧会话/画面/声音；查看“工程状态”。不兼容候选只允许显式从入口重载，story_id 改变需重启。成功热更不写自动档/重复 CG 解锁，不重播瞬时动作。

## 窗口与设置

客户区固定 16:9，拖边/角同步约束；默认 1280×720，最小 640×360 逻辑尺寸，物理像素为 16×9 整数单位。标题栏和边框不参与游戏比例，最大化按钮禁用。

demo 设置页“全屏显示”由 XML/JS 调用 ctx.setting("fullscreen", bool)，保存成功后切当前显示器无边框全屏；关闭恢复进入前的大小和位置。fullscreen 默认 false，完整 settings.json 独立保存，重启按它恢复模式。非 16:9 显示器全屏仍等比留边，不改变系统分辨率。位置不跨进程保存。

每个新功能优先通过已有 XML/JS 组合；只有缺少宿主能力时才改 Rust，不按游戏页面新增原生窗口。

## 资源包与便携目录

```powershell
cargo run --locked --bin kakera-pack -- games/demo target/v083-publication/game.kpak
cargo run --locked --bin kakera-pack -- games/demo target/v083-publication/game.kpak --force --level 19
cargo run --locked -- --package target/v083-publication/game.kpak --cubism-core runtime/live2d/windows-x64/Live2DCubismCore.dll --check
./tools/package.ps1
```

kakera-pack 两个位置参数先于选项。支持 --format 1|2（默认 2）、--force、--level 1..22（默认 19）、--cubism-core DLL、--no-live2d。没有播放器的 --check/--renderer/--save-dir 参数；不带参数打印用法并失败退出。禁用 Core 校验不表示包中模型永久禁用，播放器仍须配置 Core 或显式 --no-live2d。

打包先记录引擎实际读取的依赖，建立只含发布文件的固定快照；合并 next 链到包内 story.xml，移除链上的源章节，UI include 另行展开为原入口路径的单个完整 UI，并移除实际子文件；JS/资源及其他显式附加文件保留逻辑路径，不改源工程。完整候选校验、包自检/独立加载和所选源文件指纹复查通过后原子写出；输出须在工程根外，已有输出没有 --force 就拒绝。KPAK 按逻辑路径排序、同字节去重、Zstandard 压缩与 SHA-256 损坏检测，未加密。2 使用独立 1MiB 原文块，1 保持整包 solid。素材不有损转码。默认只含引擎依赖、assets/licenses 与 licenses 目录的普通文件。其他文件须在根 kakera.package.json 以 schema 1/include 列表明确声明；不支持目录/通配，清单自身不入包。未引用的设计稿、模型权重和编辑器索引不会自动收集。

发布脚本默认 target/package/kakera-avg-v0.9.0，支持 -Project、-Output、-CompressionLevel、-Format 1|2（默认 2）。目标已存在（包括空目录）就拒绝，失败目录保留诊断；本地操作不公开上传。输出结构：

```text
kakera-avg-v0.9.0/
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

默认生成 KPAK 2；`kakera-pack games/demo target/small.kpak --format 1` 保留小包输出，发布脚本也支持 `-Format 1`。播放器按包头自动识别，不需额外播放参数；未知版本或坏包明确失败，不回退散文件。KPAK 1 的旧包可直接读取，包内内容仍须符合 story 8/UI 3/save 9。

2 打开时只读取头和索引，访问时验证并解压对应块，每个流只缓存当前 1MiB 原文块；`--check` 完整扫描唯一块。没有访问的坏块不保证在普通启动时发现。格式与损坏检测约定见 [KPAK 1/2 格式](kpak-format.zh-cn.md)。

工程候选和 KPAK 2 打包使用私有临时磁盘快照，不全量放入 RAM；需要足够磁盘空间。热更复查整个工程快照；打包只复制/复查所选发布源文件，同时验证依赖集合、清单和许可证列表未变化。KPAK 1 打包保留 512MiB 内存快照限制。视频流式复制到后端临时 MP4，不先构造整文件 Vec；已声明图像/音频/模型仍按既有预算预先解码，视频仍有尺寸/时长/音频缓存上限。不是所有解码资源的场景级惰性加载，也没有包内直接视频 seek 或网络流。

## 多文件 UI

在 story 的 interface.src 入口中写 `<include src="ui/settings.xml" />`，将变量/画廊/完整页面拆到根为 `<ui schema="3">` 的文件。子根只允许 schema；全局画布、initial 和 title/title-bind 由入口声明。include 仅在 UI 根级允许，路径始终相对游戏根，可继续嵌套，不能写在 screen/panel 内。

最多 128 文件/16 层，源 XML 累计与合并结果各 1MiB；重复/循环/大小写重复路径、错误 schema、未知字段、跨文件重复 ID 或绑定错误拒绝。所有页面与 UI 变量统一校验，子文件可引用其他文件声明的状态。

工程模式直接读取并监控子文件；kakera-pack 两种格式自动合并为一个完整 UI 入口并移除实际展开的子文件。默认 ui.xml 保留为 ui.xml，自定义入口路径原样保留；不要将自定义入口和另一个无关 XML 混为同一个输出。源文件不改写，额外声明的其他 XML 不因扩展名相同而被删除。无需额外打包开关，接口与示例见 [API 第 7.4 节](xml-js-api.zh-cn.md#74-外部-ui-xml-引入与打包合并)。

## 点九、图集与缺省场景 ID

源 .9.png 用资源和 UI 专用 nine-patch 标签，普通 image/panel/button 的 region/region-bind 复用图集；JS 用现有 ctx.setUi 切换。解析/纹理准备和越界校验都在提交前，不在绘制时切图或读文件。完整例子、density/padding 与约束见 API 第 26 节。

frame.id 可省略，改用稳定 name 作为作者入口/JS 引用；工程用该名字作临时内部 ID，两种格式打包时在候选插入随机 UUID v4 并保留 name，不改源 XML/JS。显式 ID 不变，无缺省 ID 的内容仍确定性打包。独立构建的 UUID 会变化，需提升 content-version；需要跨构建稳定存档应固定显式 id。工程的临时名字档与发布 UUID 档不自动迁移。详见 API 第 27 节。

## 窗口标题

在 ui.xml 根使用 title="游戏名"，或 title-bind="ui.windowTitle" 绑定 UI string；JS 用 ctx.setUi 修改。UI 文件随两种 KPAK 正常打包，不需要额外打包参数；标题不从宿主设置或剧情槽读取。细节见 API 第 28 节。

## 编辑器调试与发布附加文件

调试启动、认证帧和断点事务见 [调试 API v1](editor/10-player-debug-api.zh-cn.md)。必须显式 --project、--save-dir、--debug-listen 和 --debug-token-file；仅 loopback，发布/--check 拒绝。暂停冻结呈现，断点保留旧会话和完整候选，继续才提交。协议无远程 JS eval、行级断点或任意跳转。

需要额外随包发送作者文本时，在工程根写：

```json
{"schema":1,"include":["extras/credits.txt"]}
```

路径须指向现存普通文件，禁止越界、链接、大小写冲突；附加文件不是宿主可执行依赖。默认依赖包含所有已声明并被加载器读取的资源，不分析 JS 字符串来猜路径，也不以当前场景可达性删去其他场景资源。低层容器 API 仍可打包任意合法文件，默认收敛策略由 kakera-pack CLI 实施。更多限制见 API 第 29 节。

编辑器 AI/插件的接口和开发路线是规划，未作为本版可运行编辑器交付，见 [编辑器方案](editor/README.zh-cn.md)。

## 用户数据和限制

--save-dir 直接使用指定目录，不额外附 story_id。工程与发布默认同 story_id 会共享数据，验收建议隔离。当前手动 slot-1..200.json、自动 auto-1..10.json 为 save 9，加载完整重放历史与变量（含显式 resume_to 返回边）；旧格式或内容版本不匹配明确拒绝。设置、CG、已读、UI 音效、玩家键位与配色分别在 settings.json/profile.json/read.json/ui-audio.json/keybindings.json/ui-colors.json，不被槽覆盖；幕与页面交接使用固定隐藏 flow-checkpoint.json（不进入玩家槽位页和普通自动槽，不受 autosave 开关影响），成功返回写 inactive 墓碑。settings 必须完整包含当前字段，无缺字段迁移。screenshot 字段必需，可 null（工具快照）。

用户 JSON（settings.json/profile.json/read.json/ui-audio.json/keybindings.json/ui-colors.json/slot-*/auto-*/flow-checkpoint.json，含大小写变体）、.git/.idea/.vscode/target/saves/node_modules 和备份/临时文件不入包与工程监控；游戏内容勿使用保留用户文件名。路径统一 /，禁止绝对/盘符/../反斜杠/大小写碰撞/越界链接。KPAK 2 包及逻辑总量最多 1TiB、单文件 64GiB、100000 文件、索引 128MiB；KPAK 1 仍为包 512MiB、单文件 128MiB、10000 文件；剧本链最多 128 文件、源和合并 XML 各 16MiB。

数据检查不等于 GUI 或听感验收，结果和未覆盖项见 [v0.9.0 验收](v0.9.0-validation.zh-cn.md)。完整创作契约见 [API 手册](xml-js-api.zh-cn.md)。


v0.8.4 附加清单也拒绝保留用户文件/目录及临时文件（如 settings.json、read.json、keybindings.json、ui-colors.json、slot-/auto- JSON、saves/、target/、.git/、*.tmp）；不能用显式 include 绕过原有排除规则。v0.10.0 起键位/配色偏好同样只保存在玩家 --save-dir，不属于发布内容。

按钮 route="open:页面ID"/"close" 在两种包中保留，不需要额外打包开关；UI 拆分后目标仍全局校验，详见 API 第 30 节。


## UI 演出与发布检查

UI 根 ui-media 可跨 include 拆分；两格式仍汇总为单 UI 入口。音效候选、UI 视频引用的资源由 ContentSource 实际准备并入包。玩家 ui-audio.json 是独立偏好，不发布、不热更、不随剧情槽回退。发布前除 --check 外，检查演出自动触发、淡入淡出、失焦暂停、鼠标/空格跳过无穿透、视频返回及角色禁用后重启保持；听感单独记录。新增演出交互优先 XML/JS，接口与限制见 API 第 31 节。


## v0.9.0 生命周期与隐藏交接

应用版本 0.9.0，story 8/UI 3，save 9，demo content-version=9。frame 内 video 已删除，只用 UI sequence.video；幕/页面生命周期回调（v0.11.0 起为 `frame(id).enter/exit` 与 `ui(id).bind('screen.open'/'screen.close')`）与 ctx.present/resume 负责双向交接。flow-checkpoint.json 是固定隐藏自动槽，独立于 slot-* 和 auto-*，不显示、不发布、不监控；活动交接重启恢复 UI，视频从零播放，成功返回写 inactive 墓碑。发布前验证离幕插播、自然结束/跳过关闭与指定幕恢复、隐藏槽跨进程/原子失败/普通槽隔离。接口见 API 第 32 节，旧 frame 视频写法仅在归档手册保留。

## 多故事线入口

story.xml 可声明 `entry="opening secret-entry"`，入口可跨章节、使用 ID/name；两种包都完整保留，不按入口裁剪其他路线。ui.xml initial 应指向作者主菜单，由按钮显式 `ctx.restart("opening")` 或首次 `ctx.resume("opening")`，也可以读取手动/自动槽。启动不会自动选择第一个入口；多入口重开必须传目标。隐藏路线按钮和解锁条件仅用 XML/JS；入口声明不会自动生成菜单。demo content-version=11，旧 demo 剧情槽拒绝，共享 CG 保留。见 [多起始幕 API](xml-js-api.zh-cn.md#多起始幕与首次-ui-选择)。

## 确认弹窗与正常退出

确认页是普通 UI 3 模态 screen，发布时随 include 合并。demo 的 confirmation 页、可选标题与提示/按钮文字由 XML/JS 定义，RGBA alpha 控制遮罩/面板透明度；不要用系统提示窗口替代可定制游戏界面。退出按钮事件使用 ctx.quit()，可先经过确认页；无参、无路径或进程参数，播放器正常关闭并补写设置/已读。发布包无需新增原生插件或弹窗资源格式，字体和已有皮肤照常打包。两种容器与内容/存档版本不变。示例及限制见 [API](xml-js-api.zh-cn.md#自定义确认弹窗与退出游戏)。

## v0.10.1 国际化资源

默认游戏不启用；入口声明 localization 后，清单和全部语言的 FTL/字体/语音都经 ContentSource 准备验证，不根据打包机系统语言裁剪。KPAK 1/2 均保留 key、默认值、UI text-arg 及具名场景引用，源工程不改写。关闭时未引用 FTL 不自动入包。language.json（含大小写变体）属于玩家偏好，排除监控、默认依赖/显式附加和发布；不要将它写入清单 include。

宿主 zh-CN.ftl 编译进播放器，不要求游戏额外分发，也不接受游戏覆盖。package.ps1 默认输出 kakera-avg-v0.10.1，复制 licenses 中的 Fluent/Unicode 相关许可。语音仍使用现有有界预解码，不因国际化放宽 PCM/包预算。接口及生成演示见 API 第 36 节和 tools/prepare-localization-demo.ps1。
