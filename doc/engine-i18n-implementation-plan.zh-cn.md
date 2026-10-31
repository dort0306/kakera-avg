# 工作包 A：基于 fluent-rs 的宿主提示国际化

日期：2026-10-30。目标版本 v0.10.1，状态：已接入实现，执行证据见实际验收记录。见 [总方案](v0.10.1-implementation-plan.zh-cn.md) 与 [验收表](engine-i18n-validation-plan.zh-cn.md)。只迁移现有中文，不增加其他语言的生产提示。

## 1. 资源和依赖

采用 Project Fluent 的 `fluent-bundle`、`fluent-syntax` 和 `unic-langid`：分别负责格式化、AST 校验/位置及语言标识规范化。官方文档本次展示 fluent-bundle 0.16.0、unic-langid 0.9.6；**这不是锁定依赖的指令**，实现时核对 Rust 1.88/MSRV、依赖许可证及兼容版本，再提交 Cargo.lock。精确匹配不需要额外引入 fluent-langneg 的近似协商算法。

建议源码资源目录为 `locales/engine/zh-CN/`，按 app/errors/content/save/media/input/cli 分 FTL，显式登记并编译嵌入；不在运行时从 cwd 或游戏来源寻找。全量宿主目录建议最多 128 文件/4MiB，单文件最多 256KiB，词条 ID 最多 10000；超限在构建/测试阶段失败，不无限增大预算。

每个 ID 是稳定语义，如 host-save-write-failed，而非中文本身或行号哈希。按完整句迁移，数量、槽位、资源 ID、路径、原因使用具名参数；不先在 Rust 中拼出半句中文再交 FTL。保持现有提示语义与诊断位置，排版差异必须有明确测试。

当前正式宿主语言集合恰好为 zh-CN，默认也为 zh-CN。未来添加宿主语言才扩展显式清单；游戏作者不能修改这一集合、默认值或宿主翻译。

## 2. 分层与错误传播

拟新增纯诊断结构：稳定 code、message_id、拥有的带类型参数、可选 source locator、子原因链；无 egui、窗口、文件操作、Fluent 对象。放在纯模块，由 Model/Controller 与宿主共同引用，保持 Controller 只依赖其领域边界；不得为了翻译让 Controller 依赖 App。

Model/Controller/规则模块返回诊断数据而非已翻译句子。纯数据可定义在 model 的诊断子模块或同一纯领域边界内。App/CLI/启动错误窗口在输出边界用宿主 Localizer 格式化；技术 code 不随语言变化。不要在内部用 Display 的中文进行错误类型分支或断言。

外部 anyhow/第三方原因不强制结构化重写：以本地化模板包住原始原因，保留文件/行列/JS 栈。嵌套的自身诊断必须递归格式化，不能外层翻译后内部仍由散落的中文 format! 生成。

主线程使用普通 bundle；工程候选准备线程需要诊断时带回拥有的数据，在主线程格式化。确需工作线程格式化时用该线程独立 Localizer，不把 RefCell 版 bundle 跨线程共享或为迁移增加全局 Mutex 服务。CLI 也独立初始化。

## 3. 初始化、格式化与失败策略

1. 解析参数前即可初始化内嵌中文宿主目录，供 --help、未知参数、无游戏、坏目录/包和启动原生错误框使用。
2. 通过 `FluentResource::try_new` 检查全部解析错误；返回部分资源不能当成功。通过 `add_resource` 检查重复消息/术语/属性，禁止 overriding 后加者覆盖。
3. 每个导出消息必须有 value；本版只按消息 ID 导出值，Rust 调用不拼接 `.attribute`。词条内部可使用受校验的术语、引用和 select。
4. 抽取 AST 中的变量/引用，构建每个消息及其引用闭包的参数契约。检查缺引用、循环、同名冲突、没有默认分支、未知函数；不允许 FTL 自定义宿主 IO、时间或随机函数。本版不使用 DATE/TIME 或任意函数调用；需要数字展示使用数值参数/select，不隐式开放系统时钟。
5. 每次格式化必须检查 FluentError 列表；输出了字符串不代表成功。错误不返回部分输出或 `{unknown-id}` 给用户。
6. 生产中文目录在测试/--check 中有任何上述错误都必须失败。发布二进制理论上的运行期词条缺失/参数错误，记录稳定 code 和上下文，用独立、内嵌的中文紧急提示兜底，避免提示系统递归调用自己。紧急提示是唯一批准保留的固定人类提示文本，集中在一个位置并列入白名单；错误详情保留日志，不 panic 或静默吞错。

格式化门面返回拥有的 String，禁止 borrow 的 Pattern/Cow 或 FluentResource AST 离开本地化模块。Model、Session、存档、JSONL 调试协议不序列化 bundle、引用或翻译缓存。

## 4. 参数、隔离符和预算

参数使用受限的 string、bool 对应的稳定 selector 值及可精确表示的数值。大整数 ID/计数若超过数值库的精确区间，应按十进制字符串展示，不强转 f64 丢失精度；只有需要 plural/select 的小整数走数值类型。消息 ID/参数名不接受任意路径或表达式。

