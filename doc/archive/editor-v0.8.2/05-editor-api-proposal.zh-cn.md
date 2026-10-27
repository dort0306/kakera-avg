# 05 编辑器 API 设计草案

状态：**全部为拟议接口，不存在可调用服务**。E1 先实现进程内命令接口，任务用有界工作进程；此文不是 HTTP/插件/MCP 框架需求。传输/序列化按实际子进程边界再实现，不能把下表直接当作引擎 API。

## 1. 公共约定

| 字段/类型 | 设计含义 |
| --- | --- |
| projectId | 编辑器会话不透明标识，不是可任意指定的文件路径 |
| revision | 已提交工程修订；修改命令必须带 expectedRevision |
| sourceFingerprint / resourceRevision | 输入文件指纹与资源修订，捕捉外部文件替换 |
| objectRef | kind + stableId；幕引用优先实际 frame.id，导入缺省 ID 另存 name 定位 |
| jobId / requestId / generation | 异步任务、请求去重和快照代次 |
| decimal i64 | 变量整数/大偏移的协议表示，用十进制字符串，避免前端 Number 精度丢失 |
| ContentPath | 验证后的游戏内相对路径；绝对导入位置只经宿主文件选择授权 |

所有更改走 Command → ChangeSet → Validate → Apply，不开放任意文件写入字符串接口。UI 不直接修改内存对象后期待编译器猜差异。

统一结果包含 requestId、基于的 revision、状态、数据和 diagnostics；修改成功返回新 revision 与 undoToken，任务成功返回不可变 artifactId。结果不得泄露凭证/任意机器路径；具体绝对路径仅在作者明确查看宿主诊断时展示。

初始错误码建议：INVALID_ARGUMENT、UNSUPPORTED_ENGINE、REVISION_CONFLICT、SOURCE_CHANGED、RESOURCE_MISSING、RESOURCE_TYPE、SCRIPT_OPAQUE、VALIDATION_FAILED、AI_UNAVAILABLE、AI_OUTPUT_INVALID、CANCELLED、BUDGET_EXCEEDED、IO_FAILED。区分“成功但有待确认项”和“失败未修改”。

## 2. 核心数据对象

| 对象 | 必需字段 | 校验与说明 |
| --- | --- | --- |
| ProjectDescriptor | editorSchema=1、engineContract、contentRootRef、projectId、revision | 版本为编辑器内部 schema，不替换 story schema |
| SourceSpan | file、start/end、line/column、sourceHash | 原稿/XML/JS 分别定位；范围过期不得用于自动补丁 |
| FrameDraft | stableId/name、chapter、displayOrder、text、speaker、visual、audio、route、provenance | 空画面/空音频是明确状态，不能继承旧幕 |
| RouteDraft | kind、options/target、orderedUpdates、guard/branches、managementLevel | kind 为 advance/choice/end；复杂 JS 用 OpaqueRef |
| Expression | valueType、operator、operands/constant/variableRef | 只允许类型化布尔/比较等受限节点；不是任意 eval 字符串 |
| VisualDraft | backgroundRef、portraits、backgroundEffect | portraits 最多三槽，最终 pose 明确；保留 video/Live2D 合法声明 |
| AssetRecord | assetId、kind、relativePath、hash、metadata、tags、license、dependencies | 人工/AI 标签分开；相同名字换内容更新 hash |
| Diagnostic | code、severity、message、objectRef、sourceSpan、generation、phase | 可以没有精确行列，不能伪造位置 |
| ChangeSet | baseRevision、inputHashes、operations、affectedRefs、compatibilityImpact、diagnostics | 不能包含跨管理范围的任意覆盖 |
| BuildArtifact | artifactId、generation、sourceFingerprint、engine/toolHashes、packageHash、manifest、checks | 构建可追溯，不以文件夹存在代表成功 |

FrameDraft 是未提交编辑表示；提交后源文件仍是事实来源。UI 内部对象可用带版本的 sidecar 保存恢复草稿，但不能被播放器或打包器当内容解析。

实现时应显式区分 editorObjectId、frameId 和 name：editorObjectId 用于选区/恢复；frameId 对导入缺省 ID 的幕可以为 null，此时 name 必须存在；编辑器新建幕始终分配一次 frameId。不得为了补齐协议字段自动改写导入内容。资源显示名和相对路径同样不能冒充 assetId。

受限 Expression 首版操作符建议为 Eq/Ne、仅用于 int 的 Lt/Le/Gt/Ge，以及 bool 的 And/Or/Not；左右值类型必须一致，bool/string 不隐式转换或按数字比较。超出此集合保留 Opaque 表达式，不通过 eval 接入编辑器宿主。

## 3. 工程、分镜与资源命令

