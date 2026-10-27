# 04 当前引擎 API 参考

状态：**现有能力**，核对日期 2026-10-27。用于开发编辑器适配器，不能替代 [完整 XML/JS 手册](../../xml-js-api.zh-cn.md)。以下 Rust 名称为当前源码接口，不承诺跨版本稳定 ABI；编辑器应与引擎发布版本锁定。

## 1. 格式与入口

| 对象 | 当前契约 | 编辑器规则 |
| --- | --- | --- |
| 游戏入口 | 固定 story.xml，schema 8 | 不能导出旧 schema 或 flow.xml |
| 章节 | 每份一个 next.src 或 next end=true | src 按游戏根解释；链只负责合并，不生成剧情边 |
| XML UI | schema 3，必需 | 使用当前模板，不能依赖内置兼容菜单 |
| 剧情存档 | save 8；story_id/content_version/实际 frame ID/变量/历史 | 不把编辑器对象或调试指针塞入槽位 |
| profile/read | 各 schema 1，独立用户文件 | 测试与正式数据隔离 |
| 资源容器 | KPAK 2 默认，1 显式兼容 | 容器兼容不恢复旧内容/存档 |

story 根需要 id、content-version、entry 和 schema；包含 scripts、interface、assets、characters、visuals、variables、frames 等当前分组。后续章节根仅 schema，全链分组先合并再校验；未知元素/属性、命名空间、重复标识及错误引用拒绝。结构细节见主手册第 4、21 节和 [story_chain](../../../src/story_chain.rs)。

## 2. 场景与资源映射

| 内容 | 当前 API/语义 | 表单或生成器注意事项 |
| --- | --- | --- |
| frame | id 可省略；缺省时 name 必填；text 恰好一项且非空 | 编辑器新建显式稳定 ID，导入缺省 ID 保留并提示 |
| 作者别名 | entry、JS 注册/目标可解析 name 或实际 id | 名字不得与其他幕的 ID/name 冲突 |
| visual | 完整 background、最多三槽 portrait | 不支持 visual 继承或 frame 内局部覆盖 |
| portrait | character/image/slot 必填；pose 为最终姿态 | 槽仅 left/center/right；动作要匹配最终姿态 |
| options | 每幕内 option.id 唯一、正文非空 | 每个选项对应一个 JS 注册，无默认路由 |
| audio | bgm/voice/se 为 audio 资源 ID | 省略为停止/静音；相同连续 BGM 不重启 |
| video | asset 必填；skippable/advance 默认 true | 不可与 options 同幕；advance=true 要 Advance 路由 |
| Live2D | portrait 内 live2d 引用模型、动作/表情和参数 | 静态 image fallback 必需；Core 是受信任宿主依赖 |
| 转场/效果 | frame transition、visual/portrait/UI 的既有声明 | 使用主手册枚举和范围，不生成未知效果 |

资源表使用 image、nine-patch、font、audio、video、live2d，src 是游戏根内相对路径。普通 image 当前 PNG/JPEG；nine-patch 是带标记边框的源 .9.png；字体 OTF/TTF；音频 WAV/Ogg Vorbis；视频是否可解码取决于 Windows 系统媒体组件，当前实测为 H.264/AAC MP4。

点九使用资源/UI 专用标签，不能拿普通 image 标签替代；每轴 1–8 个伸缩段，内容区至多一段，density 为 0.125–8。图集继续普通图片资源，image/panel/button 使用 region 或 region-bind（互斥），源像素矩形 x/y 非负、宽高正、不得越界；绑定仅 UI string，修改后只换 UV。见主手册第 26 节和 [image_layout](../../../src/image_layout.rs)。

## 3. 剧情 JS 契约

| 入口 | 回调行为 | 不能做的事 |
| --- | --- | --- |
| on.advance(frameRef, callback) | 同步返回目标幕引用 | 不能返回 Promise 或缺失目标 |
| on.choice(frameRef, optionId, callback, when?) | callback 返回目标；when 返回 boolean | 不能为隐藏选项伪造选择，不混合 Advance/End |
| on.end(frameRef) | 注册结束幕 | 继续不默认推进 |

全部 frame 必须覆盖且路由种类与 XML options 一致。条件在更新前变量上计算；主回调收集有序更新；目标条件也通过后才提交变量、位置与历史。动态目标只能在执行边界验证，启动检查不穷举全部路径。

| 剧情 ctx | 类型/语义 |
| --- | --- |
| frameId / optionId | name 优先的场景引用 / 选项 ID 或 null |
| vars | 只读草稿视图，int 是 BigInt |
| set(name, value) | 已声明变量赋值，类型/长度/范围校验 |
| add(name, amount) | 仅 int，i64 与声明范围校验 |

