# 工作包 B：可选的游戏文本与语音国际化

日期：2026-10-30。目标 v0.10.1，状态：已接入实现，执行证据见实际验收记录；本文中的节点、字段、命令均为拟定契约。见 [总方案](v0.10.1-implementation-plan.zh-cn.md) 和 [验收表](game-localization-validation-plan.zh-cn.md)。用户已确认文本与语音分别选择。

## 1. 开关、资源域与领域边界

游戏国际化是**内容作者开关**，不是编译 feature，也不是默认开启的播放器设置。story.xml 入口可声明一个空的 localization 元素：enabled 为必需 bool；true 时 src 必需，false 时 src 禁止。元素缺省等于关闭。章节后续文件不允许重复声明该根配置。

src 是游戏根下的语言清单 XML，所有读取走 ContentSource；不扫描 locales 文件夹、不查询在线目录。关闭时不读取语言清单、游戏 FTL 或玩家语言偏好，不探测系统语言来替换作者文本。目录里恰好存在 FTL 也不自动启用。

当前字面量节点仍是本契约的合法输入；开启后可以渐进使用本地化引用，但使用新增 key 语法而开关未开启必须报错，不能显示 key 或悄悄沿用缓存。关闭游戏国际化不关闭宿主 A 的中文 Fluent 框架。

建议在 LoadedStory/Runtime Bundle 的内容定义侧保存拥有的 LocalizationDefinition：语言集合、默认值、稳定对象到消息 key/语音 key 的映射、源位置。Frame/Choice 的现有 String 可保存解析出的开发者默认文本，保持现有纯 Model 查询；当前语言的可见文本从独立的呈现缓存读取，不能覆盖 Story.frames 或 Controller 的会话候选。FTL AST、系统语言、偏好、缓存不进入 Model/存档。

## 2. 拟定清单 schema 1

语言清单根为 localization，属性仅 schema="1"、default-text、可选 default-voice；恰有 texts 和 voices 两个组，无 include、脚本、任意附加分组。

| 节点/字段 | 冻结的含义与校验 |
| --- | --- |
| texts/locale | 1–32 个；id 是规范语言标识，label 是非空、自称显示名（最多 64 Unicode 标量），不是可翻译业务 ID |
| locale/messages | 1–16 个，只有 src，显式有序列出该文本语言的 FTL；同一语言重复文件/大小写重复路径拒绝 |
| locale/font | 1–16 个，只有 ref，引用已声明 font 资源；顺序为当前语言的优先字体链 |
| voices/locale | 0–32 个；id/label 与文本组规则相同；可与文本语言集合不同 |
| voice locale/clip | key 引用已有剧情 AudioCue.voice 使用的逻辑资源 ID；asset 引用已声明 audio 资源，或 silent="true" 表示明确静音，两者二选一 |
| default-text | 必须精确属于 texts，禁止空值或选第一项作默认 |
| default-voice | voices 非空时必需且属于 voices；voices 为空时禁止，表示本游戏没有可选语音通道 |

只区分文本目录与语音映射，不复制整套剧情。帧、人物、选项、资源 ID 不按语言生成新 ID。语音逻辑 key 仍可用原 audio.voice 的资源 ID，避免要求作者创建虚假资产；默认语音映射可以指向同一原资源。一个逻辑 key 在同一语音语言只出现一次；禁止绑定 BGM/SE/视频/图片类型。

开发者默认语音语言必须覆盖所有被剧情引用的 voice key；默认文本目录必须覆盖所有声明的文本 key。非默认语言允许缺条目，按第 5 节回退。未被任何剧情使用的多余 voice key 拒绝；未引用的 FTL 辅助消息允许，但仍须经过语法/引用/预算检查。

语言标识使用 unic-langid 接受的 Unicode language identifier 子集：语言、可选脚本/地区/variants；禁止扩展、私有附加、空白、路径、空串、und。清单只接受规范拼写及 '-'，最多 64 ASCII 字节；不能用中文 label 作为 ID。重复需按解析后的标识比较。解析器不等于完整 IANA 注册表验证，不承诺检查每个子标签现实中是否已注册。

## 3. 文本接入语法与 JS 的边界

