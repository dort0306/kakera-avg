# 工作包 B：玩家可配置快捷键落地方案

起草日期：2026-10-29；版本定稿：2026-10-30。本文保留原落地设计，功能及后续修复统一归入 **v0.10.0**。下文现状、拟定接口和分工描述对应起草阶段；当前支持的接口与限制以 [API](xml-js-api.zh-cn.md) 为准，实际通过项和未验项见 [验收记录](ui-extensibility-validation.zh-cn.md) 与 [独立复验](ui-extensibility-independent-review.zh-cn.md)。

## 1. 当前入口与宿主扩展原因

- src/app.rs：直接读取 modifiers.ctrl，ctrl_held 参与快进/自动播放仲裁；空格走剧情输入。
- src/app/skip.rs：FastForward/已读门控依赖调用者提供按住状态。
- src/app/ui_media.rs：点击或硬编码 Space 跳过 UI sequence，结束输入不得穿透剧情。
- src/render/window.rs：Windows winit/egui 输入桥；Controller 不接触设备。
- settings.rs 是完整严格字段格式，不能靠新增可选字段自动补齐旧文件。

现有 XML/JS 无法查询任意键盘输入、按住/松开边沿或可靠捕获物理键，因此需要明确 Rust 输入边界；配置页、按钮、提示和确认仍由 XML/JS 编写。禁止开放任意 on.key、每帧 JS、系统热键注册或键盘记录流。

## 2. 第一版动作集合

| 稳定动作 ID | 默认绑定 | 固定触发方式 | 有效上下文 |
| --- | --- | --- | --- |
| advance | Space | 单次按下边沿，忽略系统 repeat | 正常顶层剧情：跳过转场→补全→推进，每次只一步 |
| fast_forward | Ctrl | 按住，现有约 40ms 节拍 | 正常剧情且满足已读/skip_all；选项/结尾不代选、不推进 |
| skip_sequence | Space | 单次按下边沿，忽略 repeat | 可跳过的 UI 演出，包括视频；输入只被演出消费 |

绑定可以为 null（禁用该键盘动作）；鼠标继续/按钮/取消和“恢复默认”必须保留。第一版每动作一个绑定，不做宏、任意剧情跳转、任意 JS 事件绑定、多段按键序列或可执行脚本。新动作需单独扩展白名单和冲突矩阵，不靠存档中任意字符串自动注册。

系统 Alt+F4 等窗口行为不属于游戏动作，不可重绑或劫持。既有 XML 背景左/右键、普通按钮、调试命令与自动播放语义不变。

## 3. 按键数据契约

使用宿主拥有的规范化键枚举与修饰键集合；存储/JS 边界使用规范字符串，不序列化 winit/egui 的内部枚举名称或整数判别值。

| 项目 | 决定 |
| --- | --- |
| 主键 | Space/Enter/Tab/Backspace/Delete/Insert/Home/End/PageUp/PageDown、ArrowLeft/ArrowRight/ArrowUp/ArrowDown、KeyA…KeyZ、Digit0…Digit9、F1…F12、Minus/Equal/BracketLeft/BracketRight/Backslash/Semicolon/Quote/Backquote/Comma/Period/Slash、Numpad0…Numpad9/NumpadAdd/NumpadSubtract/NumpadMultiply/NumpadDivide/NumpadDecimal/NumpadEnter；除此以外拒绝 |
| 修饰键 | Ctrl、Alt、Shift，规范顺序固定；不接受重复/未知修饰键 |
| 组合 | 如 Ctrl+KeyK；标点使用 Equal/Minus/Slash 等固定 token，避免把加号字符当分隔符 |
| 纯修饰键 | 仅 fast_forward 支持 Ctrl/Alt/Shift 单独按住；第一版不区分左右键，不接受纯双修饰键绑定 |
| 身份 | 优先 Windows 物理键位置，显示层给中文/易读名称；不得保存 IME 文字或用字符内容反推物理键 |
| 长度 | 单绑定最多 64 字节、最多三个修饰键加一个主键；全部 ASCII，拒绝未知/重复字段与多段序列 |
| 保留 | Escape 留给捕获取消；Windows/Meta 单键及其全部组合、Alt+F4/Alt+Tab/Alt+Esc/Alt+Space、Ctrl+Esc/Ctrl+Shift+Esc 禁止；第一版统一拒绝 Ctrl+Alt 组合（含再加 Shift），避免 AltGr/系统安全组合混淆 |

实现时在文档/纯枚举测试中列出完整 token 表，不能让未列出的任意字符串落到默认键。输入表达可接受修饰键的不同顺序，提交时规范化；重复 token、空白混杂、裸分隔符拒绝。配置文件只保存规范结果。