| 拟议命令 | 输入要点 | 输出与约束 |
| --- | --- | --- |
| Project.open | 宿主已选择的根目录，读取模式 | 描述符、格式检测、迁移/错误清单；坏工程仍可只读打开 |
| Project.create | 目标、游戏名、story_id、当前模板 | 新建当前 XML/JS 内容，不依赖作者安装 Rust |
| Project.refresh | projectId、knownRevision | 外部变更/冲突；不可直接覆盖未保存草稿 |
| Project.save | expectedRevision、ChangeSet | 日志式源码提交；失败保持可恢复一致状态 |
| History.undo/redo | expectedRevision、undoToken | 应用已记录逆操作；与外部改动冲突先报错 |
| Frame.insert/duplicate | 位置、模板或 sourceFrame | 新建一次稳定 ID，不按序号重命名旧幕 |
| Frame.patch | objectRef、字段补丁、expectedRevision | 只改允许字段，显示共享 visual 影响 |
| Frame.reorder | 完整对象顺序、reconnect=false | 默认只改组织顺序；重连路由必须明确开启 |
| Route.patch | 类型化目标/条件/有序更新 | 不可覆盖 Opaque 代码；全部选项与 XML 一致 |
| Asset.import | 已选择文件、类型、逻辑用途、许可 | 流式导入/哈希、依赖和冲突诊断；母版不进 content |
| Asset.relink/replace | assetId、候选文件、影响策略 | 重检所有已知 region/动作/资源引用 |
| Asset.references | assetId、includeUncertain=true | 精确使用项及动态代码不确定项 |
| Asset.describe | 指定资源、视觉能力要求 | 创建 AI 描述任务，不覆盖人工标签 |
| Source.edit | fileRef、原文件 hash、范围补丁 | 保留未知内容；变更受管逻辑后重新判断管理级别 |

“删除”先进入可撤销 ChangeSet，不自动清空作者磁盘原画。UI 可提供彻底清理入口，但不作为 AI 提案默认能力。

## 4. 编译、校验、试玩与构建命令

| 拟议命令 | 输入 | 结果/阶段 |
| --- | --- | --- |
| Validation.run | generation、syntax/content/resources/scenario 范围 | 分级诊断；全资源准备不逐键触发 |
| Compiler.preview | expectedRevision、选中 ChangeSet/全文 | XML/JS 差异、源码映射、预算、兼容性影响，不写源 |
| Compiler.apply | previewArtifactId、expectedRevision、inputHashes | 复查后提交源码；过期预览拒绝 |
| Scene.preview | frameRef、画面配置、预览模式 | 独立构图预览，无合法 Session/存档含义 |
| Playtest.launch | validatedGeneration、隔离数据目录配置 | 外部玩家进程和任务日志；MVP 从入口启动 |
| Playtest.stop | 自己启动的 sessionId | 优先正常关闭；不终止无关播放器 |
| Scenario.run | sourceFingerprint、从入口的动作序列、断言 | 复用真实条件/控制器，不跳过隐藏选项 |
| Build.plan | validatedGeneration、format、压缩级别、发布目标 | 文件清单、预算、许可证/缺失项 |
| Build.run | buildPlanId、expectedRevision | 新临时发行目录、自检报告，成功后完成交付 |
| Job.cancel | projectId、jobId | 协作取消或停止专属子进程；不提交部分产物 |

真正帧级调试命令另见 [06](06-debug-build-release.zh-cn.md)，需播放器扩展；不要在 MVP 的 Playtest.launch 里秘密假设已有任意幕启动能力。

## 5. AI 请求和提案契约

| 对象 | 字段 | 约束 |
| --- | --- | --- |
| AiTask | taskId、taskKind、modelFingerprint、baseRevision、source/resourceHashes、scope、candidateAssets、budget | taskKind 初期仅拆幕/关联/描述/修复提案；范围有界 |
| StorySegment | segmentId、text、sourceSpan、authorHints | 原文保持不变，不把提示当宿主指令 |
| AssetCandidate | assetId、kind、hash、人工/AI 标签、描述、可选 thumbnailRef | 仅白名单资源，缩略图引用由宿主解析为字节 |
| Association | frame/segmentRef、field、selectedAssetId 或 null、alternatives、reason、unresolved | 不存在资源和跨类型关联拒绝 |
| Proposal | schema=1、proposalId、taskId、baseRevision、operations、associations、warnings | 只允许已定义编辑操作；不得带 shell、任意路径或直接覆盖文件 |
| Provenance | 模型/模板版本、输入摘要、原稿范围、作者修改记录 | 简明依据；不是依赖隐藏推理链的审计 |

AI 提案的操作子集建议仅含 createFrame、patchDraftField、assignAsset、setManagedRoute 和 addDeclaredVariable。改变既有 ID、删除原资源、接管 Opaque JS、安装组件和发布均不在白名单。

流程：AiTask.create → 运行 → Proposal.validate → 作者审阅 → Compiler.preview → Compiler.apply。Proposal.validate 的“通过”只说明结构与静态引用合法，不等于艺术关联正确或所有动态路线可达。

## 6. 并发、幂等与预算

修改命令必须同时检查 expectedRevision 与相关文件哈希；模型任务较长，仅比较内存 revision 不够。重复 requestId 返回原结果或明确冲突，不重复导入、生成 ID、写文件或启动构建。

长任务事件建议包含 job.started/progress/diagnostic/completed/failed/cancelled，附序号和 generation；事件只用于工具边界，不引入游戏 Model 消息总线。UI 切换项目后不得把旧任务结果显示到新项目。

初始编辑器协议预算建议：普通请求 1MiB，AI 最终提案 2MiB，单次操作最多 1000 幕变更并分批预览；资产文件走文件/流通道，不 Base64 塞进普通请求。所有预算为拟议，不能扩大当前引擎 XML/JS/解码上限。

错误重试只能重跑无提交任务；IO 处于提交中必须先检查事务日志，不能简单重试一次产生第二份 ID/资源或双重发布。