| 位置 | 新增的互斥来源 |
| --- | --- |
| frame/text | 原字面量文本，或仅 key 属性的空元素；不能同时正文与 key |
| option | 原正文，或 text-key；保留同一 option.id，禁止正文与 key 同时出现 |
| character | 原 name，或 name-key；说话人 ID 不变 |
| 剧情变量声明 | 原 label，或 label-key；变量名/类型/初值不翻译 |
| UI label/button/toggle | 原 text、原 bind、text-key、text-key-bind 四选一；动态 key 只绑定 UI string |
| UI gallery 条目 | 原 title 或 title-key；专用 CG 自动条目没有显式标题时仍用既有缺省规则，不能翻译 CG ID |
| UI 根窗口标题 | 原 title/title-bind，或 title-key/title-key-bind；只允许入口根，仍非空、最多 256 标量、禁止目标原有控制字符 |

新文本属性只在具备文本语义的节点开放，不向 image/slider/scene 等任意节点扩散。按钮的 route/event 二选一不变；repeat 模板可使用 text-key，动态 key 只接受 ui string，不使用 item 提供任意 message ID。

剧情台词、人物、选项、变量标签、画廊标题及窗口标题在本版只支持**无外部参数的消息**，FTL 内部术语/消息引用可以使用。这样历史重放和槽位预览无需猜测“当时的变量值”。不要为支持玩家姓名插值而擅自用当前变量替换所有历史句子；剧情参数化需后续专门设计其历史契约。

UI text-key/text-key-bind 可包含最多 16 个 text-arg 子节点，每个只有 name 和 value 或 bind 二选一。value 是字面字符串；bind 仅允许已有 ui bool/int/string、只读 cg.total/cg.unlocked/saves.page/saves.pages 及 locale 标量白名单；禁止对象/列表、文件、原始系统语言或任意表达式。名字重复、缺参、多余参、类型不符报源位置。int 超过 ±(2^53−1) 时不得无损数值承诺：需要数值 selector 的调用拒绝；作者可显式传十进制字符串作普通文本，不隐式变浮点。

通过 AST 推导消息引用闭包所需参数；text-args 必须满足所选语言及其实际默认回退消息的完整参数契约。UI 未选中的语言目录也按契约校验。类型判定使用声明路径类型，不通过格式化失败后不断重试猜测。布局/标题/文本输出等已有预算始终优先。

JS 改变翻译 key 与参数继续复用 ctx.setUi。**不新增 ctx.tr 返回格式化字符串**：否则作者把结果保存到普通 UI string，隐藏槽恢复后会冻结旧语言且丢失原消息身份。保持 key/参数为普通 UI 变量，渲染缓存从它们派生；隐藏槽无需新增字段，恢复时按当前语言重新格式化。

没有 key 的字面量是明确不参与翻译的作者内容，所有语言原样显示；未声明的 key 是错误，不是字面量回退。游戏关闭时禁止 text-arg 和新 key 属性；清单/页面/标题/字体位置错误均带原文件和行列。

## 4. 玩家选择与系统语言匹配

每个通道都有 requested（null=跟随系统或作者集合中的精确 ID）、effective、source（manual/system/default/disabled）。选择只返回作者实际声明的 canonical ID，不返回 label，不允许字符串变成路径。

| 请求与系统状态 | 文本/语音各自的结果 |
| --- | --- |
| requested 非 null 且仍在该通道列表 | requested；忽略系统排序，source=manual |
| 持久请求非 null，但新内容已移除该 ID | 对应开发者默认；source=default，保留请求、提示一次；不继续按系统猜测，也不覆盖磁盘 |
| requested=null；系统首选列表第一项命中 | 采用第一项命中，source=system |
| requested=null；第一项不支持、后续项命中 | 按系统优先顺序采用第一个精确命中 |
| 系统列表为空、读取失败、均未命中 | 对应开发者默认，source=default |
| voices 为空 | effective=null、source=disabled；没有语音选择控件，不产生 voice 空字符串路径 |
| 国际化关闭 | 不加载偏好，不进行上述选择；原文字及语音保持现有行为 |

系统数据在启动阶段采样一次，保持原始优先顺序，去除空/非法/重复项（最多 32 项）；Windows 通过 GetUserPreferredUILanguages 的名称列表，不以地区/键盘布局/系统 TTS 音色代替显示语言。读取失败有中文宿主诊断，继续用默认。测试注入纯列表，不修改机器系统语言。

系统来源允许规范化大小写，并适配 '_' 为 '-'；内容作者和玩家持久文件要求规范形式。只做精确匹配，不做缩短/likely-subtags/地区替换：系统 zh-TW 不会隐式命中 zh-CN 或 zh-Hant；开发者需明确提供相应选项，未命中就默认。该限制属于可预期的 v1 规则，后续扩展需改验收表。