Windows 原生桥应提供本帧物理键 pressed/released/down、修饰键、repeat、焦点及可辨认的 IME/编辑状态。egui 已有 physical_key 足够的键可复用；缺失时不要悄悄按逻辑字符代替，须在能力表中标记不支持。非 Windows 先保证默认可用键的映射与明确限制，不以 Windows 验收代替其他平台。

## 4. 冲突必须按上下文和前缀检查

纯函数检查完整候选映射，而不是逐个覆盖旧绑定：

- advance 与 skip_sequence 都用 Space 合法，因为有效上下文互斥。
- advance 与 fast_forward 的有效剧情上下文重叠，不能绑定同一触发键或具有按住前缀副作用的组合。
- 默认纯 Ctrl 快进与 Ctrl+KeyK 前进应判冲突：先按 Ctrl 的瞬间可能已经快进，不能只比较最终字符串就宣称无冲突。必须先改快进键，才能使用这种前进组合。
- 检查同一主键与修饰前缀的潜在重叠；按住绑定是另一组合的物理按下前缀时，采取保守拒绝。返回动作 ID 和可显示的冲突原因，不自动互换、不覆盖其他动作。
- 交换两个动作的绑定必须在一个候选批次中整体验证，不能因中间值暂时冲突而先写一半。

可用于显示的错误不是任意路径/系统信息。验收必须含“字符串不同但按下前缀冲突”，不能只测完全相同的键。

## 5. 按键捕获与作者 UI

无需再造原生设置窗口。现有按钮触发拟新增命令，读取状态通过新只读快照显示：

| 拟定接口/绑定 | 语义 |
| --- | --- |
| ctx.captureShortcut(action, resultEvent) | 开始一次瞬时捕获；动作必须白名单，resultEvent 必须是已注册普通 on.ui，禁止伪造生命周期事件 |
| ctx.cancelShortcutCapture() | 取消当前捕获，不改偏好 |
| ctx.keybinding(action, bindingOrNull) | 修改候选偏好；同文件可批量聚合，整批校验/原子写后生效 |
| ctx.resetKeybindings() | 单独恢复三个默认绑定并原子保存，不与单项修改混在同一批 |
| ctx.shortcuts | 普通 UI 回调的只读状态：已提交键位及捕获提示，不含原始键盘流 |
| shortcuts.advanceLabel/fastForwardLabel/skipSequenceLabel | XML 可绑定的中文显示文本，不作为可解析的存储格式 |
| shortcuts.captureActive/captureText | 捕获是否进行及提示文本 |

ctx.shortcuts 的对象字段固定为 bindings（与偏好文件相同的三动作对象）、上述三个 Label、captureActive、captureText；XML 只绑定列出的标量字段，不开放任意对象表达式。显示 Label 不回传作 canonical binding，避免中文翻译改变存储契约。

成功捕获后，在下一次可提交边界排一个普通 UI 事件：ctx.item.action 为稳定动作 ID，ctx.value 为规范绑定字符串。取消不发成功事件。页面中用 UI string 保存草稿；捕获并不自动写文件，“应用”才调用 keybinding。清空绑定由明确按钮完成，不能把用户第一次按 Backspace 偷偷当删除绑定。

捕获生命周期固定为：等待启动事件涉及的按键全部释放 → 武装 → 收集一个支持的组合 → 等待该组合全部释放 → 发一次结果事件。纯 Ctrl 等仅用于快进的绑定也在释放后确认；Escape 在任意阶段取消。鼠标按下启动捕获不能被当作键值。

捕获仅允许普通 UI，禁止 game.enter/frame/screen 生命周期自动启动；活动交接、UI 演出、提示/调试阻挡时不启动。绑定到最终候选顶层页面 ID、UI 定义修订及捕获代数；换页、页面关闭、热更、读档/重开、失焦或再次捕获时清掉旧候选，不投递陈旧结果。普通离焦必须取消，不能回焦后把仍按住的键认成新的绑定。

捕获期间普通剧情快捷键、自动播放及其他控件编辑不处理同一键。UI 的取消按钮可以工作；其他鼠标操作先取消并消费这次点击，不让它同时触发原剧情。无后台钩子、无全局监听、无用户不在游戏窗口时的记录。

## 6. 统一输入所有权和单意图

增加 App 层输入仲裁，不让三个现有硬编码入口各自读取键位。顺序与 [总入口](ui-extensibility-roadmap.zh-cn.md) 一致，形成一份本帧消费结果；ui_media、普通推进和 FastForward 使用此结果。

