# 多起始幕验收记录

日期：2026-10-29。应用 v0.9.1，story 8/UI 3/save 9/profile 1/KPAK 1/2 不变，demo content-version=11。依据 [实现策略](multiple-entries-implementation-plan.zh-cn.md)。

## 实现

- entry 接受空白分隔的多个 ID/name；未知、空列表、重复及别名指向同幕重复均拒绝，保留 story.xml 源位置。逐个入口验证初始选项和 game.enter 计划、立绘及模型命令。
- 启动仅显示 XML initial 页面，不提交默认剧情或执行剧情进入、音频、自动档和已读。首次 resume/restart 必须显式选声明的起始幕，或载入手动/自动档；禁止无参进入、推进、保存和自动播放。剧情绑定及 scene 不显示内部校验起点。
- 首次建立初始变量和单条起点历史，忽略 inactive 隐藏续接内容；active 交接仍完整验证恢复。运行中 resume 保持既有续接，restart(frame) 重置剧情变量/历史；多入口无参 restart 拒绝。调试状态新增 game_started，未进入时 frame_id/frame_name 为 null、历史长度为零，禁止剧情步进。
- demo 主菜单提供两条起始路线、读档和设置；HUD 重开返回选线菜单，加载后切 HUD。菜单与条件显隐只用 XML/JS。工程热更及渲染重建保留未选择状态。

## 自动检查

cargo fmt --check、cargo clippy --locked --all-targets -j 1 -- -D warnings、cargo test --locked -j 1、cargo run --locked -- --check、git diff --check 均通过。完整回归 **188 passed / 0 failed / 2 ignored**。完整测试使用允许 loopback 监听的执行环境；沙箱中调试 TCP 曾报 10013，授权本机监听后单独 6 项及完整回归均通过。新增三项 App 测试覆盖首次 UI 等待、键盘/JS/保存/自动播放阻挡、别名选择、无默认幕历史、初始变量/生命周期、重复/未知/空入口及第二起点回调验证、坏目标/隐藏写入失败回滚、普通读档及显式重开。既有 UUID 两格式打包测试增加第二起点，demo 包测试验证入口列表；旧 UI 测试显式使用已进入 HUD 状态，调试集成测试先通过真实 egui 点击选择起始幕。

测试临时目录使用 target/test-tmp，避免沙箱默认临时目录的原子替换权限限制。最终日志为 target/multi-start-tests-complete.log、multi-start-clippy-final.log、multi-start-fmt-final.log、multi-start-check.log；未将失败的环境运行计入通过。

## 两种真实包

真实 CLI --format 1/2 --level 1 均打包成功：48 文件、原文 27304093 字节，KPAK 1 为 24715480 字节，KPAK 2 为 24719051 字节。源工程未被装配改写。新菜单合并入包内 UI，两个起点均保留。

把最终 debug 播放器、固定宿主 Core 和两个包复制到新建系统临时目录，无游戏散资源；以该目录为工作目录执行两个 --package --check，均退出 0，报告 31 幕/4 变量/3 JS/16 UI 页/2 视频/1 模型。位置记录在 target/multi-start-portable.path，输出在 target/multi-start-portable1/2.log。

## 首次交付的窗口状态（后续已复验）

首次交付时 Computer Use 应用授权超时，未取得截图，当时未验收 GUI；后续按用户要求重新启动真实窗口，结果见下方复验记录。原始失败记录保留，不以自动测试替代实际窗口。

使用 --no-audio，未验收设备流或主观听感；未另行执行默认忽略的 Cubism SDK/600MiB 压力用例，没有生成优化版正式发行目录或公开发布。保留任务开始前的所有工作区修改，无 Git 提交或推送。

## GUI 复验与修复（2026-10-29）

本轮成功使用 Computer Use 操作真实原生窗口。发现主菜单事件先 open(hud) 再 restart/resume 时，宿主曾把事件本身的页面切换误判成 frameEnter/Exit 发起的 UI 交接，导致 HUD 显示但文字时钟暂停、没有自动档。现以事件已准备的候选页栈作为幕生命周期比较基线，只有回调新增的页面切换/演出才交接；真正交接的 origin 同样使用该候选栈，返回 HUD。事件已派发的页面回调不会在进入事务中重复执行。

