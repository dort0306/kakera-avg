# Kakera AVG Rust · v0.2

基于 XML 场景和纯 Rust 控制器的 Windows 桌面 AVG 播放器。一句台词对应一个可独立还原的 Frame。实现依据见原始 [MVC 方案](doc/rust-mvc-plan.zh-cn.md) 和先于代码制定的 [v0.2 详细落地策略](doc/v0.2-implementation-plan.zh-cn.md)。

当前支持四场景双分支样例、背景与三个立绘槽位、中文逐字显示、历史记录、六个手动存档槽，以及 BGM/语音/音效三通道。游戏区域固定 16:9，窗口变化时等比缩放并留黑边。

## 启动与校验

需要 Rust stable（最低 1.88）及 Windows MSVC C++ 构建工具。首次构建需下载依赖；提交了 Cargo.lock，请使用 `--locked`。

```powershell
cargo run --locked
cargo run --locked -- --game D:/codes/kakera-avg-rust/games/demo
# 无需窗口和音频设备，校验 XML、图片、字体及 WAV/Ogg
cargo run --locked -- --check
# 禁用音频输出，但仍检查音频文件
cargo run --locked -- --no-audio
# 隔离存档和设置目录，用于调试或便携使用
cargo run --locked -- --save-dir target/my-saves
```

默认先查找可执行文件旁的 `games/demo`，再回退到编译时的源码样例目录。显式命令行相对路径相对于工作目录；XML 中的素材路径始终相对于游戏目录。原 `doc/rust-mvc-example` 不含素材，只作为 v0.1 数据契约保留；运行请使用 `games/demo`。

## 逐字、历史与输入

- 默认每秒显示 30 个 Unicode 字素簇，支持中文、组合字符和 emoji；换行按整句布局保持稳定。设置中速度为 0 表示整句显示。
- 文字未显示完时，“补全”或空格只补全当前句；完成后再按一次才推进。选择按钮在当前句完成后开放，空格不会选择默认分支。结尾完成后显示“已结束”。
- “历史”显示当前会话的角色、台词及已选项，包含当前句。只读、不支持回滚；循环访问也记录，最多保留最近 1000 条。
- 打开历史、存读档、设置或提示时暂停逐字；失焦也暂停，返回后不补算暂停时间。
- 重新开始清空历史并重新逐字。读档替换当前位置及整个历史，当前句整句显示。存档不保存逐字进度或音频播放指针。

## 六个手动槽位与旧档

“保存”“读档”分别打开槽位窗口，显示空槽、场景预览、相对保存时间或错误原因。已有文件需要应用内确认覆盖；取消保留原文件。空槽、损坏或不兼容槽禁止读取，其他槽仍可正常使用。

默认 Windows 用户数据目录为 `%LOCALAPPDATA%\kakera\kakera-avg\data\<story_id>`，以启动日志中的实际路径为准：

```text
slot-1.json ... slot-6.json  schema 2 快照，互相独立
settings.json              文字速度、总静音、三通道音量
save.json                  v0.1 旧档，只读导入入口
```

schema 2 包含故事/内容版本、当前位置、历史及保存时间；示例见 [v0.2-save.example.json](doc/v0.2-save.example.json)。读档前检查整个历史的场景、选项、相邻路由和当前位置，全部通过才提交。错误不会改写当前会话或重播音频。写档使用同目录临时文件、sync_all 和平台原子替换，不先删除旧档。每份 JSON 上限 1MiB，不承诺所有文件系统的断电持久性。

检测到旧 `save.json` 时，读档窗口提供“导入 v0.1 存档”。只恢复合法的当前位置，建立一条历史并明确提示旧档不含历史；原文件不被改写。之后可手动保存到任一新槽。存档 schema 与 XML content-version 独立，本次样例保留原场景 ID 和内容版本 1。其他内容版本不自动迁移。

## 音频与设置

