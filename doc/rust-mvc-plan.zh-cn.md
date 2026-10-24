# Kakera Rust：基于台词场景的最小重建方案

设计日期：2026-10-22。

**保留的核心：一句台词对应一个有稳定 ID 的场景；场景描述此刻应该显示什么；控制器选择下一个场景；视图根据当前状态显示画面。** 新项目使用 Rust，首版使用 XML 内容与流转数据，不嵌入脚本解释器。

这份方案根据补充的原始设计制定，替代上一份评估中以剧情命令流为主的建议。旧 C++ 快照继续保留作为历史资料，新核心不要求兼容原 JSX API。

## 1. 首版只做一个可保存的分支短篇

首发平台为 Windows 桌面；固定 16:9 游戏区域，随窗口等比缩放并留黑边。只支持：

- 背景、最多三个固定位置的立绘、人物名、中文台词。
- 点击“继续”或按空格推进；选择按钮进入不同场景；结尾明确显示结束。
- 一个存档槽，保存、关闭程序、重新启动、读档后恢复同一场景。
- 文件错误、缺失引用、坏存档用清楚的消息提示。

首版整句显示文本，不加入逐字动画。音频、转场、已读快进、回滚、脚本、编辑器、资源包和移动端都放到后续版本。这是范围选择，不是永久放弃。

验收样例：`opening → question → accept/refuse`，四个场景、两条分支。示例内容和流转文件已放在 [story.xml](D:/codes/kakera_engine-dev_quickjs/doc/rust-mvc-example/story.xml) 与 [flow.xml](D:/codes/kakera_engine-dev_quickjs/doc/rust-mvc-example/flow.xml)。示例只定义数据，未附带素材或可运行的 Rust 程序。

## 2. 技术选择

| 职责 | 选择 | 首版如何使用 |
| --- | --- | --- |
| 语言和构建 | Rust stable + Cargo，单 crate | 提交 Cargo.lock；普通 struct、enum、函数和模块即可 |
| 窗口、输入、2D UI | eframe + egui，使用 wgpu 后端 | 使用 eframe 的集成，不直接操作 GPU surface 或写渲染循环 |
| XML | roxmltree | 读取树后校验并转换为拥有 String 的 Rust 数据 |
| PNG/JPEG 解码 | image | 只启用需要的图片格式，转换 RGBA 后上传为 egui 纹理 |
| 存档 | serde + serde_json | 自己定义存档结构，不依赖 GUI 自动持久化作为剧情存档 |
| 用户数据路径 | directories | 为游戏 ID 选择用户数据目录；不向安装目录写存档 |
| 应用错误 | anyhow | 启动及 IO 错误加上下文；核心可先用简单的自定义错误枚举 |

核对时 eframe 文档版本为 0.37.0、roxmltree 为 0.21.1。实际实现时选定可构建组合并提交 Cargo.lock，不在新项目里追随浮动的 latest API。eframe 的具体 App 入口按所选版本官方模板实现。

