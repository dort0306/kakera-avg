# JS 注册 API 重设计落地方案

事件定义/绑定规则已由 [v0.11.1 独立事件方案](v0.11.1-implementation-plan.zh-cn.md) 覆盖。以下保留 v0.11.0 原方案依据，不再作为普通两参数 bind 的现行契约。

日期：2026-10-31（方案）、2026-10-31 实施并验收。状态：**已实现（v0.11.0）**。实现按本方案第 5 节顺序完成，实际结果、未验证项与证据见 [验收记录](js-api-redesign-validation.zh-cn.md)；接口见 [API 第 37 节](xml-js-api.zh-cn.md#37-v0110-新版-js-注册-api)。

## 1. 目标、依据与范围

将剧情注册改为 `frame('frame-id').choice/advance/end`，将界面注册改为 `ui('control-id').bind('event', callback)`。剧情方法除去前移到选择器的 frame 参数，并允许 advance 的参数、choice 的第二个参数使用目标幕字符串或返回目标幕字符串的函数；其余参数顺序、可选性、回调与返回值约束不变。所有 `ctx` 接口、快照字段、权限和事务限制不变。

依据当前 [API 手册](xml-js-api.zh-cn.md)、v0.9.0/v0.9.1 生命周期与隐藏续接、多起始幕、确认退出、UI 扩展性和 v0.10.1 国际化方案。较早版本只用于理解沿革，不恢复其执行分支。当前基线为 story 8 / UI 3 / save 9 / profile 1 / read 1 / KPAK 1、2，demo content-version=11。

已核对的实现事实：

- `src/script/bridge.js` 的 `on` 同时包含剧情路由、全局 UI 事件、四个生命周期快捷注册；UI 注册表按一个事件字符串索引。
- `src/script.rs` 每次求值新建 QuickJS，重放声明的脚本，再执行 manifest、条件、剧情或 UI 请求；当前 UI manifest 只有字符串集合，剧情 manifest 有位置。
- `src/interface.rs` 的 XML 控件已有全局唯一 ID，但 `Event` 当前只有 name/item/value/route/sound，缺少来源控件。`Definition::events()` 会把同名事件合并。
- `src/loader.rs` 先加载剧情脚本，再取得 UI manifest，装配并校验 UI；`src/interface/source.rs` 保留 include 源映射。
- `src/app/custom.rs`、`lifecycle.rs`、`shortcuts.rs`、`ui_media.rs` 等分别产生普通交互、生命周期、捕获结果和演出结束操作；不能只替换桥接语法。

已有 XML/JS 能复用回调函数，但无法改变宿主注册表、来源事件和校验方式。故需要最小 Rust/桥接改动；不新增游戏命令、网络、文件路径、IO、逐帧脚本或解释器常驻能力。

## 2. 冻结公开语法

### 2.1 剧情路由

| 旧写法 | 新写法 |
| --- | --- |
| `on.advance(frameId, run)` | `frame(frameId).advance(targetOrRun)` |
| `on.choice(frameId, optionId, run, when?)` | `frame(frameId).choice(optionId, targetOrRun, when?)` |
| `on.end(frameId)` | `frame(frameId).end()` |

```javascript
frame('opening').advance('question');
frame('question').choice('walk', ctx => {
  ctx.add('trust', 1n);
  return 'walk';
}, ctx => ctx.vars.trust < 3n);
frame('question').choice('finish', 'ending');
frame('question').choice('invite', 'ending', ctx => ctx.vars.trust >= 2n);
frame('walk').advance(() => 'question');
frame('ending').end();
```

`frame` 接受非空字符串 ID/name，仍由宿主解析真实 ID。不得按声明顺序生成默认路由。`targetOrRun` 的契约如下：

- **字符串形式：**`advance('next-frame')`、`choice('option-id', 'next-frame', when?)`。接受非空原始字符串，目标仍支持现有 ID/name 解析；等价于无变量更新、直接返回该字符串的同步函数，不执行脚本文本或字符串插值求值。
- **函数形式：**`advance(ctx => 'next-frame')`、`choice('option-id', ctx => { /* 更新与分支 */ return 'next-frame'; }, when?)`。沿用原 run(ctx)，返回非空目标字符串，通过 ctx.set/add 收集更新计划；函数不在注册时执行。
- 两种形式均可使用 choice 的可选第三参数 when；when 仍只能是返回 boolean 的只读函数，不接受字符串或 bool 常量。targetOrRun 与 when 不互换，不根据参数类型猜测省略目标的重载。
- 拒绝空字符串、数字、bool、null、undefined、数组、对象、装箱 String 和其他非函数值，不隐式调用 String() 或 toString()。函数返回值仍严格为非空原始字符串，Promise 等无效。
- 静态字符串目标在完整 story 装配后的加载阶段解析并验证存在，错误保留注册文件/栈；动态函数目标在执行边界验证。两者在实际执行时都走同一个 Controller 的条件、更新、目标、历史与候选提交校验，不因目标静态可知而绕过目标条件、隐藏选项或生命周期事务。

`end()` 只声明 End 路由，不执行退出命令，也不等于 `ctx.quit()`。上例中的 walk/finish/invite 均须有对应 XML option，条件隐藏不豁免注册一致性校验。

同一幕只允许一种路由；choice 可分多次声明不同 option，重复 option、advance/end 重复、混合路由或 ID/name 重复指向同一幕均拒绝。路由必须完整覆盖 XML 场景，choice 集合必须与 XML options 相等。生命周期注册不算路由覆盖，也不与路由互斥。

选择器只捕获稳定字符串，不是剧情对象，也不能读取当前 Session。允许保存选择器局部变量后重复注册不同 choice；每个方法保持无业务返回值（undefined），本轮不增加链式方法、Promise 或动态订阅。

### 2.2 UI 的 event 含义

**bind 的 event 保留 XML 当前的作者事件名，不自动替换为 click/change 等 DOM 事件类型。** 例如：

```xml
<button id="continueButton" width="120" height="40" text="继续" event="advance" />
<slider id="textSpeed" width="300" height="36"
        value="settings.text_speed" min="0" max="80" event="text-speed" />
```

```javascript
ui('continueButton').bind('advance', ctx => ctx.advance());
ui('textSpeed').bind('text-speed', ctx => ctx.setting('text_speed', ctx.value));
```

作者也可在 XML 写 `event="click"`，再 bind('click', callback)，但未声明 event 的按钮不会因 bind('click') 自动获得行为。本轮不新增 DOM、冒泡、捕获、事件对象或控件查找 API。XML 的 `bind="game.text"` 是数据绑定，JS `.bind(...)` 是回调注册，手册须分别说明。

注册键为结构化的 **(目标类别、目标 ID、事件名)**，不是全局事件名。两个不同按钮可保留相同 event，并各自绑定不同处理器；同一完整键重复注册拒绝，不能最后覆盖或执行多个回调。

```javascript
function closePage(ctx) { ctx.close(); }
ui('settingsBack').bind('close', closePage);
ui('historyBack').bind('close', closePage);
```

旧的共享事件需要枚举真实目标并复用函数，不能把 `on.ui('close', fn)` 机械替换为 `ui('close').bind(...)`。不支持通配、隐式全局监听或缺目标时回退按事件名查找。

### 2.3 页面背景、repeat 与简单路由

- `ui(id)` 可指向全 UI 装配后的控件模板 ID 或 screen ID；同一选择器命名空间内不得歧义。若现有解析器允许 screen 与节点同 ID，新版须在完整装配时拒绝该交叉冲突，带两个位置诊断。
- screen 的 `background-click="background-advance"` 对应 `ui('hud').bind('background-advance', fn)`；右键同理。事件名不变，item/value 不变。
- 同页左右背景属性若引用同一事件名，只注册一个回调，保持原有“共享事件”语义；输入仲裁仍只派发一次。要区分左右键，作者使用两个事件名。
- repeat 只绑定模板子控件 ID，不绑定运行期实例 ID或行号；每行仍通过原 `ctx.item` 传递条目，通过 `ctx.value` 传递值。折叠/滚动/换页不重新执行注册，不保存 JS 对象。
- label/image/panel 等没有交互事件的节点不能凭 `.bind` 变成可点击；只允许其现有可产生的事件。未知目标、非交互目标、事件名与 XML 不匹配均拒绝。
- `button route="open:settings"/"close"` 继续直接走类型化页面栈事务，与 event 互斥；不得对 route 按钮补一个 JS bind 或让它同时执行 JS。纯声音按钮等已有无 JS 路径保持原规则。

## 3. 非控件回调的完整迁移

为避免新版仍混用 `on`，本方案一并迁移旧注册入口；以下是配套设计决策，均不改变 callback(ctx) 或权限。

| 旧写法 | 新写法 | 语义 |
| --- | --- | --- |
| `on.frameEnter(id, fn)` | `frame(id).enter(fn)` | 原 frameEnter；UI 命令上下文 |
| `on.frameExit(id, fn)` | `frame(id).exit(fn)` | 原 frameExit；UI 命令上下文 |
| `on.screenOpen(id, fn)` | `ui(id).bind('screen.open', fn)` | 原 screenOpen，只接受 screen |
| `on.screenClose(id, fn)` | `ui(id).bind('screen.close', fn)` | 原 screenClose，只接受 screen |
| `on.ui('game.enter', fn)` | `ui('$game').bind('game.enter', fn)` | 原全局进入钩子，保留其独立白名单 |
| `on.ui(resultEvent, fn)`，供捕获结果使用 | `ui('$host').bind(resultEvent, fn)` | 原普通 UI 捕获结果回调 |

`$game`、`$host` 是两个固定虚拟目标，不是 XML 节点、页面、变量或脚本对象；完整 UI 中保留这两个精确 ID，冲突即报错。不新增任意虚拟目标。`$game` 仅接受 game.enter；`$host` 仅接受非保留的捕获结果事件名，不能注册生命周期。其他位置禁止使用 game.enter、screen.open、screen.close 及现有 frame.enter:/frame.exit:/screen.open:/screen.close: 保留编码作为普通 XML 事件或捕获结果事件。

`ctx.captureShortcut(action, resultEvent)` 的参数与输出不变；宿主明确在 `$host` 查找 resultEvent，不在所有控件中搜索。不要求它依附某个可见按钮，也不要求 XML 增加虚构控件。此回调仍按原有普通 UI 命令权限与捕获状态机执行，不能因虚拟目标绕过捕获所有权、取消、失焦、页切清理或释放屏障。

```javascript
frame('intro').exit(ctx => ctx.open('story-intro'));
ui('story-intro').bind('screen.open', ctx => ctx.sequence('story.intro'));
ui('story-intro').bind('screen.close', ctx => ctx.resume());
ui('$game').bind('game.enter', ctx => {
  if (ctx.game.cgId) ctx.unlock(ctx.game.cgId);
});
ui('captureAdvance').bind('capture-advance', ctx => {
  ctx.captureShortcut('advance', 'shortcut-captured');
});
ui('$host').bind('shortcut-captured', ctx => ctx.setUi('shortcutAdvanceDraft', ctx.value));
```

上述 frame.enter/exit **不是**剧情 set/add 上下文，不能由方法挂在 frame 上就获得剧情变量写权限。screen.open/close 不得从普通 UI 事件伪造触发。`game.enter` 原有能力也不能借新入口混入隐藏槽跨文件事务。

顺序、首幕不自动进入、Controller 先验证、末幕手动退出一次、自循环进入、同幕续接不重复进入、页面回调上下顺序、历史不触发生命周期等全部沿用当前实现。演出 finish 仍只有现有 return/open/close 路由，不新增 sequence 完成事件；自然结束与跳过通过原 screen.close 事务单次返回。

## 4. 注册与派发实现边界

### 4.1 桥接与 manifest

1. `globalThis.frame`、`globalThis.ui` 定义为不可写、不可配置函数；返回冻结的轻量选择器。注册逻辑通过闭包捕获目标，不依赖 `this`，解构方法后也不会丢目标。不得暴露私有 Map 或回调对象。
2. 保持每次隔离加载、求值前封存注册的机制。回调执行时通过已保存选择器再次注册、修改全局入口或选择器方法必须失败，且不能产生部分命令。不要缓存 QuickJS 对象进 Rust 长期结构。
3. 剧情 manifest 保留 frame/kind/options/location 等纯数据，并补充 advance/每个 choice 的静态目标及调用位置（函数目标显式标为动态），供完整装配后预检。桥接在注册时区分原始字符串与函数；字符串可在私有注册表归一为返回固定目标的函数，同时保留静态目标元数据。执行仍产出同一目标/更新计划，Rust 复用原控制器校验，不新增第二套静态跳转执行器。UI manifest 改为拥有的注册记录，至少含目标类别、目标 ID、事件名、JS 文件/栈位置；生命周期与普通 UI 使用显式类别。
4. 键使用 tuple/struct 或嵌套 Map，不用未经转义的 `id + ':' + event` 拼接，避免作者 ID 中的分隔符造成碰撞。
5. 重复注册报当前位置与首次位置；未知目标、漏绑定或错事件同时提供 JS 注册位置和 XML 原文件位置。调用点优先落到作者方法调用行，不只显示选择器创建行或 `<engine-bridge>`。

运行预算保留：脚本总字节 1MiB、16MiB QuickJS 堆、256KiB 栈、200ms/10000 次中断限额、剧情更新 128 条、UI 命令 32 条、生命周期派发 16 次。manifest/注册不得在预算外重复运行。Date、随机、异步任务、宿主 IO 等原有封锁保持；不用长驻解释器绕过隔离。

### 4.2 装配和严格引用校验

读取完整 JS 注册数据后，按现有 ContentSource 和 UI include 流程装配 XML，再建立拥有的目标索引。对所有 `(目标, XML事件)` 检查恰好一个匹配注册；对普通注册反查真实 XML 事件。多余/拼错注册也失败，生命周期和固定虚拟目标单独校验，不要求它们出现在 XML event 属性。

保留前向引用与跨文件引用；只有完整装配后才能判断未知目标，不按 JS 或 include 的书写先后误报。生命周期 frame ID/name 通过现有 story 映射归一，按真实 ID 查重。不要为新 UI 键改变剧情目标解析、UUID 生成或 Scene/存档 ID。

### 4.3 事件来源贯穿宿主

普通 UI 事件增加拥有的来源目标标识，render 从真实节点/顶层 screen 填充，repeat 使用模板 ID。App/ScriptLogic 将来源和 event 一起传给桥接；不能留一条 name-only 回退。route/sound 路径与单意图仲裁照旧。

生命周期、game.enter、捕获结果由宿主创建不同类别事件；不能信任普通事件的字符串前缀来授予生命周期权限。具体可用已有 enum 加来源字段，不引入事件总线。JS 仍只接收原 ctx，不增加 ctx.target、ctx.event、dispatch 或 emit。页面栈准备、命令验证、媒体准备、原子 IO、候选提交均复用原事务。

## 5. 改动清单与实施顺序

| 阶段 | 文件/范围 | 可交付结果 |
| --- | --- | --- |
| A 注册层 | `src/script/bridge.js`、`src/script.rs` | 新入口、冻结/封存、结构化 manifest、原路由计划和新 UI 键查找 |
| B 加载校验 | `src/interface.rs`、`source.rs`、`src/loader.rs` | 全局目标索引、精确引用与类别校验、XML/JS 双位置诊断 |
| C 事件链 | `src/interface/render.rs`、`host.rs`、`src/app/custom.rs`、`lifecycle.rs`、`shortcuts.rs`、`ui_media.rs` 及实际 Event 构造点 | 来源贯穿，所有宿主回调迁移，原事务与输入仲裁不变 |
| D 内容与工具 | `games/demo/scripts`、当前测试 fixture、`tools/prepare-localization-demo.ps1` 等生成器与现行示例 | 所有可运行内容一次迁移；旧全局共享回调展开为具体目标复用函数 |
| E 文档与交付 | README、API、工程指南、编辑器当前接口说明、AGENTS、验收记录 | 说明新语法、破坏性脚本变更、无回退、实际验证及限制 |

建议顺序 A→B→C→D→E；最终必须作为一套完整候选交付，不能让 demo 一半用旧 API、一半用新 API。不引入额外 crate、多入口解析器、正则源代码改写或 KPAK 转译。

适配宿主诊断时复用已有稳定 Fluent ID；新语义错误追加新 ID，不重新编号，也不把游戏的语言资源用于宿主错误。同步必要的宿主消息索引。

迁移前扫描 `on.`、`ui_handlers`、`Event {`、生命周期字符串和 `captureShortcut`，包含 src 内嵌测试及生成 JS 的 PowerShell。阅读原回调业务，不只按文件名区分：demo UI JS 内也有剧情 on.end。当前 API 手册所有可运行示例与国际化示例必须迁移；`doc/archive`、v0.1/v0.3/v0.3.1 历史示例及历史验收原文保留。

## 6. 版本、兼容与历史重放

- 新播放器只安装新入口，不保留 `on` 别名、运行时迁移、旧注册器或按错误回退。旧脚本启动/--check/打包必须带文件和栈拒绝；混用新旧注册同样拒绝，已生效的旧工程热更失败时继续旧候选。
- 此为脚本创作 API 的破坏性变更。应用版本在实施交付时统一确定并同步 Cargo.toml/lock 和 package.ps1；本方案不擅自占用版本号，不因规划改版本。交付说明需明确旧脚本资源包必须由作者迁移后重建。
- 不改 story 8/UI 3/save 9/profile 1/read 1/隐藏槽 schema 1/KPAK 1、2。新语法不写入存档。容器兼容不代表旧 JS 内容兼容。
- 纯机械迁移若幕 ID、变量、选择条件、路由结果、生命周期效果和执行次序完全等价，不提升 demo content-version=11；必须用实际历史重放/隐藏槽恢复验证，不能仅凭 schema 相同声称兼容。
- 若实现过程中改变上述语义或存档引用的 UI 栈/序列等契约，必须提升 content-version 并明确拒绝旧档，不迁移、不删除用户文件。本工作包默认不做这些语义改动。
- 固定 ID 内容可以验证迁移前后槽位；省略 ID、独立打包随机 UUID 的内容仍遵守现有跨构建限制，不能借本次 API 重设计承诺稳定。

## 7. 完成条件与本次状态

实施者应按 [验收方案](js-api-redesign-validation-plan.zh-cn.md) 提供自动检查、两种真实包、GUI、跨进程槽位与隐藏续接证据；未测设备流/听感必须单列。完成门槛不是 demo --check 一项成功。实际交付结果见 [验收记录](js-api-redesign-validation.zh-cn.md)。

本方案已按约定实现并交付（v0.11.0）：注册层、装配校验、事件来源、demo 迁移、测试与文档全部完成，README 与 API 手册改为描述当前可运行接口。实现与本方案的偏差仅一处：`ui(id)` 与页面共用同一目标类别（screen.open/screen.close 作为保留事件名绑定在页面目标上），因此注册键为 `(类别, 目标, 事件)` 且类别为 `ui/game/host/frame.enter/frame.exit`。实际验证范围与未执行项见验收记录，不能把本方案当作通过记录。
