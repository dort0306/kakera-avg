# 播放器使用指南

当前应用版本 v0.11.2。本文说明启动、剧情运行、存档与用户数据；构建与测试命令见根目录 [README](../README.md)，工程与发布见 [工程、发布与窗口模式指南](project-release-pack-guide.zh-cn.md)，格式细节见 [数据契约](data-contracts.zh-cn.md)，资源边界与未支持项见 [验证与限制](limits.zh-cn.md)。

## 启动

源码运行默认使用 `games/demo` 工程：

```powershell
cargo run --locked
cargo run --locked -- --project games/demo --save-dir target/dev-saves
cargo run --locked -- --project games/demo --renderer cpu --no-audio
cargo run --locked -- --check
```

启动先显示 XML 初始 UI。demo 可选择“从午后开始”“另一条故事线”或载入存档。`story entry="opening secret-entry"` 声明多个起始幕；首次通过 `ctx.resume("opening")` / `ctx.restart("opening")` 显式选择，或 `ctx.load` / `ctx.loadAuto` 读档，不会自动进入列表第一幕。选择前没有剧情生命周期、剧情音频、自动档和已读副作用。运行中 `resume` 沿用续接变量/历史，`restart(frame)` 清空剧情进度；多起始幕不允许无参 restart。布局、路线显隐和解锁条件仍由 XML/JS 组合，见 [多起始幕策略](multiple-entries-implementation-plan.zh-cn.md)。demo content-version=11，旧剧情槽明确拒绝，共享 profile 不撤销。

窗口默认 1280×720、最小 640×360，客户区固定 16:9；拖动边或角不会产生比例黑边。设置页勾选“全屏显示”切换当前显示器的无边框全屏，退出恢复进入前的大小和位置。窗口位置在本次运行内恢复，不跨进程保存；非 16:9 显示器全屏仍等比留边。

工程模式监控 XML/JS 和素材，完整快照、脚本、资源和当前历史验证成功后才替换；坏编辑保留旧状态。契约不兼容必须显式从入口重载，story_id 改变需重启。发布模式只读 exe 旁的 `game.kpak`，不监控散文件，坏包直接失败。

## 剧情与呈现

每句是独立完整 Frame；纯 Controller 通过隔离 QuickJS 计划验证并原子提交变量/目标/历史。逐字按 Unicode 字素簇推进，继续先跳转场、再补全、再推进；选项不默认选择。支持 Ctrl 只读/全部快进、自动播放、历史、CG、图片效果和立绘动作。菜单、提示、失焦和截图等待会正确暂停或阻挡输入。

Windows 原生 D3D11 负责窗口、滤镜与 Live2D 网格/蒙版合成，GPU 结果直接注册原生纹理；CPU fallback 保留，窗口仍由 D3D11/WARP 呈现。视频使用 Windows Media Foundation，Live2D Core 参数/形变仍在 CPU。GPU 失败报告并回退，设备失效最多一次 WARP 重建。

demo 为 31 场景/4 变量/3 JS/20 页/2 视频/1 模型。Haru/Core 来自用户本地 SDK，字体、模型和 Core 原许可随本地包保留，未公开上传；Core 只从受信任宿主配置/程序旁加载，发布禁止回退源码 runtime。资源统一走 ContentSource，游戏 JS 无文件/网络/原生库/时钟/随机/异步能力。

## 存档与用户数据

200 手动槽每页十个，自动槽十个独立轮换。剧情槽保存位置、变量和历史检查点，加载时完整重放；保存时间与 320×180 PNG 缩略图在同一 JSON 原子替换。

`settings.json`、`profile.json`、`read.json`、`ui-audio.json`、`keybindings.json`、`ui-colors.json` 独立于槽位，不随读档回退，排除出包与监控；启用国际化时另有 `language.json`。幕与页面交接使用固定隐藏 `flow-checkpoint.json`（schema 1，最多 2MiB），不显示在槽位页、不占普通自动槽、不受自动保存开关影响，成功返回后写 inactive 墓碑。已读最多 100000 条、文件 2MiB，超限不破坏旧文件；快进不开放 JS 已读查询。

坏档、坏设置与坏偏好不会覆盖当前 Session 或旧文件；保存使用同目录临时文件与平台原子替换。具体字段和版本见 [数据契约](data-contracts.zh-cn.md)，文件与打包排除规则见 [工程、发布与窗口模式指南](project-release-pack-guide.zh-cn.md#用户数据和限制)。

## 资源与媒体边界

资源包与内存边界、视频临时文件和“准备阶段解码全部已声明资源”的行为见 [验证与限制](limits.zh-cn.md#资源与内存边界)。大体积内容优先使用 KPAK 2 流式包；KPAK 1 小包按用户要求保留读取兼容。当前没有包内直接 seek 播放、网络流或场景级惰性解码。

## 相关文档

- [数据契约](data-contracts.zh-cn.md)
- [XML/JS API](xml-js-api.zh-cn.md)
- [工程、发布与窗口模式指南](project-release-pack-guide.zh-cn.md)
- [验证与限制](limits.zh-cn.md)
