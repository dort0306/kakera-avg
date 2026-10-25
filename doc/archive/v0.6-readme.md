# Kakera AVG Rust · v0.6.0

基于 Rust、egui/wgpu 与 QuickJS 的 Windows x64 AVG 播放器。游戏作者使用 XML 定义场景和界面、JavaScript 触发器组织逻辑，无需编写 Rust。内容在启动时读取，修改后重启生效。

**v0.6 是后续开发的新基线**：story schema 6、UI schema 2、存档 schema 6；不承诺旧格式兼容，不自动迁移或覆盖旧档。原始设计、示例及旧验收记录保留；上一版说明见 [v0.5.1 归档](doc/archive/v0.5.1-readme.md)。

- 200 个独立手动槽，每页 10 个、共 20 页，可跳页并确认覆盖。
- 10 个独立自动槽，每次成功进入场景后轮换；读档不回写自动档，设置可关闭。
- 自动播放：文字、转场和语音完成后等待指定延迟；菜单/失焦暂停，选项与结尾停止。
- 视频：系统 Media Foundation 解码 MP4 图像/音轨，支持菜单暂停、跳过、结束推进和视频独立音量。
- Live2D：真实 Cubism Core moc3 形变，model3/motion3/expression3、贴图与裁剪蒙版；XML 声明，JS 触发已登记动作。

保留中文 Unicode 字素簇逐字、完整历史与变量检查点、条件选项、三个立绘槽、BGM/语音/音效、转场/图片效果/立绘动作、XML 自定义存读档/设置/CG 画廊和 16:9 等比布局。