新增 title_to_hud_navigation_is_not_a_story_handoff_and_hooks_run_once：同时覆盖普通起点、frameEnter 真正打开演出页、返回 HUD、重复选择不同起点、页面回调单次执行、自动档候选及单条新历史。最终 fmt、Clippy -D warnings、完整测试、demo --check、diff --check 全通过，**189 passed / 0 failed / 2 ignored**；日志为 target/gui-recheck-tests.log、gui-recheck-clippy.log、gui-recheck-check.log。忽略项仍为 SDK/600MiB 显式压力测试。

真实验收使用隔离目录 target/multi-start-gui-fixed-20261029-192426，没有读取或覆盖玩家原有存档。旧故障复现目录单独保留。播放器从 target/gui-recheck-runtime 运行，避免运行中的 exe 阻挡构建；D3D11 与 CPU 两次测试均通过 Alt+F4 正常退出，退出码 0。

| 项目 | 实际操作和结果 |
| --- | --- |
| 初始 UI | 主菜单显示中文标题、两条路线、读档和设置；按空格不会进入默认剧情，选择前未生成玩家数据。 |
| 第二起点 | 选择“另一条故事线”，逐字显示完整台词并结束；auto-1 为 alternate.opening、单条历史、初始变量，没有虚构 opening 返回边。 |
| 路线切换 | HUD 重新开始返回主菜单，再选“从午后开始”，进入 opening，淡黑转场和文字正常，没有卡在隐藏交接。 |
| 中文/画面 | D3D11、CPU 窗口截图均为 1282×751（含边框/标题），客户区 1280×720；主菜单、台词、选项、历史和存读档中文清楚，画面保持 16:9。 |
| 普通推进 | opening→question→walk→question；选择散步后 trust=1、met=true、mood=期待、invited=false，四条历史。 |
| 第 200 槽 | 保存页切到第 20 页并滚动到 200，真实保存成功；同文件含 question、上述变量、四条历史和截图。 |
| 跨进程手动加载 | 完全退出 D3D11 进程，以 --renderer cpu 重启，同目录仍先显示主菜单；直接载入第 200 槽，显示缩略图/时间/纯台词预览，返回 HUD 并完整显示文字和选项。历史页目视确认 trust=1、已相识、期待及四条历史。 |
| 跨进程自动加载 | CPU 进程读取 D3D11 的 auto-1，恢复第二路线完整台词；读取前后五个自动档逐文件 SHA-256 全部不变，没有写回自动档。 |
| CPU 模式 | 展开工程状态，明确显示“像素后端：CPU fallback”；原生窗口本身仍使用 D3D11 swapchain，不将两者混淆。 |
| 真正的 UI 交接 | 从 question 进入 intro，文字完成后继续，frameExit 启动真实 MP4；观察到测试色条与时间戳。空格跳过只返回 opening，不穿透到 question；隐藏槽 active=false、source/continuation=opening、origin/stack=hud。 |
| 自动播放 | 在 opening 开启后自动到 question，按钮恢复“自动播放”，停在选项，没有替玩家选择。 |
| 模型动作 | CPU 窗口进入 live2d.stage，真实 Haru 正常显示；点击 Live2D 动作，观察到双手合拢变为张开等姿态变化，没有错误提示。 |

修复后的播放器另复制到新的源码外系统临时目录，用原有两种完整 demo 包分别 --package --check，均退出 0，无游戏散资源；位置在 target/gui-recheck-portable.path。内容/存档格式与 content-version=11 不变。

本轮没有重做全部 DPI/跨显示器/全屏、菜单视频暂停/自然结束/视频中退出重启矩阵或真实驱动丢失；不据部分媒体检查宣称全部媒体矩阵通过。使用 --no-audio，设备流与主观听感仍未验收。没有 Git 提交、推送或公开发布。
