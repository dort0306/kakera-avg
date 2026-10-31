# 版本说明

当前应用版本为 v0.11.2。本文汇总各版本已实现的能力与入口；详细实现方案、验收记录和历史手册分别在对应版本文档与 [doc/archive](archive/) 中保留。旧版说明不能当作本轮复测结果。

## v0.11.2 — UI 组合与通用分页

实现三项 UI 组合能力：

- `image` 与 `panel` 统一内部节点表示，作者契约不变。
- 脚本可用 `component.register('x-名称', { props, expand })` 注册加载期组合标签；实例展开为宿主节点，支持局部 ID 前缀、作者子节点、嵌套与前向事件引用。
- 任意 repeat 可用 `page-size`/`page-bind` 分页，并通过 `repeat.<节点ID>.*` 绑定与 `ctx.repeats` 读取页码/页数/范围；`slots` 只按当前页范围读取槽元信息。

接口、预算与限制见 [API 第 38 节](xml-js-api.zh-cn.md#38-v0112-组合标签与通用分页)；设计依据见 [交接总方案](v0.11.2-implementation-plan.zh-cn.md)、[组合标签设计](ui-composition-tags-handoff.zh-cn.md)、[通用分页设计](repeat-pagination-handoff.zh-cn.md)；实际检查与未验项见 [v0.11.2 验收记录](v0.11.2-validation.zh-cn.md)。demo 在“效果演示 → 组合 / 分页”提供可运行示例。story 8/UI 3/save 9/profile 1/read 1 与 demo content-version=11 不变，无旧格式迁移。

## v0.11.1 — 独立事件与可选控件绑定

事件定义与控件解耦：用 `event.register('事件ID', callback)` 独立定义函数，再用 `ui('控件ID').bind('事件ID')` 或 XML `event="事件ID"` 引用。控件事件可省略/为空，函数可独立存在，绑定支持前向引用，共享事件只注册一次。旧普通控件 `.bind(event, callback)` 和捕获结果 `$host` 注册入口已删除，生命周期专用入口保持不变。

剧情继续使用 `frame(id).advance(targetOrRun)`、`choice(option, targetOrRun, when?)` 和 `end()`，字符串目标与函数目标均支持；ctx、权限、预算及数据格式不变。见 [API 第 37 节](xml-js-api.zh-cn.md#37-v0110-新版-js-注册-api)、[落地方案](v0.11.1-implementation-plan.zh-cn.md) 与 [验收记录](v0.11.1-validation.zh-cn.md)。

## v0.11.0 — 新版 JS 注册 API

重做脚本注册层：剧情路由使用 `frame(id)`，UI、宿主与游戏目标使用 `ui(id)`、`host`、`game`；旧 `on.*` 注册不再接受。隔离执行、候选校验、原子提交和 ctx 权限保持不变。接口见 [API 第 37 节](xml-js-api.zh-cn.md#37-v0110-新版-js-注册-api)，实际结果见 [v0.11.0 验收](js-api-redesign-validation.zh-cn.md)。

## v0.10.1 — 国际化

接入 fluent-rs 宿主提示框架（正式资源仅简体中文），新增默认关闭的游戏国际化；作者显式声明文本/语音语言集合及各自默认值，玩家分别选择，两通道独立回退。接口见 [API 第 36 节](xml-js-api.zh-cn.md#36-v0101-国际化)，设计依据见 [落地方案](v0.10.1-implementation-plan.zh-cn.md)，实际验证与未验项目见 [验收记录](v0.10.1-validation.zh-cn.md)。

语言偏好 schema 1 独立保存在 `language.json`，坏文件保留并提示；运行中切换需静止 UI 边界，不改剧情档案。Windows 探测系统首选语言，其他平台目前以作者默认降级；不承诺 RTL/复杂塑形/多音轨。可运行示例见 [示例说明](localization-example/README.md)，Fluent 依赖许可见 [许可清单](../licenses/i18n-notices.md)。

## v0.10.0 — UI 扩展性

- 同一 `cg-view`/`cgFull` 查看页可在已声明图片间切换，支持上一张/下一张、工具栏隐藏与背景恢复。
- 玩家可为前进、快进、演出跳过绑定单键/组合，写入独立 `keybindings.json`；捕获、冲突、整批交换、原子失败回滚和释放屏障均有覆盖。
- 新增 HSL 圆环与正方形 `color-picker`、`color-bind`、作者 `palette` 与玩家 `ui-colors.json`；demo 设置页提供“快捷键…”与“配色…”编辑页。
- 后续按用户要求把选色器从三角 HSV 整体替换为 HSL 圆环与内部正方形：横轴饱和度、纵轴亮度，整个方形可自由点击/拖动；控件自身只保留环、方形和可选透明度条，颜色预览与 HEX 由作者用 `color-bind` 与 `bind` 组合。
- 独立复验发现的三个 HSV 输入/焦点问题已修复：先确定本帧控件输入归属再仲裁剧情；隐藏、禁用或裁剪后清理焦点并等待松键；Tab/Shift+Tab 可在色盘子区与其他控件间导航。修复后 252 项测试通过、2 项忽略，原生窗口已复验键盘调色；DPI 等剩余未验范围见 [独立验收报告](ui-extensibility-independent-review.zh-cn.md)。

接口与限制见 [API 第 33–35 节](xml-js-api.zh-cn.md#33-v0100-动态图片查看组合)，结果见 [UI 扩展性验收](ui-extensibility-validation.zh-cn.md) 与 [HSL 落地方案](hsl-picker-composition-implementation-plan.zh-cn.md)；规划与工作包依据见 [UI 扩展性路线](ui-extensibility-roadmap.zh-cn.md)；选色器方案与历史结果见 [HSL 方形选色器](hsl-square-picker.zh-cn.md)，旧三角仅作历史。story 8/UI 3/save 9/profile 1/read 1/KPAK 1/2/content-version 11 不变；两个新偏好文件排除监控、打包和发布复制。

## v0.9.1 — 末幕退出与内容能力

- 修复末幕 `frameExit`：文字完成后手动继续触发一次，UI 演出可恢复末幕或显式返回其他幕；自动播放与 Ctrl 仍在结尾停止。普通读档/重开重置退出状态。数据契约保持 story 8/UI 3/save 9。
- UI 唤起剧情：普通 UI 回调与自动演出的 `screen.close` 回调中用 `ctx.resume("幕ID或name")` 指定起始幕，用 `ctx.resume()` 从磁盘隐藏槽 continuation 恢复（包括 inactive）。首次无槽须指定起始幕；指定幕沿用变量/历史，不清空变量。成功唤起后同步隐藏槽，失败保留原状态。

见 [v0.9.1 计划](v0.9.1-implementation-plan.zh-cn.md) 与 [v0.9.1 验收](v0.9.1-validation.zh-cn.md)。

### 专用 CG 资源

在 assets 声明 `<cg id="作者标记" src="…"/>`，visual 用 `cg="作者标记"` 显示完整 CG，不与背景/人物混用。专用资源自动加入画廊，`game.cgId` 提供当前标记；在现有 `game.enter` 回调中 `if (ctx.game.cgId) ctx.unlock(ctx.game.cgId)` 可在进入时解锁。`cg.progress` 显示共享解锁数量，`profile.json` 独立于各剧情槽，同标记只计一次。demo content-version 升为 10，save 9/profile 1 不变。详见 [CG 策略](cg-resources-implementation-plan.zh-cn.md) 与 [验收](cg-resources-validation.zh-cn.md)。

### 多起始幕

`story entry="opening secret-entry"` 声明多个起始幕；首次通过 `ctx.resume("opening")` / `ctx.restart("opening")` 显式选择，或 `ctx.load` / `ctx.loadAuto` 读档，不自动进入列表第一幕。选择前没有剧情生命周期、剧情音频、自动档和已读副作用；运行中 `resume` 沿用续接变量/历史，`restart(frame)` 清空剧情进度；多起始幕不允许无参 restart。菜单、隐藏路线显示与解锁仍优先 XML/JS。demo content-version=11，旧剧情槽拒绝，共享 profile 不撤销。详见 [策略](multiple-entries-implementation-plan.zh-cn.md) 与 [验收](multiple-entries-validation.zh-cn.md)。

### 确认弹窗与退出

确认弹窗直接使用 XML 模态 screen：标题可省略，正文/按钮文字可绑定，遮罩与面板通过 RGBA 设置颜色和透明度，图片/点九背景及按钮皮肤由作者配置。`ctx.quit()` 可由普通 UI 事件关闭游戏窗口，走正常退出补写与资源清理，不强制杀进程。详见 [API](xml-js-api.zh-cn.md#自定义确认弹窗与退出游戏)、[计划](confirmation-quit-implementation-plan.zh-cn.md) 与 [验收](confirmation-quit-validation.zh-cn.md)。

## v0.9.0 — UI 视频与生命周期

视频统一到 UI，`frame` 不再接受 `video`。新增幕/页面生命周期回调（现为 `frame(id).enter/exit` 与 `ui(id).bind('screen.open'/'screen.close')`）与 `ctx.present`/`ctx.resume`，可在离幕时自动播放 UI 视频，结束后续接指定幕。

固定隐藏槽 `flow-checkpoint.json`（schema 1）独立保存剧情/待续点和 UI 栈，不显示在玩家槽位页，也不占普通自动槽；重启恢复演出从零播放。save 9、demo content-version=9，旧档拒绝且不迁移。见 [API 第 32 节](xml-js-api.zh-cn.md#32-v090-幕与页面生命周期隐藏交接槽)、[计划](v0.9.0-implementation-plan.zh-cn.md) 与 [验收](v0.9.0-validation.zh-cn.md)。

## v0.8.4 — 按钮路由、UI 演出与窗口标题

- 按钮支持 `route="open:页面ID"` 与 `route="close"`，简单页面导航无需 JS 回调；与 event 互斥，复用原页面栈及事务校验。见 [API 第 30 节](xml-js-api.zh-cn.md#30-v084-按钮-ui-路由) 和 [v0.8.4 验收](v0.8.4-validation.zh-cn.md)。
- UI 演出支持自动/JS 触发的顺序淡入淡出、UI 视频覆盖层及单次结束路由，点击或空格可跳过。HUD 的“演出与角色音效”页提供片头返回原页、片尾返回剧情界面、单音效/随机音效与角色开关示例。音效候选先过滤再随机，全排除静音；偏好独立保存在 `ui-audio.json`，不随读档回退，仅控制 UI 音效。详见 [API 第 31 节](xml-js-api.zh-cn.md#31-ui-演出过场视频与角色音效)、[计划](ui-presentation-implementation-plan.zh-cn.md) 与 [验收](ui-presentation-validation.zh-cn.md)。
- 窗口标题在 ui.xml 根配置 `title="我的游戏"`，或用 `title-bind="ui.windowTitle"` 绑定 UI string，再通过 `ctx.setUi("windowTitle", "新的标题")` 修改。默认 Kakera AVG；标题最多 256 个 Unicode 字符，非空且无控制字符，整批验证后提交，不进入剧情档/设置。详见 [API 第 28 节](xml-js-api.zh-cn.md#28-可配置窗口标题) 及 [标题功能验收](window-title-validation.zh-cn.md)。

## v0.8.3 — 调试 API、UI 拆分与发布选择

- 新增工程调试 API：外部工具可查询运行状态、设置场景断点、暂停/继续和受约束步进。协议见 [API 第 29.1 节](xml-js-api.zh-cn.md#291-调试启动与运行状态)，实际检查见 [v0.8.3 验收](v0.8.3-validation.zh-cn.md)。
- UI XML 支持根级 `<include src="ui/settings.xml" />`，可将状态、画廊和完整页面拆到 `<ui schema="3">` 子文件。路径相对游戏根目录，支持嵌套引入；全局统一校验，拒绝重复/循环与 ID 冲突。KPAK 1/2 打包自动汇总到一个完整 UI 入口，不带子 UI 碎片，不改源工程。写法、预算与限制见 [API 第 7.4 节](xml-js-api.zh-cn.md#74-外部-ui-xml-引入与打包合并)，结果见 [UI 拆分验收](ui-split-validation.zh-cn.md)。
- 打包改为收集引擎实际依赖、约定许可证和显式附加文件，未引用的设计稿、AI 权重或编辑器元数据不自动入包。见 [计划](v0.8.3-implementation-plan.zh-cn.md)。

## v0.8.2 — 点九、图集与场景别名

- 新增源 `.9.png` 点九背景、共享图集区域和打包 UUID。资源/UI 使用专用 `<nine-patch>`；普通图片通过 `region="x,y,w,h"` 或 `region-bind="ui.xxx"` 选取图集，JS 复用 `ctx.setUi` 切换，不重新上传图片。demo：效果演示 → 点九 / 图集。详见 [API 第 26 节](xml-js-api.zh-cn.md#26-v082-点九图与图集区域)。
- `frame.id` 可省略，省略时写稳定 `name` 供 entry/JS 引用；打包候选生成 UUID v4 并保留名字映射，源 XML 不改写。随机 ID 会随独立构建改变；要保持跨构建档案契约请显式固定 id，否则提升 content-version。细节见 [API 第 27 节](xml-js-api.zh-cn.md#27-v082-缺省场景-id-与作者别名)。

## v0.8.1 — KPAK 2 流式资源包

新增面向大体积游戏的 **KPAK 2 流式资源包**，默认打包为 2；播放器保留 KPAK 1 小包兼容。资源包版本独立于剧情/UI/存档，不恢复旧 story/flow/存档解析器。格式见 [KPAK 规范](kpak-format.zh-cn.md)，设计见 [计划](v0.8.1-implementation-plan.zh-cn.md) 与 [验收](v0.8.1-validation.zh-cn.md)。

## v0.8 — 固定比例窗口与单一契约

窗口客户区固定 16:9，拖动窗口边/角不产生比例黑边；设置页可切换无边框全屏，关闭后恢复进入前窗口大小和位置。全屏开关用 XML toggle 和 `ctx.setting("fullscreen", bool)`，默认窗口化，独立设置跨进程保存。非 16:9 显示器全屏仍保持等比画面并留边；不拉伸或裁掉 UI。同时删除旧 story/flow 解析、声明式路由、无 XML UI 的兼容菜单、旧档导入和旧字段补齐，只维护当前契约。见 [计划](v0.8-implementation-plan.zh-cn.md) 与 [验收](v0.8-validation.zh-cn.md)。

## 更早版本

v0.7.3 及更早版本的说明、API 手册和发布指南保留在 [doc/archive](archive/) 与各版本实现计划/验收记录中，例如 [v0.7.3 说明](archive/v0.7.3-readme.md)、[v0.7.3 验收](v0.7.3-validation.zh-cn.md)、[v0.7.2 维护记录](v0.7.2-maintenance.zh-cn.md)。这些记录只作历史依据，旧格式仍被当前播放器明确拒绝。