判定游戏按键前考虑顶层页面是否允许剧情键盘、egui 编辑焦点/IME、控件是否已消费该键、失焦/提示/调试、UI sequence、自动截图等待。将来 HSV 控件键盘调色也必须声明编辑所有权。Space 激活按钮后不能再次推进剧情，Enter 不能既确认捕获又开始故事。

映射后仍调用现有 presentation gate、Controller、may_fast_forward 和 FastForward，不复制剧情规则：先跳转场，再补全文字，再尝试推进；已读定义、只读/全部快进、选项/结尾停止都不变。一次 App 更新最多一个业务意图。

## 7. 按住状态、暂停和释放屏障

把 ctrl_held 的语义改为“映射后的快进动作按住”，变量命名不再暗示固定 Ctrl；所有自动播放等待重置也使用该语义。

- 失焦、捕获结束/取消、键位应用/恢复默认、热更、普通加载/重开及 UI 演出退出后，清节拍并要求相关键完全释放后再武装。
- 模态/提示/调试切换清普通剧情按住状态，恢复时不把打开前的旧按下事件重新执行。
- **自动截图 pending 等短暂资源门控只暂停节拍，不要求每步松键**；否则开启 autosave 时按住快进会每步停死。恢复不补算暂停时间、不一次追赶多步。
- 快进释放会重新开始自动播放等待；快进按住时即使未读文本刚好显示完成，也不得让自动播放偷偷推进。
- UI 演出跳过输入消费直到释放；返回幕不复用同一个 pressed/repeat/down 状态。不可跳过的演出不消费成剧情推进，保持阻挡。
- 用户在应用键位的同一帧已经按着新/旧键时，不立即执行任何动作。

## 8. 独立偏好和原子事务

新增固定 keybindings.json，不修改 settings.json 结构。完整文件要求 schema_version=1、匹配 story_id、bindings 对象；bindings 必须恰有 advance/fast_forward/skip_sequence，每值为规范字符串或 null。16KiB 上限，未知字段、重复 JSON 键、缺动作、无效值或冲突均拒绝。

缺文件使用明确默认值，不算坏文件；坏文件保留、提示一次后用默认，不自动覆盖或迁移。以后增加必需动作应显式升级此偏好 schema，不能给旧文件静默补键。编辑/恢复默认始终写完整当前文件，不先删除旧文件。

keybinding/reset 是单独的 Operation::Keybindings 类事务，仅普通 on.ui。先验证整个键位图、所有 UI 局部更新/页面回调和绑定，再准备序列化/大小校验、同目录原子覆盖，最后替换内存映射和 UI 并设置释放屏障。写失败保留旧文件、旧映射和当前 UI；可用独立草稿页让玩家修正重试。禁止与 settings/Profile/配色/剧情/退出等其他操作混写。

捕获是非持久操作，与其他剧情/IO/捕获请求互斥；不能在写新配置的同时又开启下一捕获，避免同帧的键值归属不明。所有路径固定，不暴露给 JS。

## 9. 具体实施范围

| 层 | 工作 |
| --- | --- |
| 新纯输入模块 | 动作/键枚举、规范化/显示、冲突图、边沿/按住仲裁、捕获状态机；不引用 Model 或 IO |
| Windows 宿主/eframe 适配 | 把原始事件转成拥有的输入快照；禁止系统组合，管理焦点/编辑消费 |
| app/skip/ui_media | 替换所有 Ctrl/Space 硬编码；保留节拍/字素簇/自动档门控；捕获事件只排一次 |
| interface/data/host、bridge/custom | 拟定命令和只读快照、事务聚合与错误回滚；生命周期白名单拒绝新操作 |
| 偏好/监控/打包 | 固定文件读取、16KiB/严格解析/原子覆盖；各容器、工程快照与显式附加排除 |
| demo XML/JS | 三个动作的当前值/草稿、更改/取消/清空/应用/恢复默认，独立于音量等设置保存 |

优先复用当前原子 JSON、绑定校验、UI Event 和页面栈，不引入异步 runtime、消息总线或系统插件。新增事件队列必须有界：最多一个活动捕获/一个待投递结果；失效即丢弃，不积累键序列。

## 10. 阶段交付

先提交纯状态/冲突测试，再接捕获和偏好事务；保持默认 Ctrl/Space 行为通过后替换媒体入口，最后做设置页及真实窗口。接口/schema/排除表与 API/README/发布脚本说明同步。完成前搜索全部 Key::Space、modifiers.ctrl、ctrl_held 等残留，逐处说明是测试、系统保留键还是漏接，不能只改 HUD 的一处空格。
