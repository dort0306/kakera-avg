# 数据契约

当前应用版本 v0.11.2。以下为唯一现行格式；所有解析器严格校验，未知元素/属性、重复 ID、错误资源类型、缺失引用和选项路由不一致都会报错，并保留文件名与位置。

## 对象与版本

| 对象 | 当前值 |
| --- | --- |
| story | schema 8；固定 `story.xml`，单/多文件均须显式 `next` |
| XML UI | schema 3，必需；可多文件 include，打包时合并 |
| 手动/自动档 | schema 9；demo content-version=11 |
| KPAK | 2（流式）与 1（小包兼容） |
| profile / read | 1 / 1 |
| 其他偏好 | settings、ui-audio、keybindings、ui-colors、language 各 schema 1；隐藏交接槽 schema 1 |

v0.11.2 只在 UI 3 内新增显式语法：已注册的 `x-` 组合标签与 repeat 的 `page-size`/`page-bind`；story/UI/save/KPAK 容器版本均不变，旧播放器仍应拒绝未知标签/属性。组合标签展开、页缓存和页码都是瞬时状态，不写入剧情槽、隐藏交接槽或偏好文件。

## 剧情与 UI

- story schema 8 使用固定 `story.xml` 入口；每份 XML 恰好一个 `next src` 或 `next end="true"`，最多 128 文件/16MiB。`src` 相对游戏根目录，循环、混合 schema、未知字段和同类重复 ID 拒绝。
- `story.entry` 可声明一个或多个空白分隔的 ID/name 作为起始幕，重复或未知目标拒绝；启动只显示 initial UI，不自动进入。
- 场景 ID 是存档契约，不使用 XML 序号。`frame.id` 可省略，但必须写稳定 `name`；工程以 name 作内部 ID，KPAK 1/2 在候选中生成 UUID v4 并保留 name，源 XML/JS 不改写。重新构建会改变随机 UUID，需要跨构建稳定档案时应固定显式 id，或显式提升 content-version，不按别名迁移。
- UI 入口根可写 `<include src="ui/settings.xml" />`，将变量/画廊/完整页面拆到根为 `<ui schema="3">` 的子文件。路径相对游戏根，支持嵌套；最多 128 文件、引入深度 16（含入口），源与合并 UI XML 各 1MiB。循环、重复/大小写重复引入、跨文件重复 ID、未知字段/schema/路径/引用错误拒绝；绑定允许跨文件前向引用。全局画布、initial、title/title-bind 只在入口声明，不支持 screen 内 include 或局部组件模板。
- JS 注册覆盖所有场景并与 XML 选项对应，动态目标在执行边界校验；错误保留 XML 位置或 JS 文件/栈，不承诺穷举动态分支。`component.register` 只在脚本装配阶段注册（最多 128 个），展开在受限 QuickJS 中执行，结果重新走严格校验。
- 新内容使用 entry/scripts/interface，不读取 `flow.xml`。旧 `flow.xml` 仅在 [归档](archive/demo-flow-v0.3.xml) 留档。

## 存档与用户数据

- save 9 保存 generation、当前位置、变量与历史起点检查点；历史最多 1000 条，裁剪推进检查点，读档完整重放条件/更新/目标与终值。每条历史必须有 `resume_to`，可为 `null`；显式 UI 返回边单独校验。
- 手动槽 1–200，十槽一页、共 20 页；只扫描当前页元信息，真正加载时再完整验证。自动槽 `auto-1`…`auto-10` 独立于手动槽，按 generation 轮换，提交后写入，成功读档不写回；失败保留旧档及已提交剧情，提示并停用 autosave。
- 存档截图字段必需，可为 `null`；图像为 320×180 PNG/Base64，压缩最大 256KiB，总 JSON 2MiB。解码预算、块类型和尺寸严格验证；坏图拒绝。`saved_at_unix_ms` 只用于展示本地时间，不向 JS 开放系统时钟。
- 幕与页面交接使用固定隐藏 `flow-checkpoint.json`（schema 1，最多 2MiB），source/continuation 为完整 save 9，含 UI 栈/变量/原栈/序列 ID。它不显示在槽位页、不占普通自动槽、不受 autosave 开关影响，排除监控与发布；成功返回后写 inactive 墓碑，坏槽保留并提示。
- `profile.json`（schema 1/story_id）独立保存 CG 解锁，不被旧剧情槽或重开撤销；`read.json`（schema 1/story_id/frames）保存已读，最多 100000 条/2MiB，写失败只记日志并保留待写标记。
- `settings.json` 必须包含完整的当前字段（含 `skip_all`、`fullscreen`），使用独立单文件原子保存；缺失/损坏时提示并采用默认值，不用 `serde(default)` 猜测旧文件。`ui-audio.json`、`keybindings.json`、`ui-colors.json`、`language.json` 同样严格 schema、原子覆盖、坏文件保留并排除出包与监控。
- 偏好文件不绑定某个剧情槽，不随读档/重开回退；游戏 JS 无文件、网络、原生库、时钟、随机或异步能力。

## 资源包

- KPAK 2 为大内容提供 80 字节头、受限尾部索引、u64 偏移和独立 1MiB 原文 Zstandard 块，块级去重/摘要，按需验证后交付；禁止整包读入内存。KPAK 1 的字节格式/读取兼容保留。
- 默认打包 2，`--format 1` 显式生成小包；发布按头识别 1/2，未知版本或坏包明确失败，不回退散装目录。
- 两种格式的发布装配都合并 story 链到包内 `story.xml`，并把 UI include 展开到原 UI 入口路径（通常 `ui.xml`）；源工程不改写。
- 容器预算：包及逻辑总量 1TiB、单文件 64GiB、100000 文件、1048576 唯一块、索引 128MiB、路径 1024 字节。拒绝路径碰撞、孤立/越界引用、洞/重叠、尾随帧/数据、摘要错误和解压越界。具体二进制契约见 [KPAK 格式](kpak-format.zh-cn.md)。

## 拒绝与迁移

- 从 v0.8 起删除旧 story/flow 解析、声明式路由、无 XML UI 的兼容菜单、旧档导入、默认旧执行器和旧设置字段补齐。旧内容/槽位明确拒绝，不自动迁移或覆盖。
- 修改不兼容内容要显式提升 content-version，不悄悄接受旧档；应用版本提升不等于 story/UI/save/content-version 升级。
- 旧格式只在 [历史归档](archive/) 与历史文档中说明，不作为当前执行分支。

## 相关文档

- [播放器使用指南](player-guide.zh-cn.md)：启动、运行与用户数据。
- [XML/JS API](xml-js-api.zh-cn.md)：作者接口与预算。
- [工程、发布与窗口模式指南](project-release-pack-guide.zh-cn.md)：打包与发布装配。
- [验证与限制](limits.zh-cn.md)：容器边界与未验证项。
