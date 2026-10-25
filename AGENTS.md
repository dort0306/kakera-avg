# 项目协作约定

## 目标与依据

先阅读当前 `doc/v0.8.2-implementation-plan.zh-cn.md`、`doc/v0.8.2-validation.zh-cn.md`，再阅读 `doc/v0.8.1-implementation-plan.zh-cn.md`、`doc/v0.8.1-validation.zh-cn.md`，再阅读 `doc/v0.8-implementation-plan.zh-cn.md`、`doc/v0.8-validation.zh-cn.md`，历史依据再阅读 `doc/v0.7.3-implementation-plan.zh-cn.md`、`doc/v0.7.3-validation.zh-cn.md`，再阅读 `doc/v0.7.2-implementation-plan.zh-cn.md`、`doc/v0.7.2-validation.zh-cn.md`、`doc/v0.7.2-maintenance.zh-cn.md`、`doc/v0.7.1-implementation-plan.zh-cn.md`、`doc/v0.7.1-validation.zh-cn.md`，再阅读 `doc/v0.7-implementation-plan.zh-cn.md`、`doc/v0.7-validation.zh-cn.md` 与 `doc/kpak-format.zh-cn.md`，再阅读基线 `doc/v0.6-implementation-plan.zh-cn.md`、`doc/xml-js-api.zh-cn.md` 与 `doc/v0.6-ui-validation.zh-cn.md`，再阅读历史 `doc/rust-mvc-plan.zh-cn.md`、`doc/v0.2-implementation-plan.zh-cn.md`、`doc/v0.3-implementation-plan.zh-cn.md`、`doc/js-trigger-implementation-plan.zh-cn.md`、`doc/v0.4-implementation-plan.zh-cn.md`、`doc/effects-implementation-plan.zh-cn.md` 和原始数据示例，再检查代码与测试。`doc/rust-mvc-example` 保留 v0.1，`doc/v0.3-example` 保留声明式规则，`doc/v0.3.1-example` 保留 schema 4 JS 契约；可运行样例在 `games/demo`。用户已要求 JavaScript/QuickJS 触发器与 XML/JS 自定义 UI；界面位置、基础样式与菜单放 XML，交互放 JS，不要求游戏作者编写 Rust。不提前引入 ECS、异步运行时、消息总线或插件框架。

从 v0.8 起，每个新需求必须优先评估能否通过已有 XML 与 JavaScript 能力组合实现；能实现就只改 XML/JS。只有现有宿主能力确实无法满足需求时，才扩展 Rust 的明确命令、校验或平台能力，并在策略中说明原因。

## 架构约束