音频使用 rodio 0.21.1，支持 WAV 和 Ogg Vorbis。启动时解码并缓存，绘制不读盘、不触发播放。BGM 循环；连续场景声明同一 BGM 时继续播放；切换资源或省略 BGM 时替换/停止。语音和音效在进入场景时单次播放，离开时停止旧音轨。读档、重新开始从头重建音频。

菜单/提示暂停语音和音效，BGM 继续；失焦暂停三通道。设置提供总静音、独立音量和文字速度，关闭设置及退出应用时保存为独立偏好。无设备或输出中断时提示并降级静音，剧情和存读档仍可使用。`--no-audio` 可显式禁用设备输出。

音频限制：单/双声道、8000–96000Hz、单资源最长 5 分钟/PCM 上限 128MiB、总 PCM 缓存上限 256MiB。样例中的 `lin.wav` 是验证语音通道的合成提示音，不是实际角色配音。所有演示音频均由本项目生成，来源与许可见 [素材说明](games/demo/assets/licenses/README.md)。

## 内容契约与模块

story schema 1 继续兼容；新音频内容使用 schema 2，flow 仍为 schema 1：

```xml
<audio id="bgm.room" src="assets/audio/room.wav" /> <!-- assets 内 -->
<frame id="opening" visual="room.empty">
  <text>午后的房间里，传来了轻轻的敲门声。</text>
  <audio bgm="bgm.room" se="se.knock" />
</frame>
```

每场景最多一个 audio，bgm/voice/se 可选；省略表示该通道静音，没有隐含继承。图片、字体、音频须使用游戏目录内相对路径。未知元素/属性、命名空间、重复 ID、类型错误、缺失引用和不一致路由都会阻止加载，XML 错误附文件与行列。文本与选项只接受普通 XML 文本，保留空格和换行，不接受内嵌标签。每个场景必须显式对应 next/choice/end，选项与路由一一匹配；循环合法、不可达场景警告。

```text
src/model.rs         纯领域数据：完整场景、音频目标、会话与历史
src/controller.rs    纯流转、选项登记、显式场景进入结果
src/presentation.rs  逐字时钟、字素边界、补全/推进仲裁
src/loader.rs        XML 严格校验及预设展开
src/assets.rs        图片/字体准备与纹理缓存
src/audio.rs         PCM 缓存、纯通道计划、设备输出与降级
src/save.rs          六槽、schema 2、旧档导入、原子 JSON 写入
src/settings.rs      独立偏好与范围校验
src/view.rs          只读场景绘制与 UI 意图
src/menus.rs         历史、槽位、设置窗口
src/app.rs           成功提交、呈现计时与进入副作用的装配
```

不引入脚本、变量条件、ECS、异步运行时或事件总线。GUI 固定 eframe/egui 0.33.3 与 wgpu，XML 固定 roxmltree 0.21.1，完整依赖版本以锁文件为准。

## 检查与发布

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked -- --check
cargo build --locked --release
./tools/package.ps1
# 输出已存在时，使用新目录，不覆盖旧包
./tools/package.ps1 -Output target/package/custom-build
# 重新生成占位素材
./tools/generate-demo-assets.ps1
cargo run --locked --example generate_demo_audio
```

默认便携包位于 `target/package/kakera-avg-v0.2`。将程序和整个 games/demo（包括字体及许可）一起复制即可运行；包内不含开发存档和个人设置。音频测试的 Ogg fixture 已随源码提供，运行测试不需要 ffmpeg。

验证范围与限制记录在 [v0.2 策略文档的实施记录](doc/v0.2-implementation-plan.zh-cn.md#8-实施记录)。自动测试不要求声卡；实际听感、设备拔插和其他操作系统须单独验收。暂不包含音频淡变/精确进度恢复、转场、快进、回滚、自动播放、多变量规则或编辑器。

2026-10-23 已完成 Windows [v0.2 界面验收](doc/v0.2-ui-validation.zh-cn.md)：逐字与菜单暂停、历史、不同分支及新槽位的跨重启恢复、静音/音量设置持久化、16:9 缩放均通过。实际音频设备流已建立；主观听感和设备热拔插未验收。
