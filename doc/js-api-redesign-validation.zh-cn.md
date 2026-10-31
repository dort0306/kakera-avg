# 新版 JS 注册 API 验收记录（v0.11.0）

日期：2026-10-31。状态：**已实现，自动验证通过，真实窗口复验核心链通过**。对应 [落地方案](js-api-redesign-implementation-plan.zh-cn.md) 与 [验收方案](js-api-redesign-validation-plan.zh-cn.md)，接口见 [API 第 37 节](xml-js-api.zh-cn.md#37-v0110-新版-js-注册-api)。

## 1. 实现范围与版本

- 应用版本 `0.11.0`（Cargo.toml / Cargo.lock / `tools/package.ps1` 默认输出目录同步），Rust 代码未新增 crate。
- 数据契约不变：story 8 / UI 3 / save 9 / profile 1 / read 1 / 隐藏槽 schema 1 / KPAK 2 与 1；demo content-version=11。新语法只存在于 JS 创作层，不写入任何存档。
- 破坏性变更仅限脚本注册入口：旧 `on.advance/choice/end/ui/frameEnter/frameExit/screenOpen/screenClose` 全部删除，没有别名、迁移器或回退分支。
- 改动文件：`src/script/bridge.js`、`src/script.rs`、`src/interface.rs`、`src/interface/source.rs`、`src/interface/render.rs`、`src/interface/host.rs`、`src/loader.rs`、`src/app/{custom,lifecycle,capture,capture,colors,localization,shortcuts,development}.rs`、`locales/engine/zh-CN.ftl`，以及 demo、国际化示例、测试与文档。

## 2. 注册与派发实现要点

- `frame(id)` / `ui(id)` 为不可写、不可配置的全局函数，返回冻结选择器；`advance/choice/end/enter/exit` 与 `bind` 为闭包方法，不依赖 `this`，可安全解构。
- 剧情 manifest 保留 frame/kind/options/location，并新增每条路由的静态目标与调用位置；字符串目标归一为返回固定目标的函数，静态目标在 `ScriptLogic::load` 解析存在性，动态目标在执行边界校验，两者共用同一 `Controller` 事务。
- UI manifest 改为结构化记录 `(category, target, event, location)`，登记表为嵌套 Map（类别→目标→事件），不做文本拼接；`Event` 携带 `UiEventKey`，`render` 用真实控件模板 ID/顶层 page ID 填充，repeat 行不重建注册。
- 装配期校验：每个 XML 事件（控件 `event`、页面背景左右键）必须恰好一个注册；注册必须指向真实目标且事件完全一致；页面与控件同 ID、`$game`/`$host` 作真实 ID、`game.enter`/`screen.open`/`screen.close`/生命周期前缀作普通事件均拒绝；重复注册同时给出两次 JS 位置。
- 生命周期 map 存结构化键；`frame.enter/exit`、`screen.open/close`、`game.enter`、`$host` 捕获结果由宿主分别构造类别，普通事件字符串不能授予生命周期权限。
- 脚本求值失败现在解析 KDIAG 为宿主语言消息（仍带作者文件与栈），注册错误在 `--check` 与打包中可直接阅读。

## 3. 自动验证（对照验收方案）

新增 `tests/js_registration_api.rs`（10 项），复用真实 `loader::load_game`、`Controller` 与 `ScriptLogic`：

| 编号 | 覆盖内容 | 结果 |
| --- | --- | --- |
| R01/R11 | advance、字符串/函数 choice、可选 when、end 的等价路由；`end()` 不退出；变量/历史等价 | 通过 |
| R02 | 幕 ID/name 解析、自循环、跨文件辅助函数 | 通过（`tests/js_triggers.rs`、`tests/story_flow.rs` 与新增用例） |
| R03/R12 | 重复 advance/end、混合路由、重复 option、空/非字符串 ID、非法 targetOrRun、`new String()`、`when` 非函数、未知静态目标、choice 与 XML 不一致 | 通过（全部带 `routes.js` 位置） |
| R04 | 缺幕/未知幕/漏注册、生命周期不补足路由 | 通过 |
| R05/R06 | 条件隐藏、伪造选择、坏返回值/目标、回调抛错与变量回滚 | 通过（`tests/js_triggers.rs` 既有用例） |
| R07 | 全局函数与选择器方法不可覆盖；解构方法仍指向原目标 | 通过 |
| R08 | 回调用已保存选择器再次注册失败；每回隔离、无跨回调状态 | 通过 |
| R09 | 无限循环、堆耗尽、过量更新/命令、Date/random/IO 尝试 | 通过（既有预算用例） |
| R10 | 仅调用 `frame(id)` 不注册；重复构造不等于重复回调 | 通过 |
| R14 | 静态/函数目标 manifest 区分、调用位置、等价迁移 | 通过 |
| U01/U02 | 同名 event 不同目标各自回调；共享函数绑定多目标；`ctx.item/value` 不变 | 通过 |
| U03/U04/U05 | 重复绑定、未知目标、大小写/事件不符、label 绑定、route 与 JS 互斥 | 通过 |
| U06/U07 | 页面背景左右键、repeat 模板行 `ctx.item` | 通过（渲染测试 + 真实窗口） |
| U08 | repeat 运行期 ID、跨页重复 ID、页面/控件同 ID、`$game`/`$host` 真实使用 | 通过 |
| U09/U10 | 跨 include 前向引用；含 `:` 等分隔符的 ID 不产生键碰撞 | 通过 |
| L01/L02/L05/L08 | 生命周期次序、无剧情写权限、同幕续接、screen 生命周期目标校验 | 通过（`src/app/lifecycle.rs` 23 项 + 新增用例） |
| L03/L04/L06/L07/L09/L10 | 末幕退出、视频交接、页面栈预算、game.enter、捕获结果 `$host`、非法目标 | 通过（既有生命周期/演出/快捷键用例） |
| L11/L12 | 首次选择起始幕、热更/渲染重建、坏隐藏槽 | 通过（既有用例） |
| 存档/热更/包 | 固定 ID 槽位、隐藏续接、两种包、源码外检查 | 见第 5、6 节 |

## 4. 必跑命令与结果

```text
cargo fmt --check                                  exit 0
cargo clippy --locked --all-targets -- -D warnings exit 0
cargo test --locked                                294 passed / 0 failed / 2 ignored
cargo run --locked -- --project games/demo --check exit 0（31 场景 / 4 变量 / 3 JS / 19 页 / 2 视频 / 1 模型）
cargo run --locked -- --project games/demo --check --renderer cpu  exit 0
cargo run --locked -- --check                      exit 0（开发构建默认工程 games/demo）
```

2 项 ignored 为既有的 Live2D SDK/真实模型显式用例（需 `CUBISM_SDK`/`LIVE2D_TEST_GAME`），本轮未运行，不计为通过。

## 5. 资源包与源码外检查

```powershell
cargo run --locked --bin kakera-pack -- games/demo target/v0110-game.kpak  --format 2
cargo run --locked --bin kakera-pack -- games/demo target/v0110-small.kpak --format 1
cargo run --locked -- --package target/v0110-game.kpak  --cubism-core runtime/live2d/windows-x64/Live2DCubismCore.dll --check
cargo run --locked -- --package target/v0110-small.kpak --cubism-core runtime/live2d/windows-x64/Live2DCubismCore.dll --check
```

结果：两种格式均打包成功（KPAK 2：49 文件 / 27 340 472 → 22 696 250 字节；KPAK 1：21 556 354 字节），打包过程内置完整候选校验与包内重载；两份包 `--check` 均通过（KPAK 2 / KPAK 1 分别识别）。源码外检查：把 `kakera-avg.exe`（debug 构建）、`Live2DCubismCore.dll` 与包放到仓库外临时目录，以程序旁 `game.kpak` 运行 `--check`，通过且目录内没有散装游戏资源。

旧脚本拒绝（真实 CLI）：

```text
cargo run --locked -- --project target/jsapi-probe --check
  → 启动失败：script.js：QuickJS：Error: on is not defined
      at <eval> (script.js:2:1)                                exit 1
cargo run --locked --bin kakera-pack -- target/jsapi-probe target/jsapi-probe-old.kpak --format 2
  → 打包失败：script.js：QuickJS：Error: on is not defined     exit 1（未生成输出文件）
```

同一探针工程改为 `frame(...)`/`ui(...).bind(...)` 后 `--check` 通过（2 场景 / 1 JS / 1 页）。`kakera-pack` 目前没有 `--help`（位置参数用法见 README），属既有行为，本轮未改动。

## 6. 真实窗口复验

Windows x64，debug 构建，默认渲染后端，`--project games/demo --save-dir target/v0110-gui-saves --no-audio`。截图保存在 `target/gui-shots/`（本地构建产物，不随仓库交付）。

| 步骤 | 观察结果 |
| --- | --- |
| 启动 | 只显示 initial UI（主菜单），中文与 16:9 正常；窗口标题为 UI `title-bind` 值“午后物语” |
| 点击“从午后开始” | `ui('startMain').bind('start-main', …)` 执行 `restart('opening')+open('hud')`；`ui('$game').bind('game.enter', …)` 把标题改为“午后物语 · opening”，HUD 显示开场台词 |
| 点击“继续” | 剧情进入 question；`ui('continue').bind('advance', …)` 单次派发 |
| 点击选项“一起散步” | repeat 行经 `ui('choice').bind('choose', …)` 取得该行 `ctx.item`，执行 `ctx.choose('accept')` → walk；变量 `trust` 增加，标题变“午后物语 · walk” |
| 点击背景 | 背景 `background-advance` 只派发一次并推进 walk→question；选项帧不会被背景点击替玩家选择 |
| 点击“退出游戏” | 打开 XML 模态确认页（标题“退出游戏”，正文与“退出游戏/取消”按钮），先点“取消”回到 HUD，再次确认后进程正常退出（写回 settings.json），未强制杀进程 |
| 点击“保存”→“槽位 1 存入” | 存档页显示 10 槽；写入 `slot-1.json`（37 373 字节，含截图），提示“存档完成” |
| 关闭后重启并“载入存档” | 槽位 1 显示截图、时间与“先散步增进了解…”摘要；点击“读取”后恢复 question、标题“午后物语 · question” |

本轮未在窗口内操作：视频/演出跳过与隐藏交接槽恢复、Live2D 动作、Ctrl 快进与自动播放仲裁、全屏/DPI、D3D11 与 CPU 两个后端的像素对比。这些路径的自动化覆盖见第 3 节与既有记录，但**不视为本版窗口复验通过**；实际设备音频与主观听感本轮未涉及（改动不含音频路径）。

## 7. 未验证与限制

- 未新增 GUI 功能，未复测设备音频流与听感。
- 未做多显卡、高 DPI 跨屏、实际驱动丢失、超大内容压力测试；不援引旧记录当作本轮结果。
- 历史重放与隐藏槽恢复的跨进程结果来自自动化用例（`src/app/lifecycle.rs`、`tests/v090.rs`、`tests/v081.rs`）与第 6 节手动读档；视频隐藏槽未在本轮窗口手动复验。
- 旧脚本没有任何兼容层；作者必须迁移后重建资源包，KPAK 1 的容器兼容不代表旧 JS 内容兼容。
- 手册第 32 节仍有少量以 `frameExit`/`screenOpen` 等阶段名叙述的流程文字（非代码），阅读时以上下文代码块的新写法为准。

## 8. 文档迁移

- `doc/xml-js-api.zh-cn.md`：新增第 37 节完整契约；第 1–36 节的全部可运行示例、签名表与“当前用 on.* 注册”的说明迁移到新语法（保留 `on.*` 的仅第 37 节“旧写法（已移除）”对照表、迁移清单与文件头版本说明）。
- `README.md`：版本、API 摘要、数据契约与验证入口更新；`AGENTS.md` 增加 v0.11.0 约束并同步旧措辞；`doc/project-release-pack-guide.zh-cn.md`、`doc/localization-example/story.js`、`games/demo/scripts/*` 已迁移。
- 历史文档与 `doc/archive` 不改写；`doc/v0.3.1-example` 等旧示例按约定保留。
