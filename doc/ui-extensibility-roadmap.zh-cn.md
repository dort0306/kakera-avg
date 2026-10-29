# 动态图片、可配置快捷键与三角 HSV：接手入口

方案起草日期：2026-10-29；版本定稿：2026-10-30。**本次功能及后续 HSV 输入/焦点修复统一归入 v0.10.0。** 下文保留起草时的现状与实施分工；当前已实现接口以 [API](xml-js-api.zh-cn.md) 为准，实际验证范围见 [验收记录](ui-extensibility-validation.zh-cn.md) 和 [独立复验](ui-extensibility-independent-review.zh-cn.md)。

## 1. 结论与范围

| 需求 | 当前代码事实 | 决定 |
| --- | --- | --- |
| JS 更改一个图片框的内容，单页展示不同 CG | 已有 image-bind、UI string、ctx.setUi；demo 的 cg-view/cgFull 已这样工作 | 复用 XML/JS；补齐浏览交互、说明和回归，不另造图片路径命令或逐图页面 |
| 玩家自定义 Ctrl 快进、空格前进等快捷键 | App 直接读 modifiers.ctrl/Key::Space，UI sequence 另有硬编码空格 | 新增有界输入映射、按键捕获和独立偏好；实际页面与按钮仍用 XML/JS |
| 内置三角形 HSV 颜色选择器 | 没有对应 Kind；UI 值只有 int/bool/string；颜色目前是静态 Node.color | 新增控件几何/命中、RGBA 字符串绑定缓存及显式配色保存；业务仍通过 UI 事件事务提交 |

对应文档：

| 工作包 | 落地方案 | 验收方案 |
| --- | --- | --- |
| A 动态图片与 CG 单页查看 | [落地](dynamic-image-viewer-implementation-plan.zh-cn.md) | [验收](dynamic-image-viewer-validation-plan.zh-cn.md) |
| B 玩家快捷键 | [落地](configurable-shortcuts-implementation-plan.zh-cn.md) | [验收](configurable-shortcuts-validation-plan.zh-cn.md) |
| C 三角 HSV 与配色 | [落地](hsv-triangle-picker-implementation-plan.zh-cn.md) | [验收](hsv-triangle-picker-validation-plan.zh-cn.md) |

## 2. 当前依据与读取顺序

先遵守根 AGENTS.md 的版本文档读取顺序，再读本入口及三个工作包。近期补充依据包括 [多起始幕](multiple-entries-implementation-plan.zh-cn.md)、[确认与退出](confirmation-quit-implementation-plan.zh-cn.md)、[CG 资源](cg-resources-implementation-plan.zh-cn.md)、[当前 API](xml-js-api.zh-cn.md)。不要覆盖工作区已有未提交改动。

本轮核对的是当前源码和 Cargo.lock：egui/ecolor 为 **0.33.3**，不能按历史方案中出现的其他版本猜 API。依赖源码的 color_picker 使用二维区域；不能把普通矩形 SV 面板验收成三角 HSV。接手时再次核对锁定版本，方案不要求升级依赖。

关键入口：src/interface.rs、src/interface/source.rs、src/interface/render.rs、src/interface/data.rs、src/interface/host.rs、src/script/bridge.js、src/app.rs、src/app/custom.rs、src/app/ui_media.rs、src/app/skip.rs、src/render/window.rs、src/package.rs。具体修改范围见各包。

## 3. 冻结的共同约束

