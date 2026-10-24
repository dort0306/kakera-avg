# 项目协作约定

## 目标与依据

先阅读当前基线 `doc/v0.6-implementation-plan.zh-cn.md`、`doc/xml-js-api.zh-cn.md` 与 `doc/v0.6-ui-validation.zh-cn.md`，再阅读历史 `doc/rust-mvc-plan.zh-cn.md`、`doc/v0.2-implementation-plan.zh-cn.md`、`doc/v0.3-implementation-plan.zh-cn.md`、`doc/js-trigger-implementation-plan.zh-cn.md`、`doc/v0.4-implementation-plan.zh-cn.md`、`doc/effects-implementation-plan.zh-cn.md` 和原始数据示例，再检查代码与测试。`doc/rust-mvc-example` 保留 v0.1，`doc/v0.3-example` 保留声明式规则，`doc/v0.3.1-example` 保留 schema 4 JS 契约；可运行样例在 `games/demo`。用户已要求 JavaScript/QuickJS 触发器与 XML/JS 自定义 UI；界面位置、基础样式与菜单放 XML，交互放 JS，不要求游戏作者编写 Rust。不提前引入 ECS、异步运行时、消息总线或插件框架。

## 架构约束

- `model` 只含领域数据和只读查询，不引用 egui、文件 IO、窗口和 GPU。
- `controller` 仅依赖 `model`，保持纯函数；只有验证成功才返回新 Session。无效选项不跳到默认分支。
- `model::Logic` 是纯计划接口；`script` 适配 QuickJS，不能把解释器对象放进 Model 或存档。回调每次隔离执行；只通过 ctx.set/add 收集更新，Rust 校验并原子提交。条件回调只读，异步任务、时钟、随机和宿主 IO 不开放；资源预算不得绕过。
- 规则放在 `model/rules`，条件基于更新前变量；命中路由后按顺序更新副本，与场景、历史一起原子提交。类型错误、溢出、越界或隐藏选项均失败，不留下部分更新。branch 首个命中，otherwise 必须兜底。
- `loader/save` 引用 `model`；`view` 读取 `model/assets`，返回 UI 意图；`app` 执行意图和 IO。
- 每个 Frame 是可独立恢复的完整显示状态，不隐式沿用上一句；visual 在加载阶段展开，不链式继承。
- XML 节点和借用不离开加载器；内容使用拥有的 String。静态纹理缓存由 assets/view 持有，不逐帧读盘/解码/上传。视频帧与 Live2D 动态纹理只在 App 更新阶段生成和上传，绘制只读。
- 不在绘制过程中推进剧情、执行脚本或保存文件；单次 UI 更新最多提交一个意图。
- 转场与图片效果依据 `doc/effects-implementation-plan.zh-cn.md`；配置在 Model，时间/旧画面快照在 App 呈现层，指针不入档。所有目标条件、生命周期命令及 IO 成功后才启动效果；读档直接显示最终态。转场时不积累逐字时间，继续先跳过转场；菜单/失焦暂停。滤镜启动时缓存，额外缓存上限 256MiB，不在绘制时过滤/解码/上传。
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
- v0.6 为新的正式基线：story 6 / UI 2 / save 6，demo content-version=6；不承诺历史格式兼容，不自动迁移旧档。历史解析器/显式导入若暂留，只作为历史工具，不得让正常 load 猜测旧变量。
- schema 6 保存 generation、当前位置、变量与历史起点检查点；历史最多 1000 条，裁剪推进检查点，读档完整重放条件/更新/目标与终值。手动槽 1–200，十槽一页/20 页，只扫描当前页元信息；真正加载再完整验证。
- 自动槽 auto-1…10 独立于手动槽，按 generation 轮换，提交后写入、成功读档不写回；失败保留旧档及已提交剧情，提示并停用 autosave。外部捕获必须验证 Session；App 已提交状态可直接捕获，不能重复重放所有历史。
- 新内容使用 entry/scripts/interface，不读取 flow.xml；JS 注册覆盖所有场景并与 XML 选项对应，动态目标在执行边界校验。错误保留 XML 位置或 JS 文件/栈，不承诺穷举动态分支。
- UI 图片/模型/视频引用使用资源 ID，严格验证绑定/事件/模板。profile.json 独立保存 CG（schema 1/story_id），不被旧剧情槽或重开撤销；坏进度提示并使用初值。
- 运行入口统一使用 ContentSource；DirectorySource 校验游戏内相对路径及真实路径边界，XML/JS/图像/字体/音频从来源读取字节。未来资源包实现相同接口，不把物理目录或压缩格式泄漏到规则与绘制中。编辑内容后重启读取，不暗中热重载。
- 偏好保存在独立 settings.json，不绑定某个剧情槽。坏设置使用默认值并提示；音频设备失败应降级静音，不阻止剧情播放。
- 资源路径以游戏根目录为基准。始终随发布包分发字体及许可证，不依赖开发机字体。
- 保留原始 `doc`；需要完整演示素材时修改 `games/demo`，不要假定文档示例含有图片。

## 修改与验证

工作在当前根目录单 crate，提交 `Cargo.lock`，不提交 `target`、本地工具缓存或用户存档。保持实现直接；主逻辑单线程，视频允许一个有界解码工作线程，不引入异步 runtime。新增抽象前先确认真实需求。文档使用中文，源代码采用惯用 Rust。

代码完成后运行与变更有关的检查：

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked -- --check
```

控制流变化验证条件组合、首命中/兜底、隐藏选项伪造、变量回滚、分支、历史、自循环、非法输入与结束；加载器变化验证 story schema 1/2/3、flow 1/2 及位置诊断；存档变化验证变量类型/范围、检查点裁剪重放、槽位隔离、只读旧档导入、原子覆盖与失败保留旧档。GUI 变化实际检查中文、稳定换行、16:9、补全/推进仲裁、菜单暂停，以及保存后完全退出再重启恢复位置、变量、条件选项与历史。音频纯计划和解码测试不依赖设备；实际设备流与主观听感分别报告。不要仅凭核心测试通过就声称 GUI 或听感已验收。

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
