# v0.10.0 UI 扩展性独立复验

初次复验：2026-10-29。依据三个工作包的实现/验收方案，复验当前未提交工作区。此记录独立于实现者的 `ui-extensibility-validation.zh-cn.md`；初次只验收和记录，2026-10-30 按用户要求修复，结果见末节。未提交或推送。

## 结论

版本定稿（2026-10-30）：本次 UI 扩展功能和 R1–R3 修复统一归入 v0.10.0；以下保留原复验时间、日志与实际执行范围。更改应用版本不改变 story/UI/save、偏好 schema 或 demo content-version，也不将未验项改记通过。

**初次复验未通过；R1–R3 已于 2026-10-30 修复并复验。** 以下保留初次 246 项测试通过而三项针对性复现失败的证据。修复后的 252 项结果及 GUI 范围见末节，不将未执行的其他验收项改记通过。

| 编号 | 优先级 | 已确认问题 | 定位 |
| --- | --- | --- | --- |
| R1 | P1 | 首次点入调色器与前进键同帧时，可以推进剧情 | `src/app.rs:509`、`src/app.rs:611` |
| R2 | P1 | 已聚焦调色器隐藏后仍占用键盘，阻挡剧情快捷键 | `src/interface/render.rs:291`、`src/app.rs:513` |
| R3 | P2 | Tab 无法从外部进入调色器，只能鼠标点入后切换内部子区 | `src/interface/render.rs:704`、`src/interface/render.rs:744` |

## R1：首次获焦当帧的键盘穿透

复现使用最小真实 PlayerApp：两个场景 opening → second，文字速度为零、自动存档关闭；根页 pause=false，包含一个初值为 `#FF0000FF` 的 color-picker。标题在 game.enter 中更新为当前场景，用于从公共接口观察真实剧情提交。

1. 从明确的开始按钮进入 opening，等待输入释放屏障清除。
2. 在同一次 egui 更新中，向调色器红色色相位置派发主键按下，同时派发 Space 按下/释放。点击的颜色与已提交颜色相同，因此无需产生颜色更改业务事件。
3. 期望标题仍为 opening；实际变成 second，证明剧情已推进。

原因：`story_input` 在绘制前读取旧的 `pickers.focus`；绘制建立新焦点后，快捷键分支继续使用该旧 bool。当调色器不发出颜色事件时，没有其他 intent 阻止前进。按住快进/自动播放也共享绘制前的门控，应一起审查。

修复要求：让本帧控件编辑/命中所有权在剧情输入仲裁前生效，或在绘制后以本帧实际所有权重新仲裁；不要仅依靠颜色改变事件是否存在。增加相同颜色、预览区、首次获焦、已聚焦以及与自定义前进/快进同帧的 App 集成测试。

## R2：隐藏控件遗留焦点

1. 在含单个调色器的页面，按下/释放三角内部，确认 PickerState.focus 已设置。
2. 不改变页面栈，把该控件的 visible 条件变为 false，然后正常绘制下一帧。
3. 期望 focus/gesture 清除；实际 focus 保持原节点。

针对性测试直接将已解析节点的 visible 条件改为 false，以隔离渲染失效处理；作者通过 UI bool 隐藏时走同一提前返回分支。当前节点隐藏/裁剪会直接 return，未对 App 持有的 PickerState 清理；剧情门控却始终要求 focus=None。若根页隐藏最后一个 picker，普通背景点击也没有对应控件逻辑来释放它，快捷键会持续失效。

修复要求：每次 UI 候选提交或绘制前，根据顶层页面、可见性、启用状态、裁剪、绑定和定义代数统一校验焦点/抓取，失效即清理并落实释放屏障。覆盖控件本身隐藏、父容器隐藏、禁用、裁剪，以及不变页面栈的外部绑定改变。

## R3：键盘无法进入调色器

最小页仅包含一个启用的 color-picker，无鼠标输入；先绘制空帧，再连续派发四次 Tab 按下/释放。期望至少进入环编辑区，实际 PickerState.focus 始终为 None。

当前只有指针命中能首次设置 focus；键盘逻辑包在 `if let Some(area) = focused_area` 内，也未为 picker 注册 egui 可遍历焦点。现有 `triangle_ring_alpha_and_keyboard_emit_one_canonical_event` 测试先点环才按 Tab，因而漏掉从前后控件进入的路径。真实配色页按 Tab 也未进入色盘。

修复要求：将整个控件接入统一焦点顺序，再管理环/三角/alpha 子区；Tab/Shift+Tab 应能从前后按钮进入和离开，不能只把内部焦点置 None。覆盖两个 picker、alpha=false、隐藏/禁用、页面重开，以及按钮确认与剧情输入不重复提交。

## 本轮执行证据

| 检查 | 结果 |
| --- | --- |
| cargo fmt --check | 通过 |
| cargo clippy --locked --all-targets -- -D warnings | 通过 |
| cargo test --locked | 沙箱外重跑：246 passed / 0 failed / 2 ignored |
| cargo run --locked -- --check | 通过：31 场景、19 UI 页面、2 视频、1 Live2D |
| 两种真实 CLI 打包与独立 --check | KPAK 1/2 均通过；49 文件；使用 level=1、--no-live2d |
| git diff --check | 通过，只有 Git 行尾转换提示 |
| 独立针对性复现 | R1/R2/R3 共三项，全部复现失败，未混入上面的 246 项统计 |

首次在默认沙箱临时目录运行，有 43 项原子替换报 os error 5；将 TEMP/TMP 指向工作区 `target/review-tmp` 后消失。随后三个 loopback 测试报 os error 10013；在沙箱外完整重跑后全部通过。这两类环境错误不计为实现缺陷。