eframe 提供原生窗口和 egui 集成，支持 wgpu；wgpu 有原生 D3D12、Vulkan、Metal 等后端。首版将这些细节留给集成层，不沿用旧工程的 Skia/GLFW/GLAD/ANGLE 配置。[eframe 文档](https://docs.rs/eframe/latest/eframe/)、[wgpu 文档](https://docs.rs/wgpu/latest/wgpu/)。

roxmltree 的只读树适合这种加载一次、转换为领域数据的用途；无需把 XML 节点和借用关系带进整个程序。[roxmltree 文档](https://docs.rs/roxmltree/latest/roxmltree/)。

egui 首先用于实现这个有限的播放器。它不是对完整 AVG 特效和所有文字排版需求的保证；中文字体加载和基础换行在第一阶段就验证，复杂排版与渲染扩展在有需求后再评估。中文字体明确随游戏配置提供，不假定默认字体包含全部中文字符。[字体配置接口](https://docs.rs/egui/latest/egui/struct.FontDefinitions.html)。

## 3. MVC 的实际边界

```text
story.xml + flow.xml
          │ 加载、校验、展开视觉预设
          ▼
Model：Story（静态内容和图）+ Session（当前位置）
          │ 只读当前完整场景
          ▼
View：背景、立绘、姓名、台词、按钮
          │ Advance / Choose(option_id)
          ▼
Controller：验证输入，按流转关系生成新 Session
          │
          └───────────────→ Model

App：装配以上三部分，并调用 Assets / SaveStore 完成 IO
```

**Model** 包括不可变的内容与流转图，以及可变的游戏会话状态。它不包含 egui 类型、GPU 纹理、窗口句柄、脚本对象或文件读写。

**View** 只读取当前场景，输出用户意图。它可以维护纹理与文字布局缓存，但不能直接修改剧情位置，也不能在绘制期间执行进入场景的操作。

**Controller** 接收 `Advance`、`Choose` 等意图，检查当前节点是否允许这类输入，再决定下一个场景。不查窗口、不绘图、不读图片。

**App** 是最薄的一层装配。保存、读档按钮同样输出 UI 意图，由 App 调用存档模块，校验成功后替换 Session。首版无需事件总线、依赖注入框架或通用副作用系统。

XML 中的流转图也是数据。分开两个文件方便创作和审阅；真正保障 MVC 的是 Rust 模块的依赖方向和只读接口，而不是文件后缀或文件数量。

## 4. 每个场景必须能独立还原

推荐术语：领域类型叫 `Frame`，表示“一句台词对应的场景状态”，避免与将来可能存在的章节或长场景概念混淆。UI 和创作文档中仍可直接称“场景”。

每个 Frame 最终包含：

```text
id                稳定场景 ID，例如 ch01.opening
background        可选的背景资源 ID
portraits         完整的立绘列表，各自有资源、角色和固定位置
speaker           可选的人物 ID；缺省表示旁白
text              此刻显示的整句台词
options           此刻可见的选项 ID 与文字
```

**没有隐含的“沿用上一句”语义。** 若最终状态里立绘列表为空，画面就应没有立绘；没有说话人就不显示姓名。直接定位到这个场景和按顺序播放到这里，应得到相同的场景内容。

为减少重复，XML 可以定义视觉预设：某背景加一组立绘。多个 Frame 用 `visual="room.lin"` 引用同一个预设。加载阶段把它展开到完整 Frame；预设不做链式继承，首版也不做局部覆盖或任意插值表达式。

这允许“数据层每句完整、写作层复用背景和立绘”，无需把场景变成 `show/hide/setText` 命令列表。

如果同一句文字在两种画面下出现，应定义两个场景 ID；后续再用文本 ID 复用翻译内容。首版优先让状态明确。

## 5. 内容与流转数据约定

完整样例见示例目录。核心形式如下：

```xml
<!-- story.xml：此刻显示什么 -->
<frame id="question" visual="room.lin" speaker="lin">
  <text>要一起去看看吗？</text>
  <options>
    <option id="accept">一起去</option>
    <option id="refuse">暂时留下</option>
  </options>
</frame>
```

```xml
<!-- flow.xml：用户输入意味着去哪里 -->
<choice from="question">
  <on option="accept" to="accept" />
  <on option="refuse" to="refuse" />
</choice>
```

流转类型只有三种：

1. `next`：普通场景收到 Advance 后进入目标。
2. `choice`：选项场景只能通过合法选项进入目标，Advance 无效。
3. `end`：结尾节点，Advance 不改变状态，界面显示“已结束”。

每个场景必须恰好声明一种流转类型；不使用“缺少 next 就算结束”的隐含规则。选项 ID 只需在所属场景内唯一，目标和场景 ID 在当前故事内解析。

首版不支持 `<script>`、`onEnter`、任意表达式、变量赋值或条件选项。一个确定的分支图就足以验证最初的 MVC 设计。需要好感度或条件分支时，再增加独立的规则模块和显式变量；无需因为当年习惯 JS 就先把解释器装回来。

### 加载器应一次性检查

- XML schema 版本与根节点；未知元素和属性报告错误，防止拼写错误被静默忽略。
- 资源、人物、视觉预设和场景 ID 不重复，所有引用存在。
- `entry` 对应已有场景，流转目标存在。
- 每场景流转类型唯一；带选项的场景对应 choice，选项与路由一一匹配。
- 立绘位置只接受 left/center/right，同一预设不重复占用位置。
- 图片、字体路径按项目根目录解析，不依赖进程当前工作目录。
- 只支持明确定义的普通文本；不把台词内容当 HTML 或脚本。

XML 错误带文件名和行列；资源缺失带资源 ID 与路径。不可达场景先给警告，允许作者保留尚未接入的内容。闭环流转不直接报错，只有缺失目标等结构错误阻止启动。

台词按 `<text>` 文本节点读取。首版约定不含内嵌格式标签；空格不随意折叠。CRLF 规范化为 LF，多行内容使用显式约定，避免将 XML 缩进自动变成游戏文本。

## 6. Rust 只用直接的数据结构

以下是接口草图，不是已经编译验证的代码：

```rust
type FrameId = String;
type ChoiceId = String;

struct Story {
    id: String,
    content_version: u32,
    entry: FrameId,
    frames: BTreeMap<FrameId, Frame>,
    routes: BTreeMap<FrameId, Route>,
}

struct Session {
    frame_id: FrameId,
}

enum Route {
    Next(FrameId),
    Choice(BTreeMap<ChoiceId, FrameId>),
    End,
}

enum Intent {
    Advance,
    Choose(ChoiceId),
}

fn reduce(
    story: &Story,
    session: &Session,
    intent: Intent,
) -> Result<Session, ControlError>;
```

`Frame` 持有已经展开的场景显示数据，资产 ID、人物 ID 等可以在首版继续用 String。项目很小时，少量字符串复制和 map 查询比复杂的借用关系更容易维护。

同一台词场景不合法的输入应保持状态不变或返回清楚的控制错误，不能跳到默认分支。正常结束不是错误。

调用方式：

```text
App 查找当前 Frame
→ View 绘制并返回一个 UI 意图
→ App 把剧情意图交给 Controller
→ reduce 成功后替换 Session
→ 请求下一帧显示新场景
```

从第一天保持以下依赖方向：`model` 不引用其他应用模块；`controller` 只引用 `model`；`loader/save` 引用 `model`；`view` 引用 `model/assets`；`app` 装配它们。

单线程足够。无需一开始引入 `Arc<Mutex<_>>`、`Rc<RefCell<_>>`、异步 runtime、ECS 或跨模块共享可变对象；也无需先设计 Renderer trait、插件 ABI 或消息总线。

## 7. 状态驱动显示不要求首版实现 diff

egui 采用即时模式。每次 UI 绘制读取当前 Frame，重新提交背景、立绘和文字的显示描述；纹理由 AssetCache 保留，不能每帧重新读盘、解码或上传。

这仍符合 MVC：决定画面的是 Model，View 没有剧情状态的修改权限。首次实现可以整体重绘，不需要自行比对每个属性、发送命令或构建虚拟 DOM。

首版固定布局：背景填满游戏区域（等比 cover 并裁剪）；立绘置于三个固定槽位，等比缩放、底部对齐；底部对话框固定高度；姓名和文本使用统一主题；选项用明确按钮。对话框和菜单不写进每句剧情数据中。

文字必须适配约定的对话框。首版若内容溢出，提供清楚的开发提示并要求作者拆句；不让正式内容静默裁掉。字体加载、换行与实际画面需要进行视觉验证，不能仅以 XML 和控制器测试通过为准。

以后若加入渐变和逐字显示，动画进度放在 View 的呈现状态中，最终目标仍来自当前 Frame。若要求中途保存再精确恢复动画进度，才将相应进度升级为存档状态。

## 8. 为什么首版存档很小

因为首版没有变量、脚本副作用、动态画面和随机分支，完整场景可以由静态故事和场景 ID 重建。存档只需要：

```json
{
  "schema_version": 1,
  "story_id": "kakera-demo",
  "content_version": 1,
  "frame_id": "question"
}
```

已提供 [save.example.json](D:/codes/kakera_engine-dev_quickjs/doc/rust-mvc-example/save.example.json)。schema 是存档结构版本，content_version 是作者明确管理的内容兼容版本，story_id 防止跨游戏读档。

加载成功的条件：JSON 合法、版本匹配、故事匹配、frame_id 存在。全部通过才替换当前 Session；失败保留当前游戏状态并显示错误。

首版不承诺内容更新后的自动存档迁移。不兼容版本明确拒绝加载，之后需要时再提供 ID 映射或迁移代码。不能用 XML 序号作为存档位置。

写入放在该游戏的用户数据目录。至少采用同目录临时文件、检查写入结果、平台合适的替换方式与失败恢复；不使用“先删旧档再改名”。Windows 已存在文件的替换语义在实现时单独验证。这里不把普通文件替换等同于所有平台上的掉电持久性保证。

将来增加变量、随机、音乐进度或动态场景覆盖时，这份极简存档也必须扩展。不能在状态已经变复杂后仍声称一个 frame_id 足以恢复全部游戏。

## 9. 新项目目录

建议在现目录下另建 `kakera-rs/`，与旧 C++ 的 `src/` 并存，保留旧快照。第一版只有一个 crate：

```text
kakera-rs/
  Cargo.toml
  Cargo.lock
  src/
    main.rs        启动、项目路径、错误提示
    app.rs         生命周期与装配
    model.rs       Story / Frame / Session / Route
    loader.rs      XML → 校验 → 拥有数据的 Model
    controller.rs  用户意图 → 下一个 Session
    view.rs        当前场景 → egui 画面与 UI 意图
    assets.rs      图片、字体与纹理缓存
    save.rs        存档版本校验与 IO
  games/demo/
    story.xml
    flow.xml
    assets/
      bg/room.png
      characters/lin.png
      fonts/chinese.ttf
      licenses/
  tests/
    story_flow.rs
    save_roundtrip.rs
```

纹理句柄归 view/assets 持有，Model 只知道资源 ID。XML 解析树在 loader 返回前销毁，其他模块不持有 `roxmltree::Node`。这是简化 Rust 生命周期的关键。

先用图片占位与一份许可允许随项目分发的中文字体，不必等美术素材准备完成。上述素材路径是约定，当前只提供数据示例，不代表目录里已经有图片和字体。

## 10. 开发顺序与完成标准

| 阶段 | 只做什么 | 完成标准 |
| --- | --- | --- |
| A：无图形核心 | 数据类型、XML 加载、引用校验、reduce | 示例能从 opening 到 question，再到两个不同结尾；非法选项不改变状态 |
| B：最小播放器 | eframe 窗口、字体、图片缓存、固定布局、按钮/空格 | 同一 Frame 可直接定位并显示；首帧中文正确；图片不逐帧解码；选择点击不同时触发继续 |
| C：可恢复闭环 | 单槽保存、读档、版本检查、清楚的错误提示 | 在 question 保存并完全退出，重启读档仍处于相同画面且选项可用；坏档不破坏当前会话 |
| D：交付样例 | 固定依赖、启动路径、素材清单、README | 从指定游戏目录启动可运行；开发/发布构建读取同一类 XML；能把程序和样例一起拷贝运行 |

A 阶段测试最有价值的内容是：直接定位与顺序进入相同 Frame 得到相同显示数据；两条分支正确；缺失目标、重复 ID、非法选项映射被拒绝。C 阶段测试序列化往返、故事/版本不匹配、未知场景、写入失败保留旧档。避免为普通字段 getter 写大量重复测试。

最先写 `model.rs`、`controller.rs` 和对应测试，然后写 loader。它们不依赖窗口，可以快速决定数据结构是否满足原始想法；之后再接界面，不需要先攻克 GPU 生命周期或脚本 FFI。

## 11. 后续扩展顺序

v0.2：逐字显示、历史记录、手动多槽与音频。历史记录是 Session 的显式数据；音乐等操作绑定明确的场景切换事件，不能放在绘制函数里反复触发。

v0.2 的具体输入行为、历史上限、六槽存档、旧档导入、音频契约与验收矩阵已另行写入 [v0.2 落地策略](v0.2-implementation-plan.zh-cn.md)，保留本文作为 v0.1 原始设计基线。

v0.3：变量与条件路由。把纯显示内容、规则数据和规则求值继续分开；分支输入导致变量更新时，与场景切换一起提交并保存。

v0.3 的声明式类型、条件与更新语法、历史检查点、schema 3 存档和验收矩阵见 [v0.3 落地策略](v0.3-implementation-plan.zh-cn.md)。用户已明确本版先完成规则功能，后续脚本统一选择 JavaScript / QuickJS；本版不嵌入运行时。

2026-10-23 后续需求更新：用户要求以 JavaScript 触发器/回调简化 flow.xml，并在运行时读取内容。当前实现与隔离、存档重放、内容来源接口见 [JS 触发器改造策略](js-trigger-implementation-plan.zh-cn.md)；上段保留作为 v0.3 首轮历史决定。

v0.4：用户要求以外部 XML 定义 HUD、存读档、设置、历史、CG 等界面，通过 JavaScript 添加交互。设计坐标、图片皮肤、只读绑定、页面栈、受约束宿主命令和独立 CG 进度的落地契约见 [v0.4 落地策略](v0.4-implementation-plan.zh-cn.md)，实际窗口结果见 [v0.4 界面验收](v0.4-ui-validation.zh-cn.md)。保留本文的原始 MVC 边界与历史措辞。

仅在规则表达能力确实不足时考虑嵌入脚本。脚本返回路由结果或受约束的状态变更，不持有 UI、纹理或窗口。可变游戏状态仍归 Model，不能退回“脚本执行现场就是存档”的设计。

若今后 egui 的文字或特效能力不再合适，只替换 view/assets 与 App 的集成，继续沿用 Story、Session、流转图与存档模型。首版无需为这种可能性提前构建抽象框架。

本次交付是设计方案与数据契约示例，尚未创建 Rust 应用、安装工具链或编译验证。已检查示例 XML/JSON 的语法、ID 和路由一致性。
