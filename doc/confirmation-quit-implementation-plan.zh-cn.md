# 自定义确认弹窗与 UI 退出

2026-10-29。先评估现有 XML/JS：模态 screen、panel/image/nine-patch、label/bind/visible、button 的皮肤/字号/圆角及 RGBA 已可组合完成确认弹窗，故弹窗仅新增 demo XML/JS，不引入原生固定窗口、任意脚本字符串执行或新的组件框架。标题为普通可省略 label；透明度通过颜色 alpha 和现有图片效果配置。按钮需要自定义文字颜色时可使用空文字 button 配合同位置 label。

现有脚本没有关闭宿主窗口权限，因此仅新增受约束 ctx.quit() → Command/Operation::Quit → App 退出请求。整批 UI 命令、绑定、页面生命周期及媒体准备成功后才提交退出请求；与保存、加载、设置、推进等操作互斥，game.enter 和 frame/screen 生命周期均禁止自行退出。确认逻辑由作者 JS 决定，宿主不强制确认，也不增加路径/进程启动或强制终止权限。

Windows 原生宿主读取请求并通过 event_loop.exit 正常退出，复用 shutdown 补写已读、设置及待完成自动档；eframe 宿主发送 Close 并由 on_exit 清理。不调用 process::exit，不运行剧情 frameExit/screenClose，不消耗 active 隐藏交接槽。UI 交接中允许退出，与系统关闭按钮语义一致。

demo 以共享确认页展示有标题“退出游戏”和无标题“返回主菜单”；按钮可取消或确认，自定义遮罩、面板、正文和按钮外观。主菜单和 HUD 均有退出入口。story 8/UI 3/save 9/content 11/KPAK 1/2 不变。

验证 JS/JSON 严格解析、同批失败不退出/不局部提交、退出与其他操作冲突、生命周期禁止、未开始/已开始/活动交接退出、取消/确认及底层控件阻挡；执行 fmt/Clippy/完整测试/--check、两种包及真实窗口的有/无标题、取消、正常退出和退出补写。GUI 与设备/听感分开记录。