两个通道独立运算：系统 [en-US,ja-JP]、文本集合 [zh-CN,en-US]、语音集合 [zh-CN,ja-JP]，自动结果为 en-US 文本和 ja-JP 语音。手动选择 zh-CN 文本不顺带把语音改成 zh-CN。无 CLI 语言覆盖优先级，本版不额外引入 --language。

玩家运行时提交未声明 ID 直接拒绝，而不是“成功后默认”；只有从旧偏好读取的、后来移除的有效 canonical ID 走保留请求的默认回退。回到“跟随系统”提交 null，并用本次启动采样的列表重新计算。

## 5. 三类回退不能混淆

1. **语言选项未命中**：按第 4 节选择开发者默认语言。
2. **已选择语言缺少某消息**：用开发者默认文本语言的同一消息；默认也缺少则整份内容拒绝。不借用其他非默认语言，不显示 ID。一次只在一个 bundle 中求解整条消息，不能混合不同语言的术语语法。非默认目录已有的消息若缺依赖术语、引用循环、解析/格式化报错，应判坏内容，不能假装缺 key 后静默回退。
3. **已选择语音语言缺少 clip key**：用默认语音语言同一 key；显式 silent=true 则真正静音，不再回退。默认 clip 未映射/坏音频/错类型必须拒绝，不能自动静音掩盖作者错误。

default-text 与 default-voice 可不同；整句文本回退与某段语音回退各自记录实际来源，便于工程诊断，但列表中的 effective 仍代表玩家选定的语言，不随个别句子回退而跳来跳去。

## 6. 独立偏好与只读 UI 数据

拟新增固定 `language.json`，与 settings/keybindings/ui-colors 同一 SaveStore 目录。schema_version=1、story_id、text、voice 四个顶层字段必需，text/voice 为规范 ID 或 null；无 schema 默认填充。新建偏好且 voices 为空时 voice=null；若内容后来移除全部语音，旧文件中有效 canonical voice 请求仍保留、effective=null/source=disabled，不把它误判为坏 JSON 而连带重置文本请求。未知/重复字段、非规范/非法 ID、错误类型、超过 16KiB 均拒绝；缺文件使用两个 null；坏文件保留字节、提示一次、以 null/null 计算，不自动重写。

关闭国际化时不读、不写该文件；重开/普通档/自动档/隐藏槽不修改它。有效但已移除的 ID 不是 JSON 损坏，按第 4 节处理。恢复默认含义为恢复 text=null/voice=null 的跟随系统模式，而不是硬设开发者默认标签。

拟新增普通 on.ui 命令 ctx.locale(channel, localeOrNull)，channel 只接受 text/voice。同批可各修改一次，最多聚合两个通道，先构建最终请求和呈现候选；同一通道重复拒绝，不能与 settings、配色、CG、快捷键、剧情/存读档、退出或其他持久 IO 混写。允许同批普通 UI 草稿/提示/页面变化，但先完整验证页面生命周期和最终绑定；生命周期内禁止调用该命令。作者开关关闭时命令拒绝。

拟新增只读 ctx.localeInfo / XML locale 标量：enabled、textRequested/voiceRequested（XML 用空串表达 null，仅作显示，不能当命令合法 ID）、text/voice（有效值，disabled 语音空串）、textSource/voiceSource。ctx.localeInfo 中 requested/effective 使用真实 null，不套用 XML 空串。列表 textLocales/voiceLocales 只有 id/label/selected/default；“跟随系统”按钮由 XML/JS 显式提交 null，不伪造语言 ID。不要向 JS 暴露原始 OS 列表或系统设置。

XML 仍使用既有 repeat/button/event 编写两个选择区、草稿/应用/取消。切换两项可以一次语言事务写同一个文件；不能把“应用全部设置”按钮扩成多文件伪原子写。未启用时隐藏选择区，宿主不替游戏生成强制菜单。

## 7. 换语言事务、逐字与音频

事务顺序：解析/验证请求 → 计算两个 effective → 准备文本/历史/当前页槽位/字体/标题/动态 key 最终缓存 → 校验音频映射与所需 clip 已准备 → 验证页面回调/资源及总预算 → 原子写语言偏好 → 一次提交请求、缓存和有限音频动作。任一准备或写入失败保留旧 UI、语言、Session、语音/BGM/SE、标题、文件与输入状态。

字体候选先准备拥有的字节/字体定义及覆盖校验，不得在写偏好前对活动 egui Context 调用 set_fonts 而破坏回滚。验证后的字体安装、标题与缓存替换只在提交点执行；需要 GPU 准备的资源使用候选持有者，失败释放候选，绘制阶段不创建字体或纹理资源。

