# 任意 repeat 通用分页交接设计（v0.11.2 已实现）

日期：2026-10-31；归属 [v0.11.2 总方案](v0.11.2-implementation-plan.zh-cn.md)。`page-size`/`page-bind`、`repeat.*` 与 `ctx.repeats` 已在本版实现，接口与限制以 [API 第 38.3 节](xml-js-api.zh-cn.md#383-repeat-通用分页) 为准；本文保留设计依据。

## 1. 范围与语法

支持当前全部数据源：choices、slots、autosaves、history、gallery、variables、textLocales、voiceLocales。每个 repeat 独立选择是否分页；不新增任意 JS 数组数据源、筛选表达式或排序脚本。

```xml
<!-- variables 中声明：<int id="galleryPage" initial="1" min="1" max="256" /> -->
<repeat id="galleryCards" x="40" y="100" width="1000" height="450"
        source="gallery" columns="3" item-width="300" item-height="200"
        page-size="6" page-bind="ui.galleryPage">
  <label id="galleryCardTitle" width="280" height="32" bind="item.title" />
</repeat>
<label id="galleryPagerText" x="40" y="570" width="300" height="32"
       bind="repeat.galleryCards.pageText" />
<button id="galleryPrev" x="360" y="570" width="120" height="40"
        text="上一页" enabled="repeat.galleryCards.canPrev" event="gallery-prev" />
<button id="galleryNext" x="500" y="570" width="120" height="40"
        text="下一页" enabled="repeat.galleryCards.canNext" event="gallery-next" />
```

```javascript
// 后续拟议用法：页元数据 Number，写入既有 UI int 可使用安全整数。
event.register('gallery-prev', ctx => {
  const p = ctx.repeats.galleryCards;
  if (p.canPrev) ctx.setUi('galleryPage', p.page - 1);
});
event.register('gallery-next', ctx => {
  const p = ctx.repeats.galleryCards;
  if (p.canNext) ctx.setUi('galleryPage', p.page + 1);
});
```

- page-size 与 page-bind 同时出现或同时省略；page-size 是静态整数 1–200，不接受动态表达式。
- page-bind 仅引用已声明 UI int，要求 min=1，max 足以容纳数据源法定最大页数，避免列表增长后无法归一化。初值还须在启动候选的实际页数内；空数据初页为 1。
- 一个页变量只能绑定一个分页 repeat，防止不同总数相互夹取；需要联动的列表由 JS 显式更新多个变量。不允许两个 repeat 共享绑定造成隐式导航。
- 无分页属性时保持现有 repeat 行为，不对历史/画廊自动分页，也不自动生成分页器。
- 不增加 ctx.repeatPage 命令，复用 ctx.setUi 与现有 32 命令事务；上一页/下一页/跳页/页码按钮均由作者 XML/JS 或组合标签提供。

## 2. 元数据、范围和变化

新增白名单 repeat.<节点ID>.<字段>，节点引用按最终完整 ID 精确解析，不按点号任意猜测；组件前缀包含点时也须支持。JS 使用 ctx.repeats[完整ID]。仅对声明分页的 repeat 提供下表，不暴露任意对象或整个源列表。

| 字段 | 类型与语义 |
| --- | --- |
| page / pages | Number；当前页与总页，均从 1 起 |
| pageSize / total | Number；固定每页上限与源记录总数 |
| start / end | Number；本页在完整源中的一基序号，空集均为 0 |
| canPrev / canNext | bool；是否存在相邻页 |
| pageText | string；宿主中文“第 x / y 页”，经现有 i18n 格式化 |

页数公式 pages=max(1,ceil(total/pageSize))；空集 page=1、无行、不能翻页。本页 offset=(page−1)*pageSize，先检查安全算术，再取有限范围。脚本读取为事件开始的快照；setUi 后须用局部变量计算，不假定 ctx.repeats 或 ctx.ui 同步改变。

区分两类边界：作者在事件中显式写入 0、负数、非整数或大于当前候选 pages 的值，整批拒绝；源因剧情推进、读档、语言变化或 profile 等提交而缩短时，将原页夹取到最后一页，并将归一化结果写回同一候选 UI int。初始值不合法、热更改成不合法的新声明初值，均报错，不能自动修正坏配置。数据自然变化的夹取属于准备计算，不是额外 JS 命令，不触发生命周期或变量变更回调。

若同批显式写页又改变数据源，以最终候选源的总数校验该显式值；不能先按旧源通过再提交越界页。整批最终无效时，页码、UI、Session、音频、文件与缩略图缓存保持旧状态。列表大小不变时保留页；源变化不强制回第一页，保留能容纳的页号。

## 3. 各源行为与存档范围查询

| 数据源 | 分页所依据的完整源 |
| --- | --- |
| choices | 已在 Controller 提交边界校验并缓存的可用选项，保持 XML 顺序 |
| history | 当前保留历史，最多 1000 条，保持原顺序，不包含隐藏检查点 |
| gallery | 已有完整 CG 集合与当前解锁字段，保持当前排序与预算 |
| variables | 当前剧情变量展示记录，保持当前排序，不提供修改入口 |
| textLocales / voiceLocales | 国际化当前允许集合及其既有顺序，关闭时遵循已有空源规则 |
| autosaves | 最近实际存在的最多十槽，保持 generation 降序及坏槽可见语义 |
| slots | 全部手动槽 1–200，以槽号升序，页大小由该 repeat 指定 |

choices 的条件仍只在剧情提交边界求值，不因翻页执行 JS；game.complete 的显示/交互门控及隐藏选项防伪不变。分页不自动替玩家选选项。ctx.item 保持真实原始记录，不能用页内索引代替 slot、选项 ID 或 CG ID；不新增可伪造剧情历史路由。

slots 不能在已有十槽结果上再次切片。新增受限元信息范围读取能力，例如 SaveStore::list_range，供显式分页 repeat 读取实际槽号区间；完整变量/历史重放仍只在真正加载时执行。只读取相关页面，不先扫描所有 200 槽。autosaves 保持现有最多十槽机制。

未声明通用分页的 slots 继续使用 saves.page / ctx.slotsPage / 固定十槽的现行宿主能力。这不是旧格式兼容解析器，而是两个明确用途的当前接口。声明通用分页的 slots 只用自己的 UI int，不读写 saves.page；同一页可以存在不同页码/大小的两个 slots repeat。

缩略图只准备已挂载页面栈涉及的实际可见槽范围，同槽去重；隐藏的其他页面不能引发全槽扫描。页面/栈/存档变动在 App 准备阶段刷新缓存，保留现有解码与纹理预算，预算不足则明确失败，不静默扩大内存。label/image 绘制不读盘。预览保留真实槽身份，不显示 frame ID。

## 4. 宿主数据与缓存实现

当前 Data.lists 按 source 存 Vec，无法表达同源不同页。保留源数据与顺序，在准备阶段为每个分页 repeat ID 计算 PagePlan、元数据及可见行；render 按 repeat ID 读取自己的缓存，未分页 repeat 按现有源数据读取。不要直接覆盖共享 gallery/history/slots 列表。

存档源使用按范围提供者及按槽缓存，避免为计算 total 而解码所有缩略图。两个 repeat 重叠范围共享元信息/纹理；页面栈内仍被使用的纹理不能因另一个 repeat 换页就释放。隐藏控件同样验证属性和静态引用，只有真实可见范围才做磁盘和纹理准备。

页变化后仅重置对应 repeat 的滚动位置；同页普通重绘不复位，其他 repeat 的滚动状态不变。行交互 ID/缓存身份须包含 repeat、真实条目和页面修订，换页清理该列表旧焦点/按下状态。按下旧页、换页后释放不能触发新页同位置按钮；异步输入/旧 Event 必须通过当前绑定、条目与状态修订检查，不能用新页数据解释旧事件。仍保持一次 UI 更新最多一个意图。

## 5. 事务、恢复、热更与发布

顺序为：收集命令 → 原有 Controller/宿主候选验证 → 得到候选数据源 → 校验或归一化页变量 → 构建页计划/准备元信息及资源 → 验证所有新绑定 → 原子 IO（如有）→ 一次提交 UI/剧情/缓存。分页候选准备失败必须先于 settings/save/profile/隐藏槽写入；绘制不补做准备。

分页变量沿用现有 UI 生命周期：普通存读档不从剧情槽恢复页码；新进程按初值、热更/重开按原 UI 重建语义处理后再校验。活动隐藏交接已有 UI 变量快照，恢复时验证变量/数据源，按源缩短规则夹取；保留原文件严格字段、总 2MiB 预算与错误回滚，不给隐藏槽新增分页字段。页计划、滚动、纹理、焦点不入档。

--check 验证所有分页节点、页变量独占与容量、前向引用及空源行为，不触发玩家存档扫描或写盘。真实槽元信息由运行时验证；无玩家存档的 --check 不冒充全部分页效果已验证。KPAK 1/2 保留分页 XML/组件脚本，经独立重载验证；story/UI/save/容器版本不改变。