每次最多 16 个参数，单字符串最多 4KiB，总参数最多 64KiB；格式化输出最多 64KiB，已有标题/节点更小限制优先。翻译目录引用深度最多 32，循环预检拒绝；采用 Fluent 本身的解析/求解边界，不开放作者函数调用导致绕过预算。实现时验证输出截断不会在参数构造阶段先无界分配；超限返回诊断，不悄悄截掉文本。

不能只在 format_pattern 返回后检查长度：无循环的重复引用也可能指数膨胀。准备阶段对引用 DAG 和各 select 的最大分支做饱和计数，结合本次有界参数长度，预先计算保守展开/输出上界；最多 100000 条引用边，超过输出上限或计算预算即拒绝，再调用 Fluent。无需运行 JS 或自定义求解器，保留 Fluent 自身求解错误检查。

保留 Fluent 默认的双向文本隔离策略，不全局关闭 isolating，不把格式化结果再当程序语法。中文带文件名/数字的精确测试要考虑 U+2068/U+2069；字形验证应识别受允许的 Unicode 格式控制符，不要求给不可见隔离符提供可见字形。其他任意控制字符仍按当前目标约束校验。

## 5. 迁移覆盖面

| 范围 | 必须迁移的输出 | 不能错误地迁移 |
| --- | --- | --- |
| main/launch_console/pack | 帮助、使用方式、状态行、失败上下文、原生错误框标题 | 命令行参数名、程序名、退出码 |
| loader/content/interface/source/package | 自有 XML/引用/预算/路径/容器诊断，保留源位置 | XML 属性/节点、schema、资源 ID、摘要 |
| model/controller/rules/script | 自有规则/更新/隐藏选项/脚本预算提示 | JS 注册名、栈的原始脚本内容、稳定诊断 code |
| save/settings/profile/read/flow | 坏档、旧档、槽位、截图、原子失败、自动档停用提示 | JSON 字段、文件名、用户 excerpt 内容 |
| App/render/assets | 提示框、项目窗、默认状态、存档列表文案、UI 溢出等 | 作者 XML/JS 文本、剧情台词、图像内文字 |
| audio/media/Live2D/D3D11 | 初始化/解码/设备降级和能力限制上下文 | 第三方/OS 原始原因、模型参数 ID |
| shortcuts/colors/debug | 捕获状态、动作显示名、保留键错误、配色错误、调试 UI | canonical 键位 token、颜色 HEX、协议键/认证值 |

先建立 `rg` 清单：所有面向用户的中文字符串、UI_TEXT 字形种子、format!/ensure!/bail!/context/expect/panic 的用途。逐项标为词条、作者数据、测试、注释、开发断言或批准紧急常量。不能只用“中文搜索没有结果”作完成标准，也不能把注释/测试夹具翻译成 ID。

成功路径的宿主绑定（继续/补全/已解锁/页码/空槽/按住 Ctrl 等）在 App/Data 准备阶段生成缓存。重绘不重新格式化不变的状态，不在 `render::draw` 内启动 bundle/读取 FTL。快捷键规范化纯模块返回动作/键名信息，由门面生成易读标签，持久 token 不改。

## 6. 与游戏语言的关系

游戏禁用国际化：宿主走自身默认 zh-CN，作者字面量保持原样。游戏启用且文本为任意其他语言：宿主选“自身支持集合 ∩ 有效文本语言”，不命中仍 zh-CN，不用游戏目录中的同名 key 代替。

以后添加宿主语言时才让此路径产生不同提示；本版不得为完成匹配测试而增加正式英文提示。宿主 CLI 在没有游戏和语言偏好上下文时直接用自身默认；不为单语言框架新增独立 CLI 语言开关。

FTL 与词条参数变化更新缓存代数；提示消息建议保存诊断数据到显示边界，而非长时间保存旧语言格式化句子。生产只有中文时仍测试缓存失效结构，以便后续加入目录。

## 7. 修改地图和交付

新增纯诊断、本地化门面与 locales/engine；逐层迁移输出调用、App/Data 缓存、字体种子及 CLI/native 提示。`assets::validate_text` 依据实际宿主词条/代表参数与游戏字形检查，不能继续只依赖手写 UI_TEXT 清单。

移植前保持稳定 code/参数断言，移植后检查中文、错误源位置/原因链、退出码和 --help。宿主 FTL 内嵌无需入 game.kpak；播放器第三方许可补 Fluent 依赖，资源声明/打包不允许游戏覆盖宿主目录。手册说明“框架已接入，正式提示只有中文”。

## 8. 官方依据

- [fluent-rs 仓库](https://github.com/projectfluent/fluent-rs)：本工作包的库来源，不引入 Web Fluent 运行时。
- [FluentBundle](https://docs.rs/fluent-bundle/latest/fluent_bundle/bundle/struct.FluentBundle.html)：bundle 的语言目录、资源添加和格式化错误需要调用者处理；本文采用应用层显式消息回退。
- [FluentResource](https://docs.rs/fluent-bundle/latest/fluent_bundle/struct.FluentResource.html)：解析失败可携带部分资源，本文明确整批拒绝。

以上链接用于核对库机制；资源预算、诊断结构、嵌入方式和失败策略是本项目设计，不是库自动保证。

实现状态与偏差说明见 [v0.10.1 实际验收记录](v0.10.1-validation.zh-cn.md)。本文中的原代码事实和“拟定”描述保留为设计依据，不代表当前尚未实现。