主剧情 ctx 没有 setUi、save、open 或任意宿主调用。预先保存的 vars 快照不会被后续更新改写；需要草稿新值时重新读取。每次求值隔离重建 QuickJS，闭包与全局变量不跨事件保存。

## 4. UI XML 与 JS 契约

UI 设计画布为至少 640×360 的 16:9。页面包括 scene、panel、image、nine-patch、label、button、slider、toggle、repeat。位置/基础样式/页面放 XML；交互放 JS。图片、视频、模型均通过资源 ID，不能绑定任意路径。

| 配置 | 当前行为 |
| --- | --- |
| UI root initial | 存在且非模态的 screen |
| screen modal/pause | 页面栈和暂停；仅顶层接收输入 |
| background-click/right-click | 顶层背景事件，控件/黑边/浮层等阻挡 |
| image region-bind | UI string，用 setUi 修改后整批校验 |
| root title/title-bind | 静态或 UI string 窗口标题，互斥；默认 Kakera AVG |
| fullscreen 设置 | bool，独立设置事务后由原生宿主切换 |

窗口标题最多 256 个 Unicode 标量值，非纯空白、无控制字符；不进入剧情档/settings。读档/重开保留 UI 变量，再执行 game.enter。详见主手册第 28 节。

所有 UI 回调使用 on.ui(eventName, callback)，同步返回 undefined；字段为 game/settings/saves、vars/ui、item/value 的只读事件开始快照。UI 中没有剧情 ctx 的 set/add/frameId，应使用 game.frameId。以下只列调用面，详细参数范围以主手册对应节为准。

| 命令组 | 已有方法 | 提交约束 |
| --- | --- | --- |
| 页面与局部值 | open、close、setUi、message | 页面/类型/范围/绑定严格校验 |
| 剧情 | advance、choose、restart、skipVideo | 至多一种剧情/IO 操作，不开放任意 jump |
| 存档 | save、load、loadAuto、slotsPage | 手动 1–200、自动 1–10；overwrite 显式控制 |
| 偏好 | setting | text_speed、音量、muted、autosave、skip_all、fullscreen、auto_delay 白名单 |
| CG | unlock | 已声明 CG，多项可聚合 |
| 呈现 | transition、animate、portrait、live2d、autoPlay | 各自目标/时机/枚举校验，不产生通用媒体 IO |

game.enter 是唯一特殊 UI 生命周期入口：可 unlock/setUi/message 和受约束转场、图片/立绘/模型动作；不允许在里面保存、读档、跳转、设置或开启自动播放。候选状态先验证，错误保留原 Session/音频/呈现。历史重放不执行它。

普通 UI 事件不能混合 save+setting、load+restart、advance+choose 等多种 IO/剧情操作；多个设置或多个解锁可分别聚合。setUi 只是排队，不改变同一回调里 ui 快照，生成器必须用局部值继续计算。

## 5. 当前 Rust 接入点

| 源文件 | 现有入口 | 编辑器用途/限制 |
| --- | --- | --- |
| [content](../../../src/content.rs) | ContentSource.read/open/copy_to；DirectorySource::new；validate_path | 统一逻辑资源来源；编辑器不要泄漏物理路径给游戏规则 |
| [story_chain](../../../src/story_chain.rs) | compile；CompiledStory.xml/files；diagnostic；publication_xml | 合并、源位置和发布 UUID；不是剧情排序器 |
| [loader](../../../src/loader.rs) | load_game(source) → LoadedStory + ScriptLogic；initial_ui_plan | 内容/注册/入口条件和初始 UI 计划校验，不做全部资源解码 |
| [runtime](../../../src/runtime.rs) | Bundle::load(source, LoadOptions) | 资源/媒体/模型准备；LoadOptions 仅 core、no_live2d |
| [script](../../../src/script.rs) | ScriptLogic::load、ui_handlers、ui_event；model::Logic 实现 | 使用现有隔离和预算；不在编辑器高权限 JS 环境替代执行 |
| [model](../../../src/model.rs) | Story::start、resolve_frame、validate_session_with；Logic::resolve/options | 领域与条件查询，编辑器不伪造旧档 |
| [controller](../../../src/controller.rs) | transition_with(story, session, Intent, logic) | Advance/Choose，返回 session 与显式 entered；只有成功才有新状态 |
| [interface host](../../../src/interface/host.rs) | State::new；prepare(def,state,settings,profile,commands,entering) | 纯 UI Plan；不是执行文件 IO 的 API |
| [assets](../../../src/assets.rs) | PreparedAssets::load_from/load_deferred；validate_interface | 解码/绑定几何校验，避免另写不一致图片裁剪规则 |
| [development](../../../src/development.rs) | ProjectMonitor::new/poll；compatible_session | 原生监控与完整候选；不是可远程订阅的调试服务 |
| [package](../../../src/package.rs) | Snapshot::directory/for_publication/encode；Archive::open/verify_all | 1 小包与 1/2 读取；不要直接以私有容器结构编写编辑器 |
| [staged](../../../src/package/staged.rs) | StagedProject::capture/for_publication/ensure_unchanged | 2 的固定磁盘快照与源指纹 |
| [streaming](../../../src/package/streaming.rs) | write_streaming | 编码/验证候选后原子发布，失败保留旧文件 |
| [save](../../../src/save.rs) | Snapshot::capture_with/restore_with；SaveStore | 测试路线验证，不能以调试快照冒充正式槽位 |

