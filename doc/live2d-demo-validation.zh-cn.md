# 默认 demo 的 Live2D 集成验收

日期：2026-10-24。用户追加要求把测试场景加入 demo，更新首次 v0.6 的“独立本地模型游戏”方案。本轮使用 games/demo，不替换 question 中的普通凛立绘。

## 内容与运行

- question 的“Live2D 测试”进入 live2d.stage，Idle:0 + F01。
- expression 选项进入 live2d.expression，Idle:0 + F02。
- angle 选项进入 live2d.angle，ParamAngleX=25 / ParamAngleY=-10，无待机动作覆盖。
- tap 选项使用 UI JS 的 ctx.live2d("center", "TapBody:0") 与 ctx.choose 一起提交到 stage；idle 重置，return 返回 question。
- 三个场景均保留完整画面/台词/选项，可存读档。新增选项不修改原变量更新语义，content-version 继续为 6。
- Haru 和 Core 来自用户 SDK，原许可证与可再分发清单已复制。Core 放固定宿主 runtime，便携包放 exe 旁；无须传本机 SDK 绝对路径。

## 验证

Clippy -D warnings 已通过；真实 SDK 集成用例已通过，确认 F01/F02/参数场景的合成像素不同，Core 参数可改变网格。新增普通集成用例验证全部测试场景间的路由、变量保持、历史重放，以及点击动作与选择的组合命令。

fmt --check、Clippy -D warnings、完整回归均通过：89 项普通测试成功、1 项 SDK 测试默认忽略；该 SDK 用例另行显式执行通过。默认 cargo run --locked -- --check 成功：30 场景、4 变量、3 JS、11 页面、2 视频、1 模型，Core 06000001。

## 实际窗口与便携包

使用最终 target/package/kakera-avg-v0.6-live2d，从 D:/codes 目录运行，不传 --game 或 --cubism-core，--check 和实际窗口均成功。测试仅写 target/live2d-demo-gui，使用 --no-audio。

- opening→question 可见“Live2D 测试”入口，question 仍为原凛立绘。
- 进入 stage 后 Haru、中文、五项测试菜单正常，眼睛/裁剪蒙版显示正确。观察到 TapBody 动作张开双臂与闭眼，未出现宿主错误。
- F02 场景表情明显变化；参数场景头部转向与角度说明一致，无待机覆盖。返回剧情后恢复凛，Live2D 专用动作按钮隐藏，转场结束画面稳定。
- 在参数场景存入手动槽 1，完全退出（exit 0）后重启；读取恢复 live2d.angle、整句文字、五项选项与真实模型参数姿态。
- 读档后历史页保留 opening/question/Live2D 选择与自循环记录；变量仍为已邀请=否、已相识=否、心情=平静、信任=0。
- 正常 16:9 窗口中文字两行布局正常，模型与右侧菜单不相互遮挡；本次未重复最大化/多 DPI 验收。
- 新包带模型、Core、SDK/模型与 Core 原许可和可再分发清单；不含测试存档与设置。旧 v0.6 便携包保留未覆盖。

主观听感不在本次验证范围，Live2D 高级功能限制与 API 手册一致。

包内 exe SHA256：FB8A0C43C476FB7253C0DDF31FE0C33B2CEB7EE549CEF41E8CCE0AE2FBEF0259。
