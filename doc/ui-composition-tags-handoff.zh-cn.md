# JS 组合 XML 标签交接设计（v0.11.2 已实现）

日期：2026-10-31；归属 [v0.11.2 总方案](v0.11.2-implementation-plan.zh-cn.md)。`component.register` 已在本版实现，接口、预算与限制以 [API 第 38.2 节](xml-js-api.zh-cn.md#382-js-组合标签-componentregister) 为准；本文保留设计依据。

## 1. 能力边界

作者可用 JS 注册新的 XML 标签，但标签最终必须展开为宿主提供的 UI 节点树。它是加载期的受限组合，不是自定义渲染器，也不是常驻组件实例。

- 可输出 scene、panel、image、nine-patch、label、button、slider、toggle、repeat、color-picker，以及已注册的其他组合标签；最后只能剩宿主节点。
- 不能生成 ui/screen/include、variables/gallery/palette/ui-media 等根级声明，不能注册新的宿主命令或读取路径。
- 页面、UI 变量、资源、事件和生命周期由作者按原契约显式声明。首期不自动生成组件私有变量或回调，避免新增状态与权限系统。
- 组件展开只读取属性和作者子节点，不接触 ctx、Session、玩家偏好、时钟、随机、IO、网络、宿主命令或 JS eval。显示变化仍由运行期绑定与 event.register 完成。
- 拖动、连续数值、媒体和颜色等能力只能复用已有宿主标签，不能通过新名字绕过宿主权限与预算。

## 2. 建议注册接口与示例

拟议签名：component.register(tagName, { props, expand })。只允许脚本装配阶段注册，重复注册或封存后注册拒绝。自定义名要求 x- 前缀及小写 ASCII，例如 x-card / x-tabs，最长 64 字节，不能覆盖宿主或结构标签。

props 是严格属性表；首期类型为 string、bool、int、number、binding、event。每项声明 required 或类型匹配的 default；int/number 可声明 min/max，string 可声明长度上限。属性值由宿主从 XML 解析并转换，禁止作者自行解释 XML 字符串。binding 额外声明 valueType，先接受路径 token，最终按实际用途再验证白名单和类型；event 是独立事件 ID，最后验证真实注册及绑定来源。

以下为待实现示意，完整类型名以本方案为基准落地，不是当前可运行代码：

```javascript
component.register('x-card', {
  props: {
    title: { type: 'string', required: true },
    expanded: { type: 'binding', valueType: 'bool', required: true }
  },
  expand({ props, children }) {
    return {
      tag: 'panel',
      attrs: { id: '$root', width: 600, height: 360 },
      children: [
        { tag: 'label', attrs: {
          id: 'title', width: 560, height: 40, text: props.title
        } },
        { tag: 'panel', attrs: {
          id: 'body', y: 50, width: 600, height: 310,
          visible: props.expanded
        }, children }
      ]
    };
  }
});
```

```xml
<!-- expanded 为绑定属性；cardExpanded 由入口 variables 显式声明 -->
<x-card id="helpCard" x="40" y="60" width="600" height="360"
        title="帮助" expanded="ui.cardExpanded">
  <label id="helpText" width="550" height="200" text="帮助正文" />
</x-card>
```

expand 必须同步返回恰好一个节点描述对象；描述只允许 tag/attrs/children，children 为有限数组。禁止 Promise、函数、访问器、自定义原型、循环引用、未知键和隐式对象转字符串；宿主通过有界序列化取回拥有的数据，不能把 QuickJS 对象存入 Definition。

XML 通用布局属性 id/x/y/width/height/visible/enabled 由宿主保留，不能在 props 中重定义；最终根必须为容器 panel 或 nine-patch。实例属性覆盖展开根的对应通用属性，缺省规则与现有节点一致；示例中的尺寸常量只是描述默认布局，实例 width/height 为最终值。既有数值布局不自动重排子节点，组件作者必须为允许尺寸设计子树，不能承诺 CSS 自适应。

## 3. 子节点、ID、绑定与交互

作者子节点先成为只读描述；直接子节点的 ID 也按实例局部 ID 处理。首期采用单一 children 插入点，不支持具名插槽、任意 HTML 或文本模板。子节点必须按原顺序完整插入一次；丢弃、复制、重排或修改作者子树都拒绝。实现应使用宿主标记/不透明引用确认来源，不能信任 JS 自称的 source-location 字符串。

- 展开根 `$root` 映射为实例 ID，例如 helpCard。
- 其他局部 ID 统一映射为 `实例ID.局部ID`，例如 helpCard.title / helpCard.body / helpCard.helpText；局部 ID 禁止点、空白、保留字和重复。
- 嵌套组件按层级追加前缀；映射后的 ID 仍须满足全局长度、唯一性和保留目标规则。禁止生成 `$game` / `$host` 等来源。
- UI 变量绑定与事件 ID 是全局引用，不加实例前缀、不做字符串替换。多个实例可显式共享事件或 UI 变量，也可由 props 指向不同变量。
- `ui('helpCard.someButton').bind('help-event')` 以及 ui-media/animate 的目标在最终展开后验证。绑定只能落到实际可交互宿主节点；组件根不会自动变成可点击控件。
- repeat 模板内组件的前缀按模板静态 ID 生成，真实行继续由原实例机制和 ctx.item 区分。仍禁止嵌套 repeat、模板动画目标及模板 color-picker，不因经组件展开而例外。

tabs 组件据此输出按钮、面板、选中标记和布尔绑定；pagination 组件输出翻页按钮、页码标签和可选 slider。事件及分页 UI int 由作者显式声明，回调复用 ctx.setUi。不要为组件新增普通两参数 bind 或运行期事件注册。

## 4. 加载流程、隔离与预算

1. 从 ContentSource 读取现有 scripts 清单，收集剧情、事件、生命周期及组件 manifest，保留 JS 文件与注册栈位置；所有注册允许前向引用。
2. 展开 UI include，识别宿主标签与注册组合标签。注册缺失的未知标签照常报错，不进行宽松解析或名称猜测。
3. 对实例校验属性，然后在独立、受限 QuickJS 执行中重新加载同一固定脚本快照，定位对应展开函数。注册在初始化时收集并封存，展开期间不得修改 manifest 或调用事件；不能依赖上次实例的全局变量副作用。
4. 取回描述并展开嵌套组件；保存实例 XML 位置、定义 JS 位置及展开链，生成拥有的规范化树，再执行与手写 XML 完全相同的校验。
5. 完成资源/国际化/页引用/事件目标/动画目标校验和缓存准备后，才生成最终 Definition、允许派发键并提交候选。绘制与事件派发不再次执行 expand。

须复用现有解释器内存/堆栈/时间限制；单实例限额之外，再设整个候选的累计预算，禁止通过大量实例重置计时逃逸。建议硬上限：128 个注册、组件嵌套 8 层、累计 1024 个展开实例；最终节点仍最多 1024、节点深度仍 8。属性/描述单实例最多 256KiB、累计最多 1MiB；源 UI 与合并 UI 各 1MiB、include 128 文件/16 层不放宽。开始序列化、分配和递归前检查长度，循环依赖直接诊断；具体解释器时限不得高于当前宿主总预算。

## 5. 工程、发布与诊断

组件脚本沿用 story.scripts，不引入组件路径或动态 import API。资源必须事先出现在 story 资源表；展开后像手写节点一样记录实际依赖。组件存在多个实例也共享原纹理缓存。

首期两种 KPAK 都保留合并后的组合标签 XML 和所需 JS，在独立包重载时重新执行同一展开流程。包装配不能先用旧严格解析器拒绝组合标签；工程加载、依赖发现、固定快照验证和独立包重载必须共享新流水线。源工程不改写，源 JS、所有 UI 子文件与依赖集合在原子发布前复查。无时间/随机/IO 的展开应保持确定性；原有无显式 frame.id 的 UUID 例外仍按原规则。

不把展开生成的任意路径当作发布附加文件，不输出新的可执行资源。--check 验证所有页面实例及最终树，不能只验证 initial 页。热更先完整展开候选，失败保留旧 UI、Session、音频和标题；成功的 UI 重建继续遵守原热更规则，不重播生命周期或自动档。

错误至少包含：实例 XML 原文件行列、组件名、注册 JS 位置、expand 异常栈或最终宿主校验位置、嵌套展开链。作者子节点错误仍指向原 XML，生成节点错误指向实例与定义，不能冒充合并文件坐标。