Bundle 会准备全部已声明资源，可能耗时/占用大量缓存；不适合每敲一个字符就完整调用。编辑器区分轻量解析、批量资源准备与真实试玩校验。它只保证入口计划和资源准备，不能证明每个动态 JS 分支永远正确。

## 6. 已有命令行

播放器 `kakera-avg.exe`：

| 参数 | 语义 |
| --- | --- |
| --project 目录 | 工程模式，监控与候选热更 |
| --package 包文件 | 发布模式，只读 KPAK 1/2；与 project 互斥 |
| --check | 完整资源检查后退出；包模式扫描全部唯一块 |
| --renderer auto/d3d11/cpu | 渲染选择；cpu 在 Windows 仍经 D3D11/WARP 呈现 |
| --no-audio | 禁设备输出但仍解码检查 |
| --no-live2d | 显式 fallback，不等于完整 Core 校验 |
| --cubism-core DLL | 受信任宿主 Core 路径 |
| --save-dir 目录 | 直接指定用户数据目录，不自动追加 story_id |
| --help / -h | 用法 |

默认优先 exe 旁 game.kpak；源码运行才可能回退 demo 工程。发布坏包不得回退目录。当前没有 --start-frame、--debug、--check-json、运行态查询或远程控制入口。

`kakera-pack.exe` 的两个位置参数为工程目录、输出包，之后支持 --format 1/2、--force、--level 1..22、--cubism-core、--no-live2d。默认格式 2/级别 19；输出必须在工程根之外，覆盖需 force。它没有播放器的 renderer/check/save-dir 参数。

`tools/package.ps1` 参数为 Project、Output、CompressionLevel、Format；需要源码工具链，构建 release 后组装固定宿主 Core 和许可证，已存在输出目录会拒绝。不能直接当作不带 Rust 环境的最终编辑器发行器；产品需随附匹配的工具，并实现等价受控组装。

Windows 播放器为 GUI 子系统。自动化必须等待进程退出并捕获退出码/重定向输出，不能以 PowerShell 启动语句返回就当检查完成。当前诊断主要为文本/JS 栈，不存在稳定 JSON 诊断协议。

## 7. 重要预算

| 项目 | 当前上限或行为 |
| --- | --- |
| 剧本链 | 128 文件、XML 合计 16MiB |
| JS | 1–32 文件、源码合计 1MiB；每求值 200ms/10000 次中断检查 |
| QuickJS | 堆 16MiB、栈 256KiB |
| 回调 | 剧情最多 128 更新，UI 最多 32 命令 |
| 变量 | 剧情/UI 各 128；int 为 i64，string 4096 UTF-8 字节 |
| UI | XML 1MiB，1–64 页面、1024 节点、深度 8、栈 8 |
| 普通图像 | 单边 8192；滤镜额外缓存 256MiB |
| 音频 | 单段 300 秒、PCM 单资源 128MiB、总量 256MiB |
| 视频 | 最多 16，1920×1080、约 60fps、300 秒；音轨总 PCM 256MiB |
| Live2D | 最多 8 模型；模型/纹理/动作等细分预算见主手册 19.7 |
| ContentSource | read 128MiB；open/copy_to 64GiB，解码预算另外检查 |
| KPAK 1 | 包/逻辑总量 512MiB，单文件 128MiB，10000 文件 |
| KPAK 2 | 包/逻辑总量 1TiB，单文件 64GiB，100000 文件；1MiB 独立块，索引 128MiB |
| 历史/存档 | 最多 1000 历史；save JSON 2MiB；缩略图 PNG 320×180/压缩最多 256KiB |

这些上限不是建议规模或性能保证。编辑器应提前预警，但最终校验仍以引擎为准。更细的资源规则见 [KPAK 规范](../../kpak-format.zh-cn.md)、主手册第 14/19/26 节及对应源码。
