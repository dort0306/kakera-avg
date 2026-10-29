# CG 资源与共享解锁策略

现有 XML/JS 已支持 gallery、ctx.unlock 和独立 profile.json，复用其原子写入、坏档提示与共享状态；不把解锁统计写入剧情槽。需要新增 Rust 的仅是独立资源类型、CG 画面约束、当前 CG 标记及统计只读绑定，解锁时机仍由作者 JS 决定。

- 资源声明 `<cg id="cg.promise" src="assets/cg/promise.png"/>`；id 为作者手工填写、全局唯一的稳定标记，不按场景序号或路径猜测。
- 视觉声明 `<visual id="promise" cg="cg.promise"/>`，frame 引用该 visual。cg 与 background/portrait 互斥，CG 必须引用专用资源，普通背景与人物仍要求普通 image。运行帧展开为完整背景图，没有人物。
- 专用资源自动加入画廊，默认标题为资源 ID、默认未解锁。可在 UI gallery 中使用同 ID/同 image 资源提供中文标题与 unlocked 初值；不同标记不得重复映射专用 CG。
- game.cgId 为当前专用 CG 标记，普通幕为空字符串。作者在 game.enter 中调用 ctx.unlock(ctx.game.cgId) 即实现成功进入时解锁。复用现有 profile 事务，回放历史不解锁；解锁持久化失败按现有规则回滚进入。不新增绘制 IO/脚本。
- cg.total/cg.unlocked 为画廊总数和已解锁数，cg.progress 为中文进度文本；按唯一标记统计，多幕引用同一 CG 不重复计数。原有显式 gallery 图片条目继续视为作者指定的收藏项。
- profile 1/read 1/save 9 不变；共享记录不随普通槽、自动槽、隐藏槽或重开撤销。demo content-version=10，旧剧情槽不迁移。应用仍为 v0.9.1。

验证严格资源/互斥/预算/绑定、重复进入去重、读档/重开不撤销、跨进程共享 profile、真实资源解码及两种 KPAK 回归。GUI 与主观听感分开报告。