本地证据均在不提交的 target 中：`review-tests-unrestricted.log`、`reviewer-probe.log`、`reviewer-input-probe.log`；复现源保留在 `reviewer-probe.rs` 和 `reviewer-input-probe.rs`。复现执行时临时放入 tests/reviewer_probe.rs，执行完已移走，未改现有实现/测试文件。复现用例从现有 color_picker_render 测试夹具派生；R1 使用真实 PlayerApp，不是仅模拟颜色算法。

## 本轮实际窗口观察

Windows 原生窗口，独立 `target/reviewer-saves`，禁用音频与 Live2D。先用默认 D3D11 模式，随后以 --renderer cpu 重启；CPU 像素模式仍使用 D3D11 原生 UI，不能称为独立 CPU GUI 渲染后端。

- A：进入画廊，打开凛的竖图（2/3），下一张切换透明方图（3/3），下一张正确禁用；黑底及透明孔未显示上页残留，当前仍为 question。横图、非模态单图根页本轮未另做窗口复验。
- B：Escape 取消捕获后未改草稿；重新捕获 Enter，应用后标签变为“回车”，独立文件保存 advance=Enter。剧情中 Space 保持 opening，Enter 推进到 question。正常关闭进程后，用同一隔离目录以 CPU 模式重启，快捷键页仍显示“回车”与 Enter，确认跨进程加载。
- C：左侧 alpha 条拖动将 `#10253BFF` 改为 `#10253B75`，预览同步，文件未写入；取消后重新打开恢复 `#10253BFF`。两个三角色盘、中文及主路径显示正常，但这些不能抵消 R1–R3。

## 仍未完成的验收

不将实现者旧截图或旧结果计作本轮执行：100/150/200% DPI、多显示器/IME、60 秒拖动及像素色差、全键表/小键盘、改键后真实视频跳过、坏文件/写失败窗口注入、发布包离开源码目录运行 GUI、玩家槽/隐藏交接/自动槽跨进程完整矩阵均未复验。音频设备与主观听感未验收。

实现者已记录的小键盘物理区分限制仍存在；不是已完成全键表支持的证据。JS 失败后抓取清理、外部绑定/布局改变时的缓存失效，也应在修复 R2 时补足专门集成测试。

接手修复时先为 R1–R3 增加长期回归测试，再修输入所有权和焦点生命周期；通过后更新本记录及原验收记录。不要仅修改文档，将这三项从验收范围排除。

## 2026-10-30 修复与复验

R1–R3 已修复，未更改 Model、剧情/偏好/包格式或 demo 剧情路由。

- R1：App 在 UI 绘制确定本帧输入所有权之后，统一仲裁前进、按住快进及自动播放。点击原颜色、预览区或禁用控件也消耗输入，不依赖颜色更改事件；旧键释放后才恢复剧情。只读快进仍使用帧开始时的已读资格，避免本帧刚显示完的未读句立即跳走。
- R2：根据本帧顶层、启用且实际可见的 picker 清理失效焦点、抓取和缓存，并释放对应 egui 焦点。覆盖节点/父容器隐藏、节点/父容器禁用、裁剪；真实 PlayerApp 回归也验证作者回调隐藏正在编辑的 picker 后，旧键不穿透、重新按下可正常前进。
- R3：环、三角、可选 alpha 分别注册到 egui 统一焦点顺序，不再依靠鼠标建立首次焦点。方向键保留在调色器内，Tab/Shift+Tab 正常交给前后控件；两个 picker 和 alpha=false 的正反导航均有回归。

新增六项长期测试：`tests/color_picker_render.rs` 五项、`src/app/colors.rs` 一项。包含首次获焦同帧前进、失效清理、无鼠标进入、按钮与双 picker 导航，以及无颜色事件的预览点击与快进/自动播放仲裁。原失败复现迁入长期测试后通过；原临时证据仍留在 target，不提交。

| 修复后检查 | 结果 |
| --- | --- |
| cargo fmt --check | 通过 |
| cargo clippy --offline --locked --all-targets -- -D warnings | 通过；最后检查使用本地缓存，避免无关索引网络连接失败 |
| cargo test --locked | 252 passed / 0 failed / 2 ignored；沙箱外执行 loopback 测试，TEMP/TMP 指向 target/review-tmp |
| cargo run --locked -- --check | 通过：31 场景 / 19 UI 页面 / 2 视频 / 1 Live2D |
| git diff --check | 通过 |

完整回归后为 R1 的同一集成测试补充了“回调隐藏控件 → 旧键阻挡 → 松键后新按键恢复”的断言，并单独重跑通过；之后格式/Clippy 再检通过。日志：target/picker-fix-full-tests.log、picker-focus-integration.log、picker-fix-clippy.log、picker-fix-check.log。

实际 D3D11 窗口使用隔离 target/picker-fix-gui，禁用音频和 Live2D；确认从色盘 Shift+Tab 退出，再从外部 Tab 进入，Tab 切到三角，Shift+Tab 返回环；右方向键使 `#3B1010FF` 变成 `#3B1110FF`，预览同步且焦点保持；Escape 退出后编辑框消失。R1 同帧输入与 R2 作者动态隐藏使用自动 App/渲染集成测试，未声称人工同时按键/点击复现。

本轮未重做 CPU 窗口、DPI/多屏/IME/60 秒性能/音频听感、两种 CLI 包及存档全矩阵；它们沿用前文的实际记录和未验标记。修复通过针对性验收，不代表整个 v0.10.0 的全部可选环境矩阵已执行。
