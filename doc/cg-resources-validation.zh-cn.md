# CG 资源与共享解锁验收

日期：2026-10-29。应用 v0.9.1，story 8/UI 3/save 9/profile 1 不变，demo content-version=10。依据 cg-resources-implementation-plan.zh-cn.md。

实现专用 AssetKind::Cg 与 assets.cg；visual.cg 严格校验引用类型并拒绝 background/portrait 混用。CG 画面展开为无人物的背景显示，复用既有静态图像解码/预算/滤镜/纹理缓存。story_chain 支持合并新资源分类。专用资源按作者手工 ID 自动加入画廊，可同 ID 声明中文标题；跨标记重复映射专用 CG 拒绝，目录总量限制 256 不变。

game.cgId 提供当前专用标记；demo 在现有 game.enter 中调用 ctx.unlock，解锁由作者回调控制。cg.total/unlocked/progress 只读绑定从独立 profile 集合统计，不增加剧情槽字段；同标记不重复计数，读档/重开不撤销。沿用共享 profile 原子写、故事隔离、坏文件提示和现有事务约束。

最终 fmt、Clippy --all-targets -D warnings、demo --check、git diff --check 通过。完整 cargo test --locked -j 1：185 passed / 0 failed / 2 ignored。新增 app CG 用例覆盖真实图像加载、纯 CG 幕、类型/互斥拒绝、进入解锁、恢复旧剧情不撤销、跨进程 profile 和统计；既有两种 KPAK 真实打包/重载、坏 profile、原子失败回归通过。首轮发现当前存档示例仍为 content 9，新增 cg-save.example.json 并更新当前样例测试后完整重跑通过，原 v0.9.0 样例保留历史。

实际 Windows 默认呈现窗口使用独立 target/cg-gui-saves 与 --no-audio：opening→两次 walk→邀请→decision→accept，accept 显示完整背景 CG、无人像叠加，中文和 16:9 布局正常。画廊显示“已解锁 3 / 3”，profile.json 含 cg.ending/cg.lin/cg.room。发现初始示例标题过长导致溢出，缩为“结尾 CG”后重新检查正常；真实子 UI 热更保留当前 accept。

Alt+F4 完全退出，再以同目录重启回到 opening，画廊仍为 3/3，三个缩略图和标题正常，证明剧情初始位置不撤销跨进程共通进度。已关闭本轮验收窗口。第一次捕获出现其他前台画面，激活核实的游戏窗口后恢复；另一次输入被目标遮挡保护拒绝，重新激活后操作成功，没有向其他应用注入点击。

本轮只验收默认窗口，未另行进行 CPU 窗口/全 DPI/200 槽完整人工矩阵或音频设备听感；默认忽略 SDK/600MiB 用例未另跑。示例 CG 复用已有房间图，并非新绘制插画。未生成正式发行目录、未公开发布第三方素材、未提交或推送 Git。

日志：target/cg-final-tests.log、target/cg-final-clippy.log；隔离 GUI 记录 target/cg-gui-saves。当前存档示例 doc/cg-save.example.json。