- `model` 只含领域数据和只读查询，不引用 egui、文件 IO、窗口和 GPU。
- `controller` 仅依赖 `model`，保持纯函数；只有验证成功才返回新 Session。无效选项不跳到默认分支。
- `model::Logic` 是纯计划接口；`script` 适配 QuickJS，不能把解释器对象放进 Model 或存档。回调每次隔离执行；只通过 ctx.set/add 收集更新，Rust 校验并原子提交。条件回调只读，异步任务、时钟、随机和宿主 IO 不开放；资源预算不得绕过。
- 条件与分支在隔离 QuickJS 中实现；Rust 的 `model/rules` 只验证变量并应用更新计划。更新、目标、历史原子提交；类型错误、溢出、越界或隐藏选项均失败。
- `loader/save` 引用 `model`；`view` 读取 `model/assets`，返回 UI 意图；`app` 执行意图和 IO。
- 每个 Frame 是可独立恢复的完整显示状态，不隐式沿用上一句；visual 在加载阶段展开，不链式继承。
- XML 节点和借用不离开加载器；内容使用拥有的 String。静态纹理缓存由 assets/view 持有，不逐帧读盘/解码/上传。视频帧与 Live2D 动态纹理只在 App 更新阶段生成和上传，绘制只读。
- 不在绘制过程中推进剧情、执行脚本或保存文件；单次 UI 更新最多提交一个意图。
- 转场与图片效果依据 `doc/effects-implementation-plan.zh-cn.md`；配置在 Model，时间/旧画面快照在 App 呈现层，指针不入档。所有目标条件、生命周期命令及 IO 成功后才启动效果；读档直接显示最终态。转场时不积累逐字时间，继续先跳过转场；菜单/失焦暂停。滤镜在准备阶段缓存，额外上限 256MiB；D3D11 后端用 HLSL，CPU fallback 保留原算法。不在绘制时做 IO/解码/资源创建。
- `interface` 使用拥有的数据和严格 XML 解析；渲染只读绑定缓存，返回事件。`on.ui` 收集受约束命令，整批验证后由 App 执行至多一种剧情/IO 操作；设置或 CG 解锁同类可聚合。不开放任意跳转、路径或 IO。新增页面与已有能力组合仅修改 XML/JS，新增宿主能力才修改 Rust 命令及校验。
- 页面栈/UI 变量为瞬时状态，不进入剧情槽；只允许顶层交互，菜单暂停不积累时间。`game.enter` 允许解锁、UI 局部赋值、提示和受约束 transition/animate，候选状态提交前验证；历史重放不触发它，失败必须保留旧会话、音频与效果。transition 只在进入事件可用；animate 不接受 repeat 模板目标。
- 可用选项在提交边界求值并缓存；目标条件报错也必须保留旧 Session 与音频。View 只读缓存，不能逐帧运行 JS。
- 历史是 Session 的显式状态；逐字时钟与音频播放指针不入档。控制器返回显式 entered，不能依赖 frame_id 变化判断自循环是否进入场景。
- 逐字按 Unicode 字素簇推进，第一下继续补全，第二下才推进；未完成时不开放选项。暂停期间不得积累显示时间。
- 音频只在明确进入事件执行，连续相同 BGM 不重启，语音/音效在每次进入时替换；成功读档强制重建。不得在绘制或逐字 tick 中重复播放。

## 内容与存档

- 保持严格 XML 校验：未知元素/属性、重复 ID、错误资源类型、缺失引用、选项路由不一致必须报错，并保留文件名与位置。
- 场景 ID 是存档契约，不用 XML 序号代替。修改不兼容内容要显式提升 content-version，不悄悄接受旧档。
- 坏档不得修改当前 Session。写档采用同目录临时文件与平台原子替换；禁止先删旧档再改名。
- v0.8 为唯一正式基线：story 8 / UI 3 / save 8，demo content-version=8；profile 1/read 1 为唯一现行格式；v0.8.1 用户明确要求同时支持 KPAK 2 与小包 KPAK 1，仅包容器保留兼容。删除旧格式解析器、flow 规则、兼容菜单、旧档导入和旧字段补齐，历史只保留文档说明。
- schema 8 保存 generation、当前位置、变量与历史起点检查点；历史最多 1000 条，裁剪推进检查点，读档完整重放条件/更新/目标与终值。手动槽 1–200，十槽一页/20 页，只扫描当前页元信息；真正加载再完整验证。
- 自动槽 auto-1…10 独立于手动槽，按 generation 轮换，提交后写入、成功读档不写回；失败保留旧档及已提交剧情，提示并停用 autosave。外部捕获必须验证 Session；App 已提交状态可直接捕获，不能重复重放所有历史。
- 新内容使用 entry/scripts/interface，不读取 flow.xml；JS 注册覆盖所有场景并与 XML 选项对应，动态目标在执行边界校验。错误保留 XML 位置或 JS 文件/栈，不承诺穷举动态分支。XML UI 为必需；无内置兼容界面，无旧格式回退。demo 的旧 flow.xml 仅在 `doc/archive/demo-flow-v0.3.xml` 留档。
- UI 图片/模型/视频引用使用资源 ID，严格验证绑定/事件/模板。profile.json 独立保存 CG（schema 1/story_id），不被旧剧情槽或重开撤销；坏进度提示并使用初值。
- 运行入口统一使用 ContentSource；DirectorySource 校验游戏内相对路径及真实路径边界，XML/JS/图像/字体/音频从来源读取字节。KPAK 1/2 与目录快照实现相同接口，不把物理目录或压缩格式泄漏到规则与绘制中。用户已要求工程状态监控热更、发布状态只读资源包：候选一致性/完整验证成功才原子替换；坏编辑保留旧状态，发布不监控/回退散文件。
- 偏好保存在独立 settings.json，不绑定某个剧情槽。坏设置使用默认值并提示；音频设备失败应降级静音，不阻止剧情播放。
- 资源路径以游戏根目录为基准。始终随发布包分发字体及许可证，不依赖开发机字体。
- 保留原始 `doc`；需要完整演示素材时修改 `games/demo`，不要假定文档示例含有图片。

