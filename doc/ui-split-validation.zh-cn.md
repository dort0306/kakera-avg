# UI XML 拆分验收

日期：2026-10-28。应用维持 v0.8.3，UI 3/story 8/save 8、KPAK 1/2 不变。依据 [实施策略](ui-split-implementation-plan.zh-cn.md)。

## 已交付

- 根级 include、嵌套 UI 3 子文件、分组装配与全局绑定/事件/ID 校验；源文件映射同时覆盖加载诊断和控件/标题运行期位置。
- 通过 ContentSource 的有界读取：128 文件/16 层、源累计及合并结果各 1MiB，保留原全局 UI 预算。
- 两种 KPAK 共用发布装配，通常只保留一个 ui.xml，自定义 UI 入口路径原样保留；仅移除实际子 UI 文件，源工程不改写。
- demo 的 ui.xml 拆成入口和 ui/ 下的状态/12 个页面文件；原 JS、界面布局与资源 ID 保持。原有单文档变异测试改用装配后的 demo UI，未用另一套过时布局代替。
- API 第 7.4 节、README、指南、KPAK 规范和 AGENTS 已同步。发布脚本允许当前 checkout 缺少可选 doc/editor 目录，避免复制不存在路径失败；未恢复用户移走的文档。

## 自动化

cargo fmt --check、cargo clippy --locked --all-targets -- -D warnings、cargo test --locked、cargo run --locked -- --check 均通过。完整测试 152 passed / 0 failed / 2 ignored；默认忽略的真实 SDK 和大包压力用例未计入通过。随后通过 CUBISM_SDK=C:/CubismSdkForNative-5-r.5、LIVE2D_TEST_GAME=games/demo 显式运行 live2d_native，另有 1 项真实 Core 用例通过；大包压力用例未重跑。demo --check 为 30 幕/4 变量/3 JS/12 UI/2 视频/1 模型。使用 CARGO_INCREMENTAL=0 避开本机增量缓存硬链接告警，检查标准未放宽。

新增 tests/ui_split.rs 的 7 组测试覆盖：

1. 嵌套/根相对路径、跨文件前向变量/CG/页面/事件引用、实体、真实 JS 标题更新、控件原行列与合并后重载。
2. 跨文件重复页面/控件/CG、变量和分组冲突、未知绑定/字段、坏 UTF-8/XML，并保留作者文件和准确语义位置。
3. 缺省/非法/越界/大小写重复/循环路径、未知属性/子节点/文本、非根 include、子根禁止入口属性/错误 schema/命名空间拒绝。
4. 文件数和深度的允许边界与超限，累计字节，以及跨文件聚合后的 64 页/1024 控件/128 变量/256 CG 预算。
5. 真实 CLI 两格式独立完整重载/块验证，无子 UI 碎片，JS/额外 notes.xml 保留、源字节不变、重复构建字节相同；低层两种发布路径保留自定义入口及无关同名 XML。
6. 子 UI 在固定候选后改变时原子输出失败，原包 OLD 保留。
7. 原生目录监控：子文件有效修改成功产生候选、坏 XML 拒绝、修复后恢复，剧情 Session 契约保持。

重复发布的确定性测试先关闭读取旧包的句柄，再执行 --force；本轮没有将 Windows 上仍被读取时的包替换行为列为已通过。

## 真实窗口与发布

- D3D11 硬件窗口实际显示拆分 demo 的中文 HUD/台词，客户区 1280×720（含边框/标题截图 1282×751），未见比例/文字溢出。
- 点击来自 ui/settings.xml 的设置页并返回，中文内容与开关布局正常；公开状态 API 同时确认 settings 位于顶层、menu_paused=true，返回后栈为 hud。
- 解锁后完成真实热更目视补验：question 幕中只改 ui/hud.xml，左上文字从“UI 分文件热更新”变为“子 UI 热更已生效”；引入未知属性后显示“工程状态·更新失败”，旧文字/人物/台词/选项保留。API 同时确认幕、变量、历史、标题与 presentation_revision 不变。修复属性并改为“子 UI 修复成功”后，窗口重新显示新文字、错误状态消失，幕/变量/历史保持、修订递增。
- 真实选择“散步”后在独立 save.xml 页面写入空手动槽 1；JSON 确认为 walk、trust=1、met=true、mood=期待、invited=false、历史 3 条和 320×180 PNG。通过 Alt+F4 完全退出工程播放器（退出码 0），再启动源码外 KPAK 2 发布进程并使用同一隔离数据目录。
- 发布窗口的独立 load.xml 页面显示槽 1 的时间、摘要和缩略图；加载后恢复 walk 台词/人物和标题，history.xml 页面实际展示上述四个变量与 opening/question/walk 三条历史。继续后回到 question，“邀请凛”保持隐藏（trust=1），常规选项正常。
- 发布窗口同样通过 Alt+F4 正常退出，退出码 0。解锁后的两次 GUI 验收进程均正常关闭，未操作用户原有实例。
- release 构建及 tools/package.ps1 -Output target/package/kakera-avg-v0.8.3-ui-split -CompressionLevel 3 成功，未覆盖旧发布目录。KPAK 2 为 48 文件/原文 27298407 字节/包 23758894 字节；直接解析真实索引确认仅有 ui.xml，没有 ui/ 子 XML。真实 demo KPAK 1 为 23613092 字节。
- 源码外副本 C:\Users\alone\AppData\Local\Temp\kakera-ui-split-5m6c1eds\release：程序旁 KPAK 2 自动发现和显式 KPAK 1 的 --check 均退出 0，均加载 30 幕/12 UI/2 视频/1 模型；无游戏散文件，Core 使用程序旁 DLL。
- 日志：target/ui-split-clippy.log、ui-split-tests.log、ui-split-check.log、ui-split-sdk.log、ui-split-package.log、ui-split-package-v1.log、ui-split-outside-check.log、ui-split-package-index.log、ui-split-live-api.jsonl、ui-split-resumed-gui-validation.json。令牌只在 target 本地文件，不在状态日志和发布包中。
- git diff --check 与本轮新增文档的本地链接检查通过；当前 checkout 原有的缺失 editor 文档链接未在本轮恢复或宣称全部有效。


先前锁屏阶段的隔离进程曾按公开状态 PID/完整路径核实后清理；该次清理不算 GUI 退出验收。用户解锁后已重新启动并完成上述目视热更、修复与跨进程手动读档。

## 未覆盖

本轮已补验单个手动槽的跨进程恢复；未完整重做全屏/跨 DPI、200 槽、自动保存/快进/视频/模型的所有人工矩阵；本次没有改渲染、音频或剧情路由。GUI 使用 --no-audio，设备流和主观听感不作通过声明。未公开上传、提交或推送 Git，保留工作开始前 Cargo.lock 的排序变化。