| 成功变化 | 显示与声音行为 |
| --- | --- |
| effective 文本不变，仅 requested 从手动改 auto | 保存请求，不重置逐字，不重播声音 |
| effective 文本改变 | 同一帧重新本地化，当前句完整显示；不把字素索引套到另一语言，不触发补全/推进/enter/exit |
| effective 语音改变 | 停止当前剧情语音，不从头播放新语言；下次真正进入帧才按新语言选 clip |
| 只改文本 | 保持正在播放的语音，不停止或重播 BGM/SE |
| 手动/自动槽成功读档 | 用当前语言解析文本/voice，依既有读档规则重建音频；槽中不恢复语言偏好 |

语言事务停止自动播放，清快进计时和键位捕获/picker 抓取，要求键释放；关闭菜单后不补计时或穿透一次前进。完整显示当前句后已读仍遵循“可见完整显示”的既有规则；已读集合跨语言共享，不为了语言建立新 frame ID。

首版只允许安全静止 UI 边界切换：无活动隐藏交接、UI sequence、视频、剧情转场、调试暂停、自动截图 pending。遇到这些状态明确拒绝并保留选择页，不隐式跳过、取消演出或启动第二个写文件事务。没有上述活动时菜单暂停不阻止语言设置。启动可先加载语言偏好，再重建 active 隐藏 UI/从零播放视频；不在运行交接中切语言。

文本改变会改变截图中的可见内容：递增呈现 revision、失效最近截图，手动保存等待匹配的新呈现；语言事务本身不写普通/自动/隐藏槽、不触发 CG/已读文件立即写入。不得把旧截图配给新语言画面。语音变化不触发故事 entered，不调用 AudioPlan::enter 来重播 SE/BGM。

## 8. 历史、槽位、标题与隐藏恢复

本版剧情消息无外部参数，因此可按 history 中 frame/choice ID 从当前文本语言缓存生成历史，无逐帧 JS、无额外重放条件。槽位元数据也按当前文本语言取帧消息；保留每页十手动槽和十自动槽的扫描上限，不为切语言加载 200 个完整 Session。

当前 save 9 **不保存翻译文案或 excerpt**；preview/excerpt 来自扫描时的 frame.text。实现时应把元信息中的内部 frame ID 交给宿主呈现投影，统一覆盖手动和自动列表，最终 JS/界面仍只得到文本，不把内部 ID 当 preview。截图保存时是什么语言就保持原像素，不重写截图；可与当前语言的文字摘要不同，这一行为需在手册说明。不得为“历史原语言”擅自给 save 9 添加可选字段。

特别注意当前自动槽扫描直接组装 JSON/中文 preview，不能只改已包含 frame_id 的手动 SlotInfo。应为自动槽也带回拥有的、未对 JS 暴露的定位元信息，再在 App/Data 统一投影；不需要更改磁盘 Snapshot 或额外扫描槽位。

窗口 title-key 按当前语言准备后验证既有限制；翻译失败不得提前改原生标题。重新打开页面与读档必须走缓存投影，不能让 Data::build 再把 Model 中默认中文覆盖回来。

隐藏槽仍保存 UI key/参数变量，不保存格式化句子、语言选择或 Fluent 对象。恢复时先用独立 language.json 选择语言，再验证隐藏 UI 并格式化；游戏 `ctx.setUi` 写入的普通字面量字符串保持其字面量语义，不自动猜测翻译。源/continuation/原栈、entered/exited、resume_to 的验证完全不受语言影响。

## 9. 校验、字体、缓存和预算

- 清单 XML 最多 1MiB；全部游戏 FTL 最多 128 个/16MiB、单个 1MiB；全部目录合计消息/术语最多 50000，单 ID 最多 128 ASCII 字节；每条引用深度最多 32，循环拒绝。XML 当前节点/深度/文件预算不放宽。
- FTL 只允许 value 导出、术语/消息引用、参数/select；不开放宿主函数、文件、时钟、随机、HTML/脚本执行。全部 locale 都预检语法、引用和参数契约，不能只验证当前系统命中的一份。
- 格式化沿用 A 的 16 参数/4KiB 单串/64KiB 总参及输出上限，更小的原剧情/UI/标题限制优先。消息输出是纯文本，不作为 XML/JS/路径或资产 ID 再执行。
- 仅在准备/提交/需要刷新绑定时格式化；静态每语言缓存、动态按 key/参数/语言/定义代数缓存，有界总显示缓存建议 64MiB。不可先构建全量无界组合再计数；不创建文本语言×语音语言的 N×M bundle。
- 字体按当前文本语言声明链优先，再补宿主中文所需的随包字体；加载/--check 验证每份目录的全部字面字形、选择分支和当前可知参数。禁止依赖系统字体弥补正式目录缺字；动态用户字符串无法穷举，运行期缺字提示并保留旧候选，不声称全 Unicode 支持。
- 允许的 Fluent 隔离符按 A 的格式控制白名单处理。RTL/塑形并未因此获得支持，伪 RTL 验证仅检验安全和可预测失败，不承诺阿拉伯文排版。
- 全部音频仍经已有解码/PCM 总预算准备，不放宽全局限制或为每种语言复制一份同 ID 缓冲。多语音包超过当前预算必须明确失败；本版不顺带新增流式音频线程。