详细创作接口见 [XML 与 JavaScript API 手册](doc/xml-js-api.zh-cn.md)，五项新能力集中在 [第 19 节](doc/xml-js-api.zh-cn.md#19-v06-分页自动保存自动播放视频与-live2d)。[落地策略](doc/v0.6-implementation-plan.zh-cn.md) 先于代码制定，[验收记录](doc/v0.6-ui-validation.zh-cn.md) 区分自动测试、实际窗口和未验证范围。

## 启动

需要 Rust stable（最低 1.88）、Windows MSVC C++ 构建工具及系统媒体组件。仓库包含 Cargo.lock；首次构建需要下载依赖。

```powershell
cargo run --locked
cargo run --locked -- --check
cargo run --locked -- --game games/demo --save-dir target/my-saves
cargo run --locked -- --no-audio
```

`--check` 校验 XML、JS 注册/初始条件/入口生命周期、图片、字体、音频、视频和模型，不需要窗口或音频设备；不能穷举所有动态 JS 分支。`--no-audio` 只关闭设备输出，仍解码音频。默认先查找程序旁 games/demo，再回退源码样例；显式命令行相对路径按工作目录解析。

便携包运行 `kakera-avg.exe`，参数相同。打包命令：

```powershell
./tools/package.ps1
# 目标已存在会拒绝覆盖；重打包时指定新目录
./tools/package.ps1 -Output target/package/kakera-avg-v0.6-review
```

默认输出 target/package/kakera-avg-v0.6-live2d，包含游戏 XML/JS、视频、中文字体和许可，不包含用户存档或完整 SDK；包含本地 demo 使用的 Haru 模型、Windows x64 Core 和原许可。运行不需要 ffmpeg；tools/generate-demo-media.ps1 仅用于重新生成自制演示片，需本机 ffmpeg 的 libopenh264/AAC 编码器。

## Demo 内置 Live2D 测试

新增集成说明与实际验收见 [Live2D demo 验收](doc/live2d-demo-validation.zh-cn.md)。直接 `cargo run --locked`，从问题页选择 **“Live2D 测试”**。独立舞台提供：

- 待机与重置：Idle:0 + F01；重复进入可重置呈现。
- 播放点击动作：通过 UI JS 的 ctx.live2d 播放 TapBody:0。
- 切换表情 F02：进入独立、可恢复的表情场景。
- 查看参数姿态：ParamAngleX=25、ParamAngleY=-10，无待机动作覆盖。
- 返回剧情：回到原问题页，剧情变量保持不变。

原问题页仍显示普通凛立绘。三个测试场景支持菜单暂停、手动/自动存读档；读档恢复 XML 声明，从起点重建。显式 `--no-live2d` 使用静态 fallback。

Haru 位于 games/demo/assets/live2d/Haru；Core 位于固定宿主目录 runtime/live2d/windows-x64。DLL 搜索顺序为 --cubism-core、CUBISM_CORE、程序旁 DLL、源码固定 runtime，均不使用游戏内容提供的原生库路径。便携包将 Core 放在 exe 旁，直接运行即可。模型内部文件以 model3 目录为基准，其余 XML/JS/素材以游戏根为基准；全部内容通过 ContentSource 读取。

素材来自用户提供的 C:/CubismSdkForNative-5-r.5，SDK/模型条款位于 games/demo/assets/licenses/LIVE2D-SDK-LICENSE.md，Core 许可与可再分发文件清单位于 runtime/live2d。本次为本地集成，未执行公开发布；第三方素材不属于项目自制 CC0 素材。更换 SDK 时可用工具生成独立副本（目标存在会拒绝覆盖）：

```powershell
./tools/prepare-live2d-demo.ps1 -Sdk C:/CubismSdkForNative-5-r.5 -Output target/live2d-copy
```

## 演示入口与操作

demo 有 30 个场景、4 个变量、3 个 JS 文件、11 个 XML 页面和 2 段自制视频。问题页可选 Live2D 测试、片头/片尾、立绘动作或剧情分支；顶部“效果演示”可预览转场与图片效果。

- 空格或继续：第一下补全当前句，第二下推进；转场时先跳过转场。未完成时不能选择。
- 自动播放：等待完整文字、转场和语音后再推进；不会替玩家选择。手动剧情输入、读档和重开停止自动模式。
- 保存/读档：末页包含 191–200 槽；读档页可打开独立自动档列表。菜单内剧情、媒体和呈现时钟暂停。
- 视频：片头自然结束进入 opening；片尾结束后保留画面并呈现结束文字；允许跳过时显示按钮。
- 历史：只读显示已访问场景、选择及当前变量；不支持回滚。读档恢复历史、变量、条件选项和当前位置。

## 数据与存档契约

默认目录为 Windows 用户数据目录 `%LOCALAPPDATA%/kakera/kakera-avg/data/<story_id>`，以启动日志为准。--save-dir 可完全隔离测试数据。

```text
slot-1.json … slot-200.json   schema 6，手动档 generation=0
auto-1.json … auto-10.json    schema 6，generation 排序并轮换
settings.json                自动保存、自动播放延迟、文字速度、总静音、四通道音量
profile.json                 独立 CG 解锁，schema 1 / story_id
```

存档包含当前位置、变量、最多 1000 条历史与历史起点变量；读取时重放条件、更新与目标并核对终值，全部成功才提交。场景 ID、变量/路由语义不兼容变化必须提升 content-version。坏档、旧 schema 或身份不匹配均拒绝，保留旧 Session 和文件。JSON 严格字段；[v0.6 示例](doc/v0.6-save.example.json) 有自动测试验证。

手动与自动档使用同目录临时文件及平台原子替换，失败保留旧文件。自动保存发生在剧情提交之后，失败不会撤销已进入的剧情，而会提示并关闭 autosave，修复后可重新开启。分页元信息只作预览，完整历史重放在实际读取时执行。

UI 栈、UI 局部变量、逐字/自动播放/动画/视频指针和 Core 句柄不入剧情档；读档整句显示、重建音频，视频和 Live2D 从 XML 声明起点重建。设置/CG 独立，不随剧情槽回退。旧设置不符合严格字段时提示并用默认值；音频设备失败降级静音。

## 当前限制

- 视频后端首发 Windows；已验证 720p/30fps H.264/AAC MP4。最大 1080p、约 60fps、300 秒/段，单文件 128MiB，最多 16 段；音轨启动时完整解码并限总缓存 256MiB。无 seek、网络流或精确音画同步保证。
- Live2D 采用 CPU 合成，最长边 512 像素、约 30Hz，复杂模型和大窗口有性能/清晰度限制。已实现基础蒙版/混合/动作/表情/Pose 切换；无 physics3、完整 Pose 渐变、自动口型/跟随、动作声音与 Cubism offscreen/高级混合。完整能力与预算见 API 第 19.7 节。
- UI 只有基础布局/颜色/图片/控件，非浏览器 CSS/DOM；新增页面可仅改 XML/JS，新增宿主能力仍需 Rust。
- QuickJS 回调同步、隔离且受预算限制，不开放宿主 IO、时间、随机、异步或网络；绘制不执行脚本、保存文件或推进剧情。
- 当前验收为本机 Windows x64 和 SDK Haru 样例；其他操作系统、全部 Cubism 模型及主观音频听感未验收。

## 开发验证

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked -- --check
# 本地真实 SDK 集成用例默认忽略，需显式配置后运行
$env:CUBISM_SDK='C:/CubismSdkForNative-5-r.5'
$env:LIVE2D_TEST_GAME='D:/codes/kakera-avg-rust/games/demo'
cargo test --locked --test live2d_native -- --ignored
```

当前结构：model/controller 为纯领域与提交；loader/script/interface 负责严格契约与计划；save/settings 负责持久化；media/live2d/audio/assets 适配运行资源；app 仲裁输入/IO/媒体更新；view 只绘制缓存。仅视频使用一个有界解码线程，无异步运行时/ECS/插件框架。参见 [AGENTS.md](AGENTS.md) 的维护约定。