1. Model/Controller 不增加键盘、颜色控件、纹理或窗口对象。绘制读取已校验缓存，返回至多一个业务事件；不在绘制内运行 JS、写偏好、解码图像或创建资源。
2. 首选现有 XML/JS。A 没有已确认的 Rust 缺口；B 的设备输入和 C 的连续几何命中确需宿主能力，不能用逐帧 JS 回调绕过。
3. 保留 story 8/UI 3/save 9/profile 1/read 1、KPAK 1/2。新增语法是 UI 3 的受约束扩展；旧播放器不认识时严格拒绝，不恢复旧格式分支。本次正式功能版本及这些扩展的最低播放器版本统一为 v0.10.0，demo content-version 保持 11。
4. 不给 settings.json 补可选字段。拟新增 keybindings.json 与 ui-colors.json，各自 schema_version=1/story_id；由 SaveStore 的固定目录定位，严格字段、大小、值与重复键校验，同目录临时文件原子覆盖。两种文件都排除工程监控、默认打包、显式附加清单及发布目录复制。
5. 偏好独立于普通槽、自动槽、隐藏交接、已读和 CG；不向 JS 开放路径或任意文件写入。拖拽和按键捕获为瞬时状态，不入任何剧情档案。
6. 一次事件仅一种剧情/IO 操作。快捷键偏好可同文件批量修改，配色偏好也可同文件批量修改；**不得做一个按钮同时写快捷键、配色与 settings.json 的伪原子事务**。demo 分别提供应用/取消/恢复默认。
7. 候选无效时保留旧 Session、UI、音频、偏好和输入映射。手势可以本地预览，JS/事务失败必须回到已提交值，不让控件和真实状态分裂。

## 4. 输入优先级与交叉依赖

B 负责建立输入所有权/消费快照：失焦或宿主提示/调试门控 → 快捷键捕获 → 文本/控件编辑 → UI 演出 → 顶层 UI 事件 → 单次剧情按键 → 按住快进 → 自动播放。每层处理后，低层不得再次消费同一物理输入。C 的指针拖拽和键盘调色必须接入该所有权，不复制另一套空格/Ctrl 判断。A 的画廊翻页若以后接键盘，只能经过该边界，不给 JS 任意原始按键流。

推进和跳过 UI 演出可以共用 Space，因为上下文互斥；快进不得选择选项。UI 演出结束/取消、修改键位、失焦和读档等边界的“等待释放”规则由 B 定义，C 不自行重写。

## 5. 实施顺序与任务分界

- A 可先交付，核对已有单图绑定并补浏览演示，无需等待 B/C。
- B 先做纯键位模型/冲突判定，再接宿主捕获、JS 事务与设置页，最后替换所有硬编码入口。
- C 的纯颜色/三角数学可独立准备；进入 UI 事件、焦点与偏好提交阶段前，先接 B 的输入所有权约定。C 内部按控件 → color-bind 预览 → 配色偏好顺序，三个阶段分别验收，完整需求全部完成后才关闭任务。
- B/C 都会修改 interface/host、data、bridge、custom、source 装配和 package 排除表，需顺序集成并复查，不能让两个实现各新增一套偏好加载器、输入路由或根级装配器。

以上是后续工作划分，不表示本轮已经派发或实现。每个接手智能体须先报告现状差异，再按已冻结契约执行；遇到不能保持原子性/预算的变化，先更新方案并说明，不能悄悄换语义。

## 6. 交付门槛

最终执行 cargo fmt --check、cargo clippy --locked --all-targets -- -D warnings、cargo test --locked、cargo run --locked -- --check 和 git diff --check。使用独立测试存档；TCP 测试若被执行沙箱阻挡，单独记录并在获准环境重跑，不能当成功。

两种真实 CLI 包都必须独立重载；UI 新根组/节点需覆盖 include 合并与原位置诊断。Windows D3D11/CPU 原生窗口、中文/16:9/输入消费、关闭重启和坏文件回滚分别按各包验收。CPU 像素模式仍可使用 D3D11 原生窗口，不得据此声称调色器已经有独立 CPU GUI 渲染后端。

交付更新 README、API、工程/发布指南、相应验收记录、打包说明；仅改变 UI/键位/配色时不提升剧情 content-version。若真实更改路由、初值或存档契约，必须另行评估内容版本。不得提交 target、用户偏好或存档，不擅自 Git 提交/推送。

## 7. 本轮状态

2026-10-29 的接手轮只完成代码事实核对与文档设计。随后 **v0.10.0 已实现并验收三个工作包**：A 的复用查看页与三图浏览、B 的纯键位/捕获/偏好/输入所有权、C 的纯颜色几何/色盘/color-bind/palette/偏好。自动测试、`--check`、KPAK 1/2 与 D3D11 实际窗口部分已执行；未执行的 DPI 矩阵、键盘布局、CPU 窗口、长时拖拽、设备音频/听感等必须在 [UI 扩展性验收](ui-extensibility-validation.zh-cn.md) 中保持“未执行”，不得用旧版本测试总数代替本包验收。