## 10. 热更与两种包

Bundle 候选包含全部语言清单/FTL/字体/voice 依赖，分别验证默认覆盖、所有可选目录和当前请求；修改翻译不得重放生命周期或改 Session。坏候选保留旧文本/语音/标题/缓存。活动 sequence/交接继续延后热更；调试候选按既有 revision 规则失效。

只改变翻译的兼容热更重建当前语言缓存，不重播语音；移除 requested 语言时走明确默认回退并提示，保留请求文件；移除默认、破坏 key 或资源映射时拒绝。作者从关闭改为开启或反向关闭可在完整候选验证后替换；关闭后保留偏好文件但不使用。story_id 改变仍要求重启。

KPAK 1/2 捕获**所有作者声明语言**的依赖，不随打包机系统/玩家选择裁剪；关闭时只捕获已实际使用内容，不因目录存在就收集 FTL。语言清单/FTL 保留各自路径，不按机器语言改写成单语言剧本。章节/具名 UUID、UI include 合并仍走原装配，新增 key/根开关/参数节点不能在合并中丢失，错误源映射必须保留。

`language.json` 及大小写变体排除工程监控、两种默认打包、显式附加清单和发布复制；FTL 则需要纳入原生监控、候选快照、依赖指纹/变化复查。包输出先完整内容/全部块校验再原子提交；坏翻译/源变化保留旧包与源工程。

## 11. 实施地图

| 层 | 工作 |
| --- | --- |
| 纯 localization 选择模块 | 规范 ID、精确匹配、两个独立通道、默认与缺条目规则，无 IO/OS/Fluent 运行对象 |
| Windows 宿主/平台边界 | 有界读取系统首选语言列表，可注入测试；不把设备依赖放 Controller |
| loader/story_chain/interface/source | 根开关、清单、key/args 及跨章/include 源位置，拥有的本地化定义 |
| runtime/assets | 全目录预检、字体和音频准备、静态/动态缓存，复用已有预算 |
| interface/data/host/bridge | locale 标量/列表、只读上下文、受限语言命令和整批绑定检查 |
| App/custom/input/audio/capture | 同文件语言事务、精确停止 voice、逐字/自动播放/释放屏障、截图 revision |
| save/历史投影/隐藏恢复 | 偏好独立加载、槽位当前页文本投影、恢复顺序，save 9/checkpoint schema 不加字段 |
| package/development/tools | 全语言依赖、FTL 热更、偏好排除、两种 CLI/源码外包运行、许可 |
| demo/XML/JS/文档 | 默认关闭演示仍可运行；独立开启样例/测试工程覆盖语言页；正式中文之外宿主词条不添加 |

先纯匹配和无设备测试，再加载/缓存，再同文件事务，最后 GUI/真实语音/打包。每阶段更新验收记录，遇到字体、声道或历史契约无法满足的点先更新设计，不用 `serde(default)`、临时跳转或重播 enter 绕过。

## 12. 官方依据

[Windows GetUserPreferredUILanguages](https://learn.microsoft.com/en-us/windows/win32/api/winnls/nf-winnls-getuserpreferreduilanguages) 提供有序用户显示语言列表；本方案选择它作为 Windows 来源。[unic-langid](https://docs.rs/unic-langid/latest/unic_langid/struct.LanguageIdentifier.html) 说明标识规范化与解析边界；本方案另加清单规范形式与长度约束。Fluent 实际格式化机制沿用 [A 的官方依据](engine-i18n-implementation-plan.zh-cn.md#8-官方依据)。本文的优先级、精确命中、通道回退和事务语义均是项目约定。

实现状态与偏差说明见 [v0.10.1 实际验收记录](v0.10.1-validation.zh-cn.md)。本文中的原代码事实和“拟定”描述保留为设计依据，不代表当前尚未实现。