## 修改与验证

工作在当前根目录单 crate，提交 `Cargo.lock`，不提交 `target`、本地工具缓存或用户存档。保持实现直接；主逻辑单线程，视频允许一个有界解码线程，工程允许原生监控及单候选准备线程，不引入异步 runtime。新增抽象前先确认真实需求。文档使用中文，源代码采用惯用 Rust。

代码完成后运行与变更有关的检查：

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked -- --check
```

控制流变化验证条件组合、首命中/兜底、隐藏选项伪造、变量回滚、分支、历史、自循环、非法输入与结束；加载器变化验证仅接受 story 8/UI 3、拒绝旧 schema/flow，并保留位置诊断；存档变化验证变量类型/范围、检查点裁剪重放、槽位隔离、旧档拒绝、原子覆盖与失败保留旧档。GUI 变化实际检查中文、稳定换行、16:9、补全/推进仲裁、菜单暂停，以及保存后完全退出再重启恢复位置、变量、条件选项与历史。音频纯计划和解码测试不依赖设备；实际设备流与主观听感分别报告。不要仅凭核心测试通过就声称 GUI 或听感已验收。

交付时明确已验证项和未验证项，更新 `readme.md` 的启动、数据契约和限制说明。不擅自删除用户文件、提交或推送 Git 变更。

## v0.5.1 立绘动作

portrait.pose 为最终显示契约；enter/exit 与 ctx.portrait 仅驱动呈现时钟。动作必须匹配最终姿态，候选 Frame 校验先于任何 UI、剧情、IO 或呈现提交。每槽单动作，退场快照只保留到动作/全场转场结束；读档不重播。新增动作或修改渲染时覆盖镜像网格、首尾姿态、快照清理、暂停、自循环和跨进程读档。接口与限制同步 doc/xml-js-api.zh-cn.md 第 18 节。


## v0.6 自动播放与媒体

- 自动播放为瞬时状态；文字、转场、语音都完成后等待 auto_delay。菜单/提示/失焦/视频暂停计时；选项/结尾、手动剧情输入/读档/重开停止。用户输入优先，每次更新只提交一个剧情操作。
- 视频资源从 ContentSource 读取，临时文件属于后端细节；Windows Media Foundation 解码，有界队列/资源预算。准备候选视频和模型必须先于 Session/UI/IO 提交；结束/跳过必须经控制器，待提交结束事件不能因菜单输入丢失。读档视频从零秒重播。
- 视频音轨独立 video_volume，播放期间暂停剧情音频；菜单/失焦同时暂停视频时钟与音轨，禁止绘制/tick 重启 Sink。
- Live2D 使用真实 Cubism Core，动态库仅从宿主受信任配置加载，禁止游戏脚本或资源提供任意原生库路径。模型从 ContentSource 读取；Core 句柄/时钟/动作进度不入 Model/存档。候选 XML 参数、动作及 JS 动作先验证；静态 fallback 只在显式禁用时使用。
- 保持模型/贴图/网格/动作/合成预算，不支持的 offscreen/高级混合明确拒绝；运行失败提示并冻结最后图像。蒙版取源贴图 alpha，不受蒙版源 drawable 是否可见/透明度影响。不要把辅助 physics JSON 读取说成物理模拟支持。
- 修改媒体时验证真实 MP4 首帧/音轨/暂停/结束/跳过、蒙版正反裁剪、Core 参数改变网格及真实模型画面；SDK 用例通过 CUBISM_SDK/LIVE2D_TEST_GAME 显式运行。用户已追加要求默认 demo 集成 Haru/Core，允许从其本地 SDK 复制并随本地包测试；必须保留第三方许可，不执行未经授权的公开发布。Core 仅从固定宿主 runtime/显式配置/程序旁加载，不从游戏资源或 JS 指定路径加载。
- GUI 必须实际检查末页第 200 槽、跨进程手动/自动读档、变量/历史、自动播放在选项停止、视频菜单暂停与结束、模型动作和 16:9。设备流与主观听感分别报告。同步 API 手册、README、验收与 package.ps1。


## v0.7 工程/发布与渲染

- --project 为工程，--package 或程序旁 game.kpak 为发布；发布坏包必须失败，禁止回退散装目录/启动监控。Core 在包外固定宿主路径，发布禁止回退源码 runtime。
- native notify 事件去抖，固定快照后台构建，主线程拒绝过期候选。兼容更新必须重放当前历史/变量并验证当前条件及生命周期/媒体；坏编辑不改变旧 Session/音频/图像。不兼容只允许用户在工程窗显式从入口重载；story_id 改变需重启。成功热更新不写自动档/CG、不重播瞬时动作，重建 UI/呈现时钟。
- KPAK 1 固定头、严格路径/目录/块/长度、SHA-256、单 solid Zstandard 帧，去重/确定性，拒绝尾随帧/路径碰撞/解压越界。只允许 --force 原子覆盖，输出在工程外；发布脚本不复制游戏散文件或用户数据，保留字体/模型/原生库许可。
- Windows 运行窗口为原生 D3D11 swapchain；PixelBackend/ModelDraw/Image 为平台边界，Model/Controller 不引用 GPU。正常滤镜/Live2D 合成输出直接作为原生纹理，不逐帧 readback/upload；Core 参数求解仍是 CPU。CPU fallback 的图像上传与测试 readback 必须区分，不得把它们当成正常 GPU 管线。
- GPU 失败明确提示并切 CPU；设备失效最多一次 WARP 重建，不能无限重试。保留 CPU 蒙版/混合/滤镜测试及 GPU 像素对比，真实 Haru 原生输出/裁剪与 CPU 轮廓比较；不宣称实现 Vulkan/Metal，只保留接口。
- 新增热更时测试真实保存/重命名/删除恢复、坏 XML/JS 回滚、契约变更/过期候选；实际 GUI 修改可见文字/图片/脚本并保留进度。发布包从源码外运行 --check，无散资源；GPU/CPU 窗口、中文/16:9/菜单暂停/跨进程读档分别记录。锁屏时仅继续非 GUI 工作，解锁后再验收，未完成项必须明确。

## v0.7.1 多文件与无控制台发布

- 当前 story schema 8；固定 story.xml，后续根仅 schema。每份恰好一个 next.src 或 next end="true"，链最多 128 文件/16MiB；src 以游戏根为准，循环/混合 schema/未知字段/同类重复 ID 拒绝。v0.8 只接受 story 8/UI 3/save 8，旧解析器与历史兼容测试删除；KPAK 1 不变。
- story_chain 只在加载阶段合并与映射源位置，不把 Node/物理路径放入 Model。分组先合并再全局校验，JS 决定跨章路由；next 不生成隐式剧情分支。工程热更沿用完整候选事务。
- 打包替换 story.xml 并移除链上其余源 XML，不删 UI/JS/链外文件、不改磁盘工程；合并包独立重载通过后确认源指纹，再原子提交。
- Windows 播放器使用 GUI subsystem，发布不创建/附着控制台，启动错误用原生对话框；工程日志可分配控制台，--check/--help 只附着已有控制台并保留重定向。kakera-pack 仍为控制台工具。覆盖 PE 头/捕获输出/失败退出码、真实窗口无控制台、章节热更与跨章读档；更新 API/指南/demo/SDK 准备及发布脚本。


## v0.7.2 背景输入与截图存档

- UI 3 screen 的 background-click/background-right-click 只向顶层派发；按下/释放都须在舞台背景，控件（含禁用）、滚动列表、黑边、浮窗、提示、失焦阻挡；一次更新至多一个事件。HUD bool/可见性及恢复逻辑放 XML/JS，scene 不随 HUD 隐藏，纯 Controller 不增加输入设备依赖。
- save 8 可选 screenshot 为 320×180 PNG/Base64，压缩最大 256KiB，解码预算/块类型/尺寸严格验证；总 JSON 2MiB。screenshot 字段必需，可为 null；旧档不迁移。saved_at_unix_ms 展示本地时间，不向 JS 开放系统时钟或路径。截图、时间和 Session 同文件原子替换，不用 sidecar。
- 宿主缓存带 revision 的最近客户区/无菜单游戏画面（每幅最多 64MiB）；正常帧只 GPU 拷贝，实际保存才 readback/缩图/编码。手动保存须匹配当前 revision，失败保留原档及 UI 计划；自动保存等匹配呈现，pending 阻挡新剧情，失败保留已提交剧情并停用。设备重建清捕获、热更新/自循环更新 revision；不把旧场景截图配新 Session。
- 预览仅刷新当前十手动槽/十自动槽时解码上传，换页清缓存，绘制只读。测试按下拖出/同帧事件/右键优先、禁用与滚动遮挡、HUD 首次恢复、坏图/旧无图/原子失败/呈现匹配；实际验证 GPU/CPU 窗口保存、时间/缩略图和跨进程读档。同步 API 第 22 节、README/指南/验收/发布。


## v0.7.3 Ctrl 快进与槽位预览

- 按住 Ctrl 每约 40ms 一步，每步只做一件且顺序固定：跳过转场 → 补全当前句 → 推进；选项/结尾不推进且不得替玩家选择，视频播放、菜单/提示/失焦、自动保存等待期间停止或暂停；Ctrl 优先于自动播放，松开后自动播放重新计时；每次更新仍至多一个剧情操作，不新增输入设备依赖。
- 只读模式只看当前句是否已读：已读句可补全并推进，未读句一经进入立即停下并按正常速度显示，不得为了让快进跑完而跳过未读内容。全部模式由 `settings.skip_all` 打开。
- 已读以“整句完整显示过一次”为界（读档后当前句立即算已读，视频遮挡不算），写入独立 `read.json`（schema 1/story_id/frames），约 3 秒节流 + 退出补写，同目录临时文件原子替换；写失败仅记日志并保留待写标记，坏文件提示一次并按空集，未知 frame ID 保留但不参与故事校验。`read.json` 排除出工程监控与 KPAK，不进入任何存档槽或历史重放，不开放 JS 查询接口。
- settings.json 只接受完整当前字段，不用 serde(default) 猜测旧文件；缺失/损坏时提示并采用默认值。skip_all/fullscreen 默认 false，绑定白名单和命令 bool 校验同步。
- 存读档页槽位 `preview`/`excerpt` 只含台词与状态文本，不得显示 frame ID；槽位 JSON 继续保存 `frame_id` 供校验与加载。只维护 XML/JS 界面。
- 修改时覆盖：只读/全部、未读停住、选项不自动选择、视频与菜单暂停、Ctrl 与自动播放、read.json 损坏/写失败/原子替换、缺字段旧设置拒绝、手动与自动槽预览无 ID；实际窗口检查按住 Ctrl 的转场/补全/推进与存读档页显示。

## v0.8 窗口与单一契约

- 窗口客户区固定 16:9；原生拖边/拖角、DPI 与程序尺寸变化不能破坏比例。全屏为当前显示器无边框模式，退出恢复窗口尺寸与位置；非 16:9 屏幕全屏保留等比边界。
- 全屏设置界面与交互放 XML/JS，ctx.setting("fullscreen", bool) 走设置事务；Rust 只负责受约束宿主窗口能力，不向剧情 Model/存档加入窗口句柄。
- 旧版本资料留在 doc/archive，不为其保留执行分支或自动迁移。通用行为测试使用当前 XML/JS；必要的旧输入测试只断言拒绝。


## v0.8.1 KPAK 2 流式包

- KPAK 2 为大内容提供 80 字节头、受限尾部索引、u64 偏移和独立 1MiB 原文 Zstandard 块，块级去重/摘要，按需验证后交付；禁止整包读入内存。KPAK 1 的字节格式/读取兼容按用户明确要求保留，旧 story/UI/save 不恢复。
- 默认打包 2，--format 1 显式小包；发布按头识别 1/2，未知版本/坏包不回退目录。--check 与打包必须验证全部唯一块，普通启动只保证索引与已访问块。
- 容器预算：包及逻辑总量 1TiB、单文件 64GiB、100000 文件、1048576 唯一块、索引 128MiB、路径 1024 字节。拒绝路径碰撞、孤立/越界引用、洞/重叠、尾随帧/数据、摘要错误和解压越界。
- 工程候选/KPAK 2 打包用私有磁盘固定快照、分块复制与哈希，完整验证候选/包并复查源指纹后原子提交；不改源工程。ContentSource.read 保持 128MiB，open/copy_to 支持受限流；大视频流复制到后端临时文件，解码/PCM 预算不放宽，不在绘制做 IO。
- XML/JS 及 story 8/UI 3/save 8/content 8 不变；文件流/解压确需 Rust 宿主能力，新增游戏交互仍优先 XML/JS。覆盖 KPAK 1 兼容、2 的索引/块恶意输入、流边界/并发、源变化/原子失败、真实媒体和超过 512MiB 实测；同步规范、API、README、指南、发布脚本与验收。


## v0.8.2 点九、图集与场景别名

- 点九资源/UI 均用专用 nine-patch，读取带标记边框的源 .9.png，加载解析/去边框。上/左每轴 1–8 段伸缩，下/右单段内容区；density 折算固定段和 padding。普通 image 不能代替点九节点，不宣称 APK 编译 NinePatch 支持。
- image/panel/button 的 region 为源像素 x,y,w,h；region-bind 仅 UI string，JS 复用 ctx.setUi。准备/提交前整批验证格式、当前图及皮肤边界，View 只读缓存、只换 UV，不切图/上传纹理。点九不接受 region；图集滤镜仍整图缓存。
- 用户确认稳定 frame.name 别名：id 缺省时 name 必填，工程暂以 name 为内部 ID；KPAK 1/2 在候选中生成 UUID v4、保留 name，源 XML/JS 不改写。ID/name 不与其他场景冲突，所有 JS 注册/动态目标仍严格验证，不引入顺序默认分支。
- 槽位/历史/已读使用实际 ID，JS ctx.frameId/game.frameId 对具名场景提供 name。重新构建随机 UUID 会改变档案契约，需提升 content-version；跨构建稳定档案应固定显式 id，不按别名迁移。无缺省 ID 的工程继续确定性打包。
- story 8/UI 3/save 8/KPAK 1/2 不变。新增宿主图片几何和打包 ID 能力确需 Rust；动画/页面/图片区域切换仍优先 XML/JS。覆盖点九多段/padding/DPI、图集 UV/共享纹理/绑定回滚、UUID 名字路由/自循环/条件/存档/两种包和源不改写，实查 GPU/CPU 窗口并同步 API/README/指南/发布/验收。
