# XML 与 JavaScript API 使用手册

适用版本：Kakera AVG Rust v0.11.0。唯一创作与存档契约为 story 8 / UI 3 / save 9，包容器为 KPAK 2（流式）/1（小包兼容），profile/read 为 1。v0.11.0 把 JS 注册改为 `frame(id)`/`ui(id)` 选择器（第 37 节），旧 `on.*` 入口已删除；v0.10.1 新增默认关闭的游戏国际化及独立语言偏好。旧播放器不认识这些扩展时严格拒绝。旧文件不自动迁移；历史接口仅查阅 [v0.7.3 归档](archive/v0.7.3-xml-js-api.zh-cn.md)。游戏界面和交互优先用 XML/JS，只有缺少宿主能力时才扩展 Rust。核对日期：2026-10-30。

从工程编辑到生成可运行发布目录的完整命令、参数、Core 配置与排错，见 [工程模式、发布模式与 kakera-pack 使用指南](project-release-pack-guide.zh-cn.md)。本手册集中说明 XML/JS 契约，第 20 节同时给出运行与资源包接口参考。

## 目录

1. [版本与职责](#1-版本与职责)
2. [文件、路径与加载](#2-文件路径与加载)
3. [最小完整示例](#3-最小完整示例)
4. [story.xml 完整参考](#4-storyxml-完整参考)
5. [变量声明与类型](#5-变量声明与类型)
6. [剧情 JavaScript API](#6-剧情-javascript-api)
7. [ui.xml 结构与页面](#7-uixml-结构与页面)
8. [UI 节点与属性](#8-ui-节点与属性)
9. [绑定和列表数据](#9-绑定和列表数据)
10. [UI JavaScript API](#10-ui-javascript-api)
11. [进入场景生命周期](#11-进入场景生命周期)
12. [常用功能组合](#12-常用功能组合)
13. [持久化与内容兼容](#13-持久化与内容兼容)
14. [校验、预算与排错](#14-校验预算与排错)
15. [旧 XML 格式](#15-旧-xml-格式)
16. [实现位置与扩展边界](#16-实现位置与扩展边界)
17. [内置转场与图片效果](#17-内置转场与图片效果)
18. [人物立绘动作](#18-人物立绘动作)
19. [分页、自动保存、自动播放、视频与 Live2D](#19-v06-分页自动保存自动播放视频与-live2d)
20. [工程热更新、KPAK 发布与渲染后端](#20-工程热更新kpak-发布与渲染后端)
21. [多文件剧本与打包合并](#21-多文件剧本与打包合并)
22. [背景输入、HUD 与截图存档](#22-v072-背景输入hud-与截图存档)
23. [按住 Ctrl 快进与已读进度](#23-v073-按住-ctrl-快进与已读进度)
24. [窗口比例与全屏](#24-v08-固定比例窗口与全屏)
25. [KPAK 2 与创作边界](#25-v081-kpak-2-与创作边界)
26. [点九图与图集区域](#26-v082-点九图与图集区域)
27. [缺省场景 ID 与作者别名](#27-v082-缺省场景-id-与作者别名)
28. [可配置窗口标题](#28-可配置窗口标题)
29. [编辑器调试与发布清单](#29-v083-编辑器调试与发布清单)
30. [按钮 UI 路由](#30-v084-按钮-ui-路由)
31. [UI 演出、过场视频与角色音效](#31-ui-演出过场视频与角色音效)
32. [v0.9.0 幕与页面生命周期、隐藏交接槽](#32-v090-幕与页面生命周期隐藏交接槽)
33. [v0.10.0 动态图片查看组合](#33-v0100-动态图片查看组合)
34. [v0.10.0 玩家快捷键](#34-v0100-玩家快捷键)
35. [v0.10.0 HSL 方形选色器与玩家配色](#35-v0100-hsl-方形选色器与玩家配色)
36. [v0.10.1 国际化](#36-v0101-国际化)
37. [v0.11.0 新版 JS 注册 API](#37-v0110-新版-js-注册-api)

## 1. 版本与职责

当前唯一契约为 **story schema 8 + UI schema 3 + 外部 JavaScript/QuickJS + 存档 schema 9**。只保留这一条内容/存档执行路径，旧 schema、flow.xml 与旧档导入均拒绝。KPAK 1/2 与数据 schema 独立，v0.8.1 按用户要求保留 KPAK 1 容器兼容。

| 文件/接口 | 职责 |
| --- | --- |
| `story.xml` / 章节 XML | story.xml 是固定入口，声明身份/UI；内容分组可分散到显式 next 链的各文件，合并后形成完整故事 |
| `ui.xml` | 画布、页面、图片、位置、大小、颜色、控件、绑定、UI 变量和 CG 清单 |
| `frame(id).advance/choice/end` | 剧情继续与选择；只有这里可更新剧情变量 |
| `ui(id).bind(event, fn)` | 页面/控件互动、设置、存读档、自动播放与模型动作；`ui('$game')` 注册 game.enter |
| Rust 宿主 | 校验与原子提交、绘制、计时、IO、音频、媒体和 Core 适配 |

| 版本字段 | 当前正式值 | 用途 |
| --- | --- | --- |
| `<story schema="8">` | 8 | 新剧本语法；每份文件必须有 next |
| `<ui schema="3">` | 3 | 界面语法 |
| `<story content-version="9">` | demo 为 9，作者维护正整数 | 历史重放/存档契约；不兼容剧情变化时提升 |
| 剧情 JSON `schema_version` | 9 | 手动/自动槽结构；旧槽不自动接受 |
| CG JSON `schema_version` | 1 | 独立 profile，按 story_id 识别 |

不读取 flow.xml，不包含旧解析器；所有场景均由外部 JS 完整注册。

## 2. 文件、路径与加载

```text
my-game/
  story.xml
  ui.xml
  scripts/
    story.js
    ui.js
    helpers.js
  assets/
    bg/room.png
    characters/lin.png
    ui/button.png
    fonts/chinese.otf
    audio/room.wav
    licenses/OFL.txt
```

`story.xml` 是固定入口名；章节由每份剧本的 next 显式串联，最后一份必须 end="true"。UI 文件与 JS 文件名可自定，由 story 显式引用。单文件 schema 8 同样需要 `<next end="true"/>`；完整分章示例见第 21 节。

- XML 与 JS 使用 UTF-8。XML 元素、属性、ID、事件名均区分大小写。
- **所有内容路径以游戏根目录为基准**，包括位于子目录中的 XML/JS 所引用的内容。Live2D 的 model3 内部文件引用是例外：相对于 model3 所在目录，仍必须留在游戏根内。
- 路径写成 `assets/bg/room.png` 或 `scripts/story.js`。不要使用绝对路径、盘符、反斜杠、`..` 或越出游戏目录的链接；统一使用 `/`。
- `scripts` 中的路径必须为不重复的小写 `.js` 扩展名；不支持通配符、自动扫描或 `import` 自动加载。
- XML/JS 在启动时读取，图片、字体、音频、视频元信息/首帧及模型资源也在启动阶段准备并缓存；视频后续图像流式解码，模型按呈现时钟更新。工程状态监控修改并构建候选资源，校验成功后更新；发布状态读取固定资源包，不监控散文件。详见第 20 节。
- 内容统一经 `ContentSource` 读取。当前提供磁盘目录快照与 KPAK 1/2 资源包，均实现同一接口；JS 不直接读取资源或文件。

启动和检查：

```powershell
cargo run --locked -- --project games/demo --check
cargo run --locked -- --project games/demo
cargo run --locked -- --project games/demo --save-dir target/my-saves --no-audio
# 发布包内直接运行 kakera-avg.exe，默认读程序旁 game.kpak
```

`--check` 不打开窗口、不需要音频设备；默认也不创建 GPU。检查 XML、JS 注册/初始条件、入口生命周期计划、资源解码和可静态收集的字体字符。它不会执行每个按钮，也不会穷举所有 JS 动态分支；Windows 显式加 `--renderer d3d11` 可额外检查硬件设备/HLSL。命令行路径相对当前工作目录；内容路径仍相对游戏根，两者不要混淆。

## 3. 最小完整示例

以下四个文件可以组成一个可检查的游戏。先准备 `assets/bg/room.png` 与 `assets/fonts/chinese.otf`；在本仓库中可从 `games/demo/assets` 复制相应文件，并保留字体许可证。示例不依赖 flow.xml。

### 3.1 story.xml

```xml
<?xml version="1.0" encoding="UTF-8"?>
<story schema="8" id="api-example" content-version="1" entry="opening">
  <interface src="ui.xml" />
  <scripts>
    <script src="scripts/story.js" />
    <script src="scripts/ui.js" />
  </scripts>
  <assets>
    <image id="bg.room" src="assets/bg/room.png" />
    <font id="dialogue" src="assets/fonts/chinese.otf" />
  </assets>
  <variables>
    <int id="trust" label="信任" initial="0" min="0" max="3" />
  </variables>
  <characters><character id="lin" name="凛" /></characters>
  <visuals><visual id="room" background="bg.room" /></visuals>
  <frames>
    <frame id="opening" visual="room">
      <text>午后的房间里，传来了轻轻的敲门声。</text>
    </frame>
    <frame id="question" visual="room" speaker="lin">
      <text>要一起散步吗？</text>
      <options>
        <option id="walk">一起散步</option>
        <option id="finish">结束今天的相处</option>
      </options>
    </frame>
    <frame id="ending" visual="room" speaker="lin">
      <text>下次再见。</text>
    </frame>
  </frames>
  <next end="true"/>
</story>
```

### 3.2 scripts/story.js

```javascript
"use strict";

frame("opening").advance(() => "question");
frame("question").choice("walk", ctx => {
  ctx.add("trust", 1n);
  return "question"; // 合法自循环，仍产生一次进入事件和历史
}, ctx => ctx.vars.trust < 3n);
frame("question").choice("finish", () => "ending");
frame("ending").end();
```

### 3.3 ui.xml

```xml
<?xml version="1.0" encoding="UTF-8"?>
<ui schema="3" width="1280" height="720" initial="hud">
  <screen id="hud" modal="false" pause="false">
    <scene id="scene" width="1280" height="720" />
    <button id="openSettings" x="1100" y="20" width="140" height="40"
            text="设置" event="open-settings" />
    <panel id="dialogue" x="36" y="505" width="1208" height="180" color="#101C2CEE">
      <label id="speaker" x="28" y="14" width="1000" height="30"
             bind="game.speaker" color="#F0CA89" />
      <label id="line" x="28" y="50" width="1146" height="78"
             bind="game.text" font-size="26" />
      <button id="continue" x="1058" y="132" width="120" height="34"
              bind="game.continueText" visible="game.canAdvance" event="advance" />
      <label id="ended" x="1058" y="132" width="120" height="34"
             text="已结束" visible="game.ended" />
    </panel>
    <repeat id="choices" x="804" y="160" width="416" height="315"
            source="choices" item-width="396" item-height="48" gap="12"
            visible="game.complete">
      <button id="choice" width="396" height="48" bind="item.text" event="choose" />
    </repeat>
  </screen>
  <screen id="settings">
    <panel id="settingsPanel" x="240" y="115" width="800" height="490" color="#101C2CF5">
      <label id="settingsTitle" x="30" y="24" width="600" height="40" text="设置" />
      <label id="speedLabel" x="30" y="97" width="210" height="32" text="文字速度" />
      <slider id="speed" x="250" y="94" width="460" height="36"
              value="settings.text_speed" min="0" max="80" event="text-speed" />
      <toggle id="mute" x="30" y="155" width="500" height="38"
              text="总静音" value="settings.muted" event="mute" />
      <button id="settingsBack" x="650" y="420" width="120" height="40"
              text="返回" event="close" />
    </panel>
  </screen>
</ui>
```

### 3.4 scripts/ui.js

```javascript
"use strict";

ui("openSettings").bind("open-settings", ctx => ctx.open("settings"));
ui("settingsBack").bind("close", ctx => ctx.close());
ui("continue").bind("advance", ctx => ctx.advance());
ui("choice").bind("choose", ctx => ctx.choose(ctx.item.id));
ui("speed").bind("text-speed", ctx => ctx.setting("text_speed", ctx.value));
ui("mute").bind("mute", ctx => ctx.setting("muted", ctx.value));
```

`advance`、`choose`、`close` 等是示例自定的事件名，并非引擎内置按钮行为；删掉对应的 `ui(控件ID).bind(事件名, …)` 注册会导致 XML 事件引用校验失败。

## 4. story.xml 完整参考

### 4.1 根元素与分组

```xml
<story schema="8" id="my-game" content-version="1" entry="opening">
  <interface src="ui.xml" />
  <scripts><script src="scripts/story.js" /></scripts>
  <assets>...</assets>
  <variables>...</variables>
  <characters>...</characters>
  <visuals>...</visuals>
  <frames>...</frames>
  <next end="true"/>
</story>
```

这里的 `...` 仅表示省略内容，不能直接作为分组内普通文本使用。

| 属性 | 必填 | 含义 |
| --- | --- | --- |
| `schema` | 是 | 唯一格式写 `8`；所有章节一致 |
| `id` | 是 | 故事 ID；ASCII 字母、数字、`.`、`-`、`_`，不能为 `.` 或 `..`；用于隔离存档 |
| `content-version` | 是 | 1–4294967295 的整数；不兼容内容更新时显式提升 |
| `entry` | 是 | 空白分隔的一个或多个起始幕 ID/name；重复（含别名重复）或未知目标拒绝；无默认自动进入 |

schema 8 入口必须声明 `interface` 和工程根属性；后续章节只声明 schema。每个文件的同类分组至多一组；合并后必须有 `scripts`、`assets`、`characters`、`visuals`、`frames`，`variables`、`effects` 可省略。人物和视觉预设组可为空。每份文件恰好一个 next，规则见第 21 节。元素分组不靠书写顺序解析；JS 按文件链及各 scripts 内的顺序执行。

`<interface>` 只接受必填 `src`，不支持内联 UI。`<scripts>` 只接受 `<script src="..." />`，共 1–32 个，不支持内联 JS。可在前面的 JS 文件声明辅助函数，在后面的文件使用。

### 4.2 素材表 assets

```xml
<assets>
  <image id="bg.room" src="assets/bg/room.png" />
  <image id="portrait.lin" src="assets/characters/lin.png" />
  <image id="ui.button" src="assets/ui/button.png" />
  <font id="dialogue" src="assets/fonts/chinese.otf" />
  <audio id="bgm.room" src="assets/audio/room.wav" />
  <video id="movie.op" src="assets/video/op.mp4" />
  <live2d id="haru" src="assets/live2d/Haru/Haru.model3.json" />
</assets>
```

资源节点均只接受必填 `id/src`，不能有子节点或普通文本。资源 ID 在整个 assets 表内唯一，不能给图片和音频复用同一 ID。

| 节点 | 支持内容 | 重要限制 |
| --- | --- | --- |
| `image` | PNG/JPEG；支持透明 PNG | 单边不超过 8192 像素，启动时解码；UI 与场景共用该表 |
| `nine-patch` | Android Studio 源 `.9.png`（保留一像素标记边框） | 专用于 UI nine-patch 节点；加载去边框，每轴 1–8 段伸缩区，详见第 26 节 |
| `font` | 可由字体解析器读取的 TTF/OTF，使用第一个字体 face | 至少配置一份；中文字符必须覆盖，随游戏分发字体及许可 |
| `video` | Windows Media Foundation；本版验证 MP4 H.264/AAC | 1080p、60fps、5 分钟以内；详见第 19 节 |
| `live2d` | Cubism model3.json 与其 moc3/贴图/动作等引用 | 需要受信任的 Core DLL；详见第 19 节 |
| `audio` | WAV / Ogg Vorbis | 扩展名 `.wav/.ogg`；1–2 声道，8000–96000Hz；单段最多 5 分钟，单资源 PCM 128MiB，总 PCM 256MiB |

字体作为统一比例字体的回退资源使用；当前没有单节点 `font="资源ID"` 属性，也不依赖开发机系统中文字体。静态台词/UI 文本会检查字形，JS 动态生成的所有字符串无法预先穷举，应由作者确保字体覆盖。

### 4.3 人物 characters

```xml
<characters>
  <character id="lin" name="凛" />
</characters>
```

`character` 只有必填 `id/name`。`name` 是显示名；`speaker`、立绘的 `character` 使用 ID。立绘存在不代表正在说话，二者独立。

### 4.4 完整画面 visuals

```xml
<visuals>
  <visual id="room.empty" background="bg.room" />
  <visual id="room.lin" background="bg.room">
    <portrait character="lin" image="portrait.lin" slot="center" />
  </visual>
</visuals>
```

| 节点 | 属性 | 语义 |
| --- | --- | --- |
| `visual` | `id` 必填，`background/background-effect` 可选 | background 必须引用 image；省略则无背景图 |
| `portrait` | `character/image/slot` 全必填，`effect/pose/enter/exit/duration/strength/easing` 可选 | 人物和 image 均须存在；slot 只能为 `left/center/right` |

同一 visual 不可重复占用槽位，因此最多三张立绘。不支持 visual 继承、在 frame 内局部覆盖或动态坐标表达式。visual 在加载时展开成场景完整数据。

### 4.5 场景 frames

```xml
<frame id="question" visual="room.lin" speaker="lin">
  <text>要一起去看看吗？</text>
  <audio bgm="bgm.room" voice="voice.lin" se="se.knock" />
  <options>
    <option id="accept">一起去</option>
    <option id="refuse">暂时留下</option>
  </options>
</frame>
```

| 元素/属性 | 必填/默认 | 语义 |
| --- | --- | --- |
| `frame.id` | 可省略 | 显式 ID 保持稳定；缺省时必须有 name，打包在候选中生成 UUID v4，不改源 XML |
| `frame.name` | id 缺省时必填，否则可选 | 非空、无空白、全故事唯一的作者别名；不能与其他场景的 ID/别名冲突，JS 和 entry 可引用它，见第 27 节 |
| `frame.visual` | 可省略 | 引用 visual；省略表示空画面，不沿用上一场景 |
| `frame.speaker` | 可省略 | 引用 character；省略表示旁白 |
| `text` | 恰好一个 | 非空普通文本，无属性；没有 HTML/富文本/变量插值 |
| `audio` | 可选，最多一个 | 只有 `bgm/voice/se` 三个可选属性，各引用 audio 资源 |
| `video` | 禁止 | v0.9.0 已移除；只允许 UI sequence.video，见第 32 节 |
| `options` | 可省略，最多一组 | 若存在至少一个 option；无属性 |
| `option` | `id` 必填 | 内部为非空普通文本；ID 在所属 frame 内唯一 |

frame 还可带一项 `<transition>`，用于进入该场景时的画面过渡；完整语法见第 17 节。

台词和选项保留空格与换行，XML 换行会规范化；在 `<text>` 内换行缩进也会成为正文。需要 `<`、`&` 时写 `&lt;`、`&amp;`，或使用合法 CDATA；不接受 `<b>`、`<br>` 等子元素。UI 控件文字通常写在 `text` **属性**中，勿与这里的 `<text>` 混淆。

每次进入场景都使用其完整画面与音频声明：

- 同一 BGM 连续出现时继续播放；不同 BGM 替换；省略 bgm 表示停止。
- voice/se 每次进入都停止旧音轨并按本场景重新播放，即使资源 ID 相同；省略表示静音。
- 自循环同样是进入事件。补全台词、绘制、逐字 tick 不重复播放。
- 成功读档和重新开始强制重建音频，不恢复播放指针。
- 暂停菜单暂停语音/音效，BGM 继续；失焦暂停所有通道。当前没有 JS `playAudio` 或音频淡变 API。

资源、人物、visual、frame 和 option 的普通 ID 应非空且无空白。推荐用 `chapter01.opening` 一类稳定命名。它们的命名空间彼此独立；修改场景 ID 或路由语义时需要考虑已有存档。

## 5. 变量声明与类型

剧情变量在 story 的 `variables` 中；UI 局部变量在 ui 的 `variables` 中。两个表分别校验，名字可以相同，但互不相通。

```xml
<variables>
  <int id="trust" label="信任" initial="0" min="0" max="3" />
  <bool id="met" label="已相识" initial="false" />
  <string id="mood" label="心情" initial="平静" />
  <string id="notice" initial="" />
</variables>
```

| 属性 | int | bool | string |
| --- | --- | --- | --- |
| `id` | 必填 | 必填 | 必填 |
| `initial` | 必填，十进制 i64 | 必填，精确 `true/false` | 必填，可为空字符串 |
| `label` | 可选，默认 id | 可选，默认 id | 可选，默认 id |
| `min/max` | 可选，闭区间 | 不接受 | 不接受 |

- ID 为 1–64 个 ASCII 字母/数字/下划线，首字符不得为数字；不能包含点或连字符。
- label 不能全为空白，最长 256 UTF-8 字节。剧情变量 label 用于状态列表。
- 每张变量表最多 128 项。string 初值和更新值最多 4096 UTF-8 字节。
- int 范围为 −9223372036854775808 至 9223372036854775807；min 不得大于 max，初值必须在范围内。
- JS 中 int 读取为 **BigInt**；bool 为 boolean，string 为 string。不要把 `2n` 与 `2` 混合做加法，推荐整数常量统一加 `n`。
- `ctx.set/add/setUi` 可接受安全整数 Number，但不会接受小数、NaN、Infinity 或不安全整数；bool/string 不做隐式类型转换。

剧情变量保存在槽位中。UI 变量、页面栈是瞬时状态，进程重启恢复初值；读档和 `restart()` 不会自动重置 UI 变量或关闭页面，作者需要时显式 `setUi/close/open`。

## 6. 剧情 JavaScript API

### 6.1 注册入口

```javascript
frame(frameId).advance(targetOrRun);
frame(frameId).choice(optionId, targetOrRun, when); // when 可省略
frame(frameId).end();
```

| 注册方法 | 触发时机 | 返回要求 |
| --- | --- | --- |
| `frame(frameId).advance(targetOrRun)` | 台词完成后收到继续 | 同步返回或提供已有目标 frame ID 字符串 |
| `frame(frameId).choice(optionId, targetOrRun, when?)` | 选择一个合法且条件允许的选项 | 目标回调同步返回目标 frame ID；when 同步返回 boolean |
| `frame(frameId).end()` | 声明结束场景 | 无回调；继续不改变剧情 |

`targetOrRun` 可以是目标 frame ID 字符串，也可以是返回目标字符串的函数。

每个 XML frame 必须完整注册，且只能属于 advance、choice、end 中一种。没有 options 的场景只能 advance/end；有 options 的场景必须 choice，并与每个 XML option 一一对应，不论其条件是否可能为 false。

同一场景可为不同 option 多次调用 `frame(...).choice`，但不可重复注册同一个 option；不能把 advance 与 choice 混用。回调执行期间不能再注册。目标场景可在执行时计算，运行时验证其存在；不能宣称启动检查已穷举其可达性。

### 6.2 剧情回调 ctx

| 字段/方法 | 类型/签名 | 说明 |
| --- | --- | --- |
| `ctx.frameId` | string | 当前源场景的 name（若有），否则实际 id；JS 可稳定按作者名字判断 |
| `ctx.optionId` | string 或 null | choice 时为选项 ID，advance 时为 null |
| `ctx.vars` | 只读变量视图 | int 为 BigInt；读取未知变量报错 |
| `ctx.set(name, value)` | name: string；value: BigInt/安全整数/boolean/string | 收集赋值；变量必须已声明，类型和范围必须匹配 |
| `ctx.add(name, amount)` | name: string；amount: BigInt 或安全整数 | 仅 int，允许负增量；检查 i64 溢出及声明范围 |

主回调可以通过方法收集更新；不能直接赋值 `ctx.vars.trust = ...`。when 只能读取，相同 ctx 结构中的 set/add 调用会报错。剧情 ctx 没有 `open/save/setting/setUi` 等 UI 命令。

```javascript
frame("question").choice("accept", ctx => {
  ctx.add("trust", 1n);
  ctx.set("met", true);
  ctx.set("mood", "期待");
  // 重新读取 ctx.vars，可见前面收集的草稿更新
  return ctx.vars.trust >= 2n ? "closer" : "walk";
}, ctx => ctx.vars.trust < 3n);
```

`ctx.vars` 每次访问生成当前草稿快照。若提前保存 `const before = ctx.vars`，它不会随后续 `ctx.set/add` 自动更新；需要重新访问 `ctx.vars`。

### 6.3 条件与提交

when 省略时视为 true；为 false 的选项隐藏，伪造选择也被拒绝。when 不接受 `0/1`、字符串或 Promise 代替 boolean。

源场景条件基于更新前变量求值；主回调中的更新按调用顺序校验和应用。进入目标场景时会用候选变量求值其可用选项。任何变量错误、溢出、非法目标、异常或目标条件错误都让这次剧情提交失败，原 Session、历史和音频保留。

返回字符串只决定下一场景，不会自动跨过该场景继续运行；每个 advance 仍对应用户一次推进。选项场景的空格不会选默认分支；全部选项隐藏时没有隐含 fallback，作者应保证故事仍能继续，或提供重开/读档入口。

## 7. ui.xml 结构与页面

```xml
<ui schema="3" width="1280" height="720" initial="hud">
  <variables><string id="status" initial="" /></variables>
  <gallery><cg id="cg.room" image="bg.room" title="午后房间" unlocked="true" /></gallery>
  <screen id="hud" modal="false" pause="false">
    <!-- UI 节点 -->
  </screen>
  <screen id="settings" modal="true" pause="true">
    <!-- UI 节点 -->
  </screen>
</ui>
```

### 7.1 ui 根元素

| 属性 | 必填/默认 | 限制 |
| --- | --- | --- |
| `schema` | 必填 | 精确为 `3` |
| `width` | 默认 1280 | 有限数值，至少 640 |
| `height` | 默认 720 | 有限数值，至少 360 |
| `initial` | 必填 | 已存在且 modal=false 的 screen ID |
| `title` | 可选，默认 Kakera AVG | 静态窗口标题；与 title-bind 互斥 |
| `title-bind` | 可选 | 绑定已声明的 UI string，例如 ui.windowTitle；见第 28 节 |

宽高必须构成 16:9；任一数值不能超过 8192。画布在窗口可用区域内居中、等比缩放并留黑边，坐标不是物理屏幕像素。一般保留 1280×720。

每个源文件根元素只接受至多一组 variables、一组 gallery、一组 ui-media，以及 screen 与 include。展开后全局须有 1–64 个 screen；变量/画廊/演出分组可跨文件合并，不允许其他任意分组。ui-media 的音效与演出声明见第 31 节。引入规则见第 7.4 节。

### 7.2 screen 与页面栈

| 属性 | 必填/默认 | 语义 |
| --- | --- | --- |
| `id` | 必填 | 页面表内唯一；给 `ctx.open()` 使用 |
| `modal` | 默认 true | true 入栈；false 替换整个根页面栈 |
| `pause` | 默认 true | 此页面在栈内时暂停逐字呈现 |
| `background-click` | 可选 | 非控件区左键的 UI 事件名，绑定到该页面 ID，规则见第 22 节 |
| `background-right-click` | 可选 | 非控件区右键的 UI 事件名，绑定到该页面 ID，不继承下层 |
| `sequence` | 可选 | 页面成为顶层时自动启动已声明的演出 ID；同页重绘不重复触发，见第 31 节 |

页面本身没有 x/y/width/height；位置由其子节点定义。入栈时保留下层页面绘制，但仅顶层可操作；栈深度包含根页最多八层。模态页不得重复入栈；`close()` 不可关闭唯一根页。

任一 pause=true 页、宿主提示或失焦都会暂停逐字计时，恢复时不补算。pause 不是脚本权限开关：作者仍可在菜单按钮回调主动调用剧情命令。空格仅在没有模态栈、当前根页未暂停、无宿主提示时触发继续，不依赖根页是否包含 scene 节点。

`screen id`、控件节点 ID、UI 变量 ID、CG ID 分属不同表；控件节点 ID 必须跨所有 screen 全局唯一。建议所有 ID 使用非空、无空白的稳定名称。

### 7.3 gallery 清单

```xml
<gallery>
  <cg id="cg.room" image="bg.room" title="午后房间" unlocked="true" />
  <cg id="cg.lin" image="portrait.lin" title="一同出发" />
</gallery>
```

`cg` 必填 `id/image/title`，`unlocked` 默认 false，只接受 true/false。image 必须是 story 资源表的 image ID。CG ID 在清单内唯一，最多 256 项。

gallery 只声明收藏数据，不自动生成页面。缩略图、锁定提示、按钮和大图由 repeat/image 等节点组合。默认解锁与 profile 中的解锁取并集；当前没有 lock/resetGallery API。

### 7.4 外部 UI XML 引入与打包合并

2026-10-28 追加，应用版本维持 v0.8.3、UI schema 3。UI 入口仍由 story.xml 的 interface.src 指定，通常为 ui.xml。入口可在根级使用多个 include：

```xml
<!-- ui.xml：全局画布、首页和标题只由入口配置 -->
<ui schema="3" width="1280" height="720" initial="hud" title-bind="ui.windowTitle">
  <include src="ui/state.xml" />
  <include src="ui/hud.xml" />
  <include src="ui/settings.xml" />
</ui>
```

```xml
<!-- ui/state.xml -->
<ui schema="3">
  <variables><string id="windowTitle" initial="我的游戏" /></variables>
</ui>
```

```xml
<!-- ui/hud.xml -->
<ui schema="3">
  <screen id="hud" modal="false" pause="false">
    <scene id="scene" width="1280" height="720" />
  </screen>
</ui>
```

```xml
<!-- ui/settings.xml：子文件仍可继续引入其他完整 UI 文件 -->
<ui schema="3">
  <screen id="settings">
    <button id="closeSettings" width="120" height="40" text="返回" event="close" />
  </screen>
</ui>
```

上述 close 事件仍需在已加载的 JS 中注册，例如 `ui("closeSettings").bind("close", ctx => ctx.close())`。事件、绑定、UI 变量、资源和页面引用沿用全局契约，不为每份 XML 新建局部作用域。

| 规则 | 行为 |
| --- | --- |
| include 属性 | 只接受非空 src，不接受其他属性、子节点或普通文本 |
| 路径 | 相对游戏根目录，**不相对当前 UI 文件**；最大 1024 UTF-8 字节，必须为 .xml；沿用相对路径/真实边界和发布链接检查 |
| 子文件根 | 必须 `<ui schema="3">`，只能有 schema 属性；width/height/initial/title/title-bind 只允许入口声明 |
| 拆分单位 | 根级 variables、gallery、完整 screen，以及根级 include；不支持 screen/panel 内 include、控件模板或任意文本拼接 |
| 顺序与引用 | 按 include 声明顺序深度优先展开，再合并数据分组并全局验证；允许引用后面文件中的变量或页面 |
| 冲突 | 同文件重复 variables/gallery、跨文件同表重复 ID、循环/重复/大小写重复引入均报错；没有后者覆盖前者 |
| 预算 | 最多 128 文件/16 层，入口为第 1 层；源 XML 累计与合并 XML 各最多 1MiB；原有全局页面/控件/变量/CG/节点深度预算不放宽 |
| 诊断 | XML/语义错误、控件及标题运行期校验保留原文件行列；合并后的内部坐标不冒充作者坐标 |
| 工程模式 | 通过 ContentSource 读取全部引入；子文件修改照常建立完整热更候选，坏编辑保留旧内容 |
| KPAK 1/2 | 自动合并为不含 include 的完整 UI 入口，并仅移除实际展开的 UI 子文件；不改源工程，不额外修改 JS |

标准入口的包内结果为单个 ui.xml；若 interface.src 为 ui/root.xml，则包内完整 UI 仍保存在 ui/root.xml，避免覆盖无关的同名根文件。其他显式附加 XML 按原发布清单保留。无 include 的单文件 UI 原字节保持；这种装配不改变 UI 3、story/save 或 KPAK 版本，也不是旧格式兼容层。

宿主工具使用 `interface::source::compile(source, entry)` 获取完整 XML、源文件列表和编译期映射，再用 `CompiledUi::parse(story, handlers)` 获得带原始位置的 Definition。低层 `interface::parse(text, file, ...)` 只解析已完整装配的文本，不自行访问文件。

可运行拆分示例见 games/demo/ui.xml 与 games/demo/ui/。策略和真实检查范围见 [UI 拆分策略](ui-split-implementation-plan.zh-cn.md) 与 [验收记录](ui-split-validation.zh-cn.md)。

## 8. UI 节点与属性

### 8.1 通用布局属性

所有八种 UI 节点都接受以下属性；不代表每个属性对每种节点都有绘制效果。

| 属性 | 必填/默认 | 说明 |
| --- | --- | --- |
| `id` | 必填 | 全部页面的节点中唯一；repeat 模板只写一份，重复实例由引擎区分 |
| `x/y` | 默认 0 | 相对父容器左上角，向右/向下为正，可为负数 |
| `width/height` | 必填 | 大于 0 |
| `color` | 一般默认 `#FFFFFFFF` | `#RRGGBB` 或 `#RRGGBBAA`；无 image 的 button 默认 `#2B405A` |
| `visible` | 默认 `true` | bool 字面量或布尔绑定；false 时不绘制也不交互，包括其子节点 |
| `enabled` | 默认 `true` | bool 字面量或布尔绑定；容器禁用会传给子节点 |

坐标、宽高和其他数值都必须有限，绝对值不超过 8192。只接受数值，不接受 `px/%/auto` 或算式。所有子绘制按祖先区域裁剪；没有 CSS、margin/padding、锚点或布局约束系统。XML 顺序决定绘制先后。

### 8.2 每种节点允许的附加属性

表中“附加属性”与上面的通用属性组成完整白名单；写在错误节点上的属性会报错。

| 节点 | 附加属性 | 可有子节点 |
| --- | --- | --- |
| `scene` | 无 | 否 |
| `panel` | `image/image-bind/fit/radius/effect/region/region-bind` | 是，任意 UI 节点，仍受 repeat 不可嵌套约束 |
| `image` | `image/image-bind/fit/effect/region/region-bind` | 否 |
| `nine-patch` | `image/density` | 是，子节点使用标记内容区，见第 26 节 |
| `label` | `text/bind/font-size` | 否 |
| `button` | `text/bind/font-size/event/route/sound/image/hover-image/disabled-image/radius/region/region-bind` | 否 |
| `slider` | `value/min/max/event` | 否 |
| `toggle` | `text/value/event/font-size` | 否 |
| `repeat` | `source/columns/item-width/item-height/gap` | 是，作为每一项模板 |

节点不接受内部普通文本，例如应写 `<label ... text="设置" />`，不能写 `<label ...>设置</label>`。

### 8.3 scene

```xml
<scene id="scene" width="1280" height="720" />
```

在矩形内绘制当前 frame 的背景和立绘，不绘制台词、人物名、选项或菜单。背景使用等比 cover；立绘保留 left/center/right 槽位、底部对齐。通常使用覆盖画布的 16:9 矩形；不是任意立绘布局编辑器。scene 的 color/enabled 不改变场景素材本身，visible 可隐藏整个场景层。

### 8.4 panel 与 image

```xml
<panel id="box" x="40" y="40" width="600" height="300" color="#102030EE" radius="12">
  <image id="logo" x="20" y="20" width="200" height="100" image="ui.logo" fit="contain" />
</panel>
```

- `image` 是固定图片资源 ID，`image-bind` 是动态图片 ID 的绑定，两者互斥。
- image 节点必须提供二者之一；panel 可以不提供，用 color 填充。
- `fit` 默认 stretch：直接拉伸；contain：等比居中完整显示；cover：等比居中填满并裁剪。
- 有图片时 color 作为图片 tint，默认白色保持原图；无图片的 panel 用 color 填充。
- `radius` 默认 0，范围 0–128，仅用于无图填色；不会给图片或子节点增加圆角遮罩。
- `image-bind` 只允许已声明的 `ui.<string变量>` 或 gallery/slots/autosaves 模板的 `item.image`。不接受路径字符串、JS 表达式或任意 game 字段。
- 被 image-bind 引用的 UI 字符串初值，以及每次命令提交后的值，都必须是存在的 image ID；即使该页面当前不可见，也会校验。

### 8.5 label

```xml
<label id="notice" width="500" height="40" text="保存成功" color="#F0CA89" font-size="22" />
<label id="dialogueText" width="1100" height="90" bind="game.text" font-size="26" />
```

`text` 默认空字符串；与 `bind` 互斥。bind 可读取受支持的 string/number/bool 字段；数字转文字，直接绑定 bool 显示“是/否”。`font-size` 默认 20，范围 8–96。文字左上对齐、按节点宽度换行；高度不足时显示 `UI 文本溢出：节点ID`，应扩大节点或拆句。

仅 `label bind="game.text"` 使用逐字呈现：按完整台词布局，但把未显示后缀设为透明，防止逐字引起换行移动。其他绑定文字整段显示；不要通过 button bind=game.text 代替对话 label。

### 8.6 button

```xml
<button id="saveButton" x="896" y="20" width="88" height="38" text="保存"
        event="open-save" image="ui.button" hover-image="ui.hover" disabled-image="ui.disabled" />
```

`event` 与 `route` 互斥：event 须注册对应的 `ui(控件ID).bind(...)`，route 直接执行页面导航（第 30 节）。也可仅声明 `sound="音频资源或音效组ID"` 创建纯音效按钮；有 sound 时可不写 event/route，或与二者之一组合（第 31 节）；`text/bind` 互斥，文字默认空，字号默认 20。图片均为共享 image ID，按按钮矩形 stretch；button 不接受 fit 或 image-bind。

悬停优先使用 hover-image，禁用优先使用 disabled-image；缺省回退到普通 image，无图则填色。禁用背景额外变暗，且不发事件。color 控制背景/图片 tint，当前按钮文字固定白色居中；不能用 color 单独改按钮文字颜色。radius 只作用于无图背景。若需复杂按钮字效，推荐把文字画入美术图片。

按钮回调 `ctx.value` 为 null；位于 repeat 中时 `ctx.item` 为对应行数据，否则为 null。节点 id 和事件名不会作为额外字段注入 ctx。

### 8.7 slider 与 toggle

```xml
<slider id="volume" width="460" height="36" min="0" max="1"
        value="settings.bgm_volume" event="volume-changed" />
<toggle id="muted" width="500" height="38" text="总静音"
        value="settings.muted" event="mute-changed" font-size="20" />
```

slider 必填 value/min/max/event，min 必须小于 max；value 必须绑定 number。toggle 必填 value/event，value 必须绑定 bool，text 为开关标签。它们在值改变时发事件，`ctx.value` 分别是 Number、boolean；**不会自动写回绑定来源**，需要回调调用 setting 或 setUi。

滑杆和开关当前采用 egui 基础控件，无图片皮肤属性；toggle 的 color/font-size 控制标签，slider 不接受 font-size。XML 接受 number 类型的 UI int 作为滑杆来源，但滑杆输出可能有小数，更新 UI int 时应使用 `Math.round(ctx.value)`；不适合编辑大范围精确 i64。

### 8.8 repeat

```xml
<repeat id="saveSlots" x="30" y="94" width="1000" height="410"
        source="slots" columns="2" item-width="480" item-height="120" gap="18">
  <panel id="slotCard" width="480" height="120" color="#203449">
    <label id="slotTitle" x="16" y="10" width="300" height="26" bind="item.title" />
    <button id="slotAction" x="350" y="8" width="110" height="32" text="存入" event="save-slot" />
  </panel>
</repeat>
```

| 属性 | 必填/默认 | 说明 |
| --- | --- | --- |
| `source` | 必填 | choices / slots / history / gallery / variables |
| `columns` | 默认 1 | 1–8 的整数，按行从左到右排列 |
| `item-width/item-height` | 必填 | 正数，单元格设计尺寸 |
| `gap` | 默认 8 | 非负数，行列间距 |

repeat 自身矩形是垂直滚动视口；模板内坐标相对每项左上角。不会自动缩小单元格适应视口，不提供横向滚动；应确保列宽总和适配。repeat 任何后代都不能再出现 repeat。color 本身不为列表绘制底色，需要增加 panel。

## 9. 绑定和列表数据

### 9.1 表达式边界

绑定是白名单路径查找，不执行 JS。有效示例：

```xml
<label id="status" width="500" height="32" bind="ui.status" />
<button id="next" width="120" height="36" bind="game.continueText" visible="game.canAdvance" event="advance" />
<panel id="locked" width="300" height="200" visible="not:item.unlocked" />
```

最后一行仅适用于 gallery repeat 模板。`visible/enabled` 接受 true、false、布尔字段或一次 `not:`；不支持多重 not、`&&/||`、比较、算术、字符串拼接、`${...}`、函数调用，路径最长 128 字节。不支持 `vars.trust` 直接 XML 绑定；显示剧情变量可用 variables 数据集，组合条件可由 JS 计算为 UI bool。

### 9.2 全局绑定

| 路径 | 类型 | 含义 |
| --- | --- | --- |
| `game.frameId` | string | 当前 frame.name（若有），否则实际 ID；剧情槽仍保存实际 ID |
| `game.speaker` | string | 人物显示名，旁白为空字符串 |
| `game.text` | string | 完整台词；label 使用它时另加逐字呈现 |
| `game.complete` | bool | 台词是否完整且没有正在进行的转场 |
| `game.transitioning` | bool | scene 画面是否正在转场 |
| `game.canAdvance` | bool | 未补全，或当前路由允许继续；不是“故事尚未结束”的别名 |
| `game.ended` | bool | 台词已完整且当前为 end |
| `game.noChoices` | bool | 台词已完整、当前为 choice 且没有可用选项 |
| `game.continueText` | string | 转场时“跳过转场”，未完整时“补全”，否则“继续” |
| `settings.text_speed` | number | 0–80，每秒字素簇；0 为整句 |
| `settings.muted` | bool | 总静音 |
| `settings.bgm_volume` | number | 0–1 |
| `settings.voice_volume` | number | 0–1 |
| `settings.se_volume` | number | 0–1 |
| `settings.video_volume` | number | 0–1，视频独立音量 |
| `settings.autosave` | bool | 自动保存开关 |
| `settings.fullscreen` | bool | 当前独立全屏设置；控制原生窗口模式 |
| `settings.skip_all` | bool | 按住 Ctrl 时是否连未读内容一起快进；false 为只快进已读 |
| `settings.auto_delay` | number | 完成文字/转场/语音后等待的秒数，0.2–15 |
| `game.autoPlaying` | bool | 当前是否开启自动播放（瞬时状态） |
| `game.autoText` | string | “自动播放”或“停止自动” |
| `game.live2dEnabled` | bool | 当前存在已启用的真实 Live2D 实例 |
| `saves.page` / `saves.pages` | number | 当前手动槽页 / 总页数 20 |
| `saves.pageText` | string | “第 x / 20 页 · 200 手动槽” |
| `saves.canPrev` / `saves.canNext` | bool | 是否可翻到前/后一页 |
| `ui.<已声明ID>` | number/bool/string | 局部 UI 变量值 |
| `audio.enabled` | bool | UI 音效总开关 |
| `audio.actor.<角色ID>` | bool | 此角色的 UI 音效是否启用 |
| `audio.excluded.<音频资源ID>` | bool | 此音频是否被玩家排除；true 表示禁止播放 |

game 进度字段逐帧更新，但选项条件只在提交边界求值缓存。设置默认值见第 10.5 节。UI int 直接显示为十进制；JS 对应的 ctx.ui int 是 BigInt。

### 9.3 repeat 数据集

只有模板及其后代可以使用所属数据集的 item 字段；别的数据集字段即使同名也不能任意引用。UI JS 的 ctx.item 获得同一条记录。

| source | 字段与类型 | 顺序 |
| --- | --- | --- |
| `choices` | `id: string`、`text: string` | XML option 顺序，已按剧情条件过滤 |
| `slots` | `slot: number`、`title: string`、`preview: string`、`exists: bool`、`loadable: bool`，以及下述截图字段 | 当前页十槽，槽号 1–200 |
| `autosaves` | 与 slots 相同的 item 字段 | 最近十个自动槽，按 generation 从新到旧，可能不足十项 |
| `history` | `id: string`、`speaker: string`、`text: string`、`choice: string` | 保留历史从旧到新，包含当前场景 |
| `gallery` | `id: string`、`title: string`、`image: string`、`unlocked: bool` | CG ID 字典序 |
| `variables` | `id: string`、`label: string`、`value: string` | 剧情变量 ID 字典序 |

字段细节：

- choices 数据本身包含“条件允许”的选项，即使仍在逐字显示。**作者应给列表加 `visible="game.complete"`，或给按钮加 `enabled="game.complete"`。** 宿主仍会忽略未补全时的 choose，不能借自定义界面绕过门控。
- slots.title 如“槽位 1”；preview 为空槽说明、台词预览或损坏原因，**不包含内部 frame ID**（槽位 JSON 仍保存 frame_id 供校验与加载）。坏槽 exists=true、loadable=false；应以 loadable 决定读档按钮可用性，以 exists 决定是否覆盖确认。另有 savedAt（本地时间字符串）、hasScreenshot（bool）、image（受信任预览纹理 ID）、excerpt（台词整理成单行，最多 64 字符及省略号；空/坏槽为状态提示）。详见第 22.3 节。
- history.id 是类似 `6 · question` 的展示字符串，不是可跳转的历史句柄。speaker 旁白为“旁白”；text 是整句；choice 为 `选择：选项文字` 或空字符串。过去选择不因当前条件隐藏而消失。
- variables.value 已转为展示字符串，包括“是/否”和十进制数；不能当成原始变量类型。UI JS 要计算剧情值应读取 ctx.vars。
- gallery.image 是资源 ID，不是磁盘路径。未解锁条目仍提供该 ID，隐藏缩略图和禁用查看由作者的 visible/enabled 与回调实现；这不是素材加密机制。
- 当前 JS ctx 没有整个列表的查询 API；按钮通过 repeat.item 获得其行。缓存会在剧情/UI 提交时重建，槽位在页面栈变化、换页或存档后刷新；没有逐帧文件扫描或显式 refreshSlots 命令。

## 10. UI JavaScript API

### 10.1 注册与输入

```javascript
ui("控件ID").bind(eventName, ctx => {
  // 收集命令；不要返回目标场景或对象
});
```

首个参数是声明该事件的控件或页面 ID，eventName 仍是 XML 中的作者事件名。同一（目标，事件名）只能注册一次；多个按钮可共用同一事件名，但必须逐目标绑定，共享逻辑写成同一个函数。`game.enter` 只能绑定固定虚拟目标 `$game`，不能用于节点 event。没有 click/tick、DOM 事件或任意事件派发 API；具名幕/页面生命周期使用第 32 节的 `frame(id).enter/exit`、`ui(id).bind('screen.open'/'screen.close')`。

v0.5 的 `ctx.transition/animate` 参数及生命周期权限见 [第 17 节](#17-内置转场与图片效果)，v0.5.1 的 `ctx.portrait` 见第 18 节；其余 UI 命令在本章逐项说明。

回调必须同步返回 undefined。宿主方法本身返回 undefined，所以 `ctx => ctx.close()` 合法；`ctx => "settings"` 不合法。所有 UI 回调沿用 QuickJS 隔离与预算。

| UI ctx 字段 | 类型 | 说明 |
| --- | --- | --- |
| `ctx.game` | 只读对象 | 第 9.2 节的 game 字段；生命周期中为候选场景 |
| `ctx.settings` | 只读对象 | 当前设置快照，数值为 Number |
| `ctx.saves` | 只读对象 | 当前分页信息；字段同 saves.*，数值为 Number |
| `ctx.vars` | 只读视图 | 当前剧情变量，int 为 BigInt；无 UI set/add |
| `ctx.ui` | 只读视图 | 当前 UI 变量，int 为 BigInt |
| `ctx.item` | 只读对象或 null | repeat 当前行；普通控件/生命周期为 null |
| `ctx.value` | Number / boolean / null | slider 新值 / toggle 新值 / button 与生命周期 null |

UI ctx 没有剧情回调的 `frameId/optionId/set/add`；场景请读 `ctx.game.frameId`，选项按钮读 `ctx.item.id`。没有 `ctx.event`、控件 ID、Session 对象、文件路径或纹理句柄。

**所有 UI 读取始终是事件开始时的快照。** setUi/setting/open 等只是排队命令，不会改变同一回调里 ctx.ui/settings/game 的读取结果。需要更新后再计算时用 JS 局部变量：

```javascript
ui("incrementButton").bind("increment", ctx => {
  const next = ctx.ui.counter + 1n; // counter 必须事先声明
  ctx.setUi("counter", next);
  ctx.setUi("status", `当前计数：${next}`);
});
```

### 10.2 页面与局部状态

| 方法 | 参数 | 行为与失败条件 |
| --- | --- | --- |
| `ctx.open(screenId)` | string | 页面必须存在；modal=true 入栈，禁止重复或超过八层；modal=false 替换根栈 |
| `ctx.close()` | 无 | 弹出顶层页；唯一根页不可关闭 |
| `ctx.setUi(name, value)` | string；BigInt/安全整数/boolean/string | 对已声明 UI 变量赋值；类型、范围、长度必须合法；动态图片变量还须引用有效 image |
| `ctx.message(text)` | string，最多 4096 UTF-8 字节 | 显示宿主通用提示窗，关闭前暂停并阻止普通 UI 交互 |

同一事件可多次 open/close/setUi，按调用顺序在副本验证；例如打开确认页后再 close 是合法的顺序导航。多次 message 最后一条生效。正常风格化提示推荐声明 UI string + XML label/panel；message 保留原生提示外观。

### 10.3 剧情操作

| 方法 | 参数 | 行为 |
| --- | --- | --- |
| `ctx.advance()` | 无 | 未完成时仅补全；完成后走纯控制器。choice/end 不默认跳转 |
| `ctx.choose(optionId)` | string | 未完成时忽略；完成后验证当前场景、选项存在及条件，非法/隐藏选项报错 |
| `ctx.restart(frame?)` | 已声明起始幕 ID/name | 恢复剧情变量初值并重置历史；首次或多起始幕必须显式传参；运行中的单入口可省略 |

这些方法没有可用的同步返回值；读取 ctx.game 仍是事件开始时的旧场景。没有 `jump/setFrame`，也不能从 UI ctx 直接修改剧情变量；v0.9.0 的 `ctx.resume(frameId)` 是受隐藏交接事务约束的显式 UI 返回命令，见第 32 节。与剧情相关的更新应放 `frame(...).advance/choice`。

restart 不会自动清除 CG、设置或 UI 局部状态；若在菜单中使用，可按页面栈情况在同一回调中 close，或 open 非模态 hud。

### 10.4 存读档

| 方法 | 参数 | 行为 |
| --- | --- | --- |
| `ctx.save(slot, overwrite = false)` | slot: 1–200 的整数 Number；overwrite: boolean | 保存当前剧情快照；已有文件默认拒绝，必须显式 true 才覆盖 |
| `ctx.load(slot)` | 1–200 的整数 Number | 严格校验故事/内容版本、变量、检查点和历史；成功才替换剧情，当前句整句显示并重建音频 |
| `ctx.slotsPage(page)` | 1–20 的整数 Number | 切换手动槽分页；刷新当前页元信息，无剧情变化 |
| `ctx.loadAuto(slot)` | 1–10 的整数 Number | 只读加载独立自动槽，严格验证后恢复 |

slot 不接受 BigInt；从 UI int 得到槽号时用 `Number(ctx.ui.pendingSlot)`，1–200 转换安全。save 不会自动弹确认框，XML/JS 作者要自行提供确认界面。坏档/空槽拒绝 load，旧会话不变。旧存档与旧导入命令均不接受，不包含迁移实现。

存读档成功不自动关闭页面或显示成功消息，作者可在同一回调中 close/setUi。回调只是声明要执行的操作，不能通过 try/catch 在 JS 内同步捕获后续宿主 IO 失败；宿主会提示错误并放弃局部 UI 更新。

### 10.5 设置

```javascript
ctx.setting(name, value);
```

| name | 类型/范围 | 默认值 |
| --- | --- | --- |
| `text_speed` | Number，有限，0–80 | 30 |
| `muted` | boolean | false |
| `bgm_volume` | Number，有限，0–1 | 0.35 |
| `voice_volume` | Number，有限，0–1 | 0.8 |
| `se_volume` | Number，有限，0–1 | 0.7 |
| `video_volume` | Number，有限，0–1 | 0.8 |
| `autosave` | boolean | true |
| `skip_all` | boolean | false（只快进已读；true 为全部快进） |
| `fullscreen` | boolean | false（窗口化；true 为当前显示器无边框全屏） |
| `auto_delay` | Number，有限，0.2–15 秒 | 1.5 |

名称区分大小写，不接受其他设置项。数值在宿主按 f32 存储，不是精确十进制。变更原子写入独立 settings.json，成功后生效；text_speed=0 会立即补全当前句。autosave 控制后续场景进入时的自动写档；关闭后不删除已存在自动槽。skip_all 只改变按住 Ctrl 时的快进范围，见第 23 节。

```javascript
ui("resetAudio").bind("reset-audio", ctx => {
  ctx.setting("muted", false);
  ctx.setting("bgm_volume", 0.35);
  ctx.setting("voice_volume", 0.8);
  ctx.setting("se_volume", 0.7);
});
```

### 10.6 CG

```javascript
ctx.unlock(cgId);
```

cgId 必须在 XML gallery 清单中。新解锁原子写入独立 profile.json；已解锁 ID 再次调用不重复写入。多个新解锁可聚合成一次写入。不提供任意收藏键、撤销解锁、查询全表、磁盘读写或动态载入图片接口；查看画廊用数据集和作者页面。

### 10.7 一次事件的提交边界

最多收集 32 条命令。宿主先验证整批命令和局部 UI 草稿，再执行一次剧情/IO 类操作；本地赋值或切页不能先部分生效后再报告槽号非法。

| 可以合并的命令 | 说明 |
| --- | --- |
| open/close/setUi/message 多条 | 纯 UI 计划，仍逐项验证 |
| 上述 UI 命令 + 一次 save/load/loadAuto/restart/advance/choose | 例如存档成功后关页；操作失败则不提交 UI 草稿 |
| 上述 UI 命令 + 多个 setting | 聚合为一次 settings.json 写入；每一步值均校验 |
| 上述 UI 命令 + 多个新 unlock | 聚合为一次 profile.json 写入 |
| UI 局部/页面命令 + playSound/sequence/多个音效偏好命令 | 偏好聚合为一次 ui-audio.json 写入；只允许一个 sequence，不能再混合剧情、settings/profile 或存读档操作 |

不能在同一普通 UI 回调中混合 save + setting、load + restart、advance + choose、setting + 新 unlock，或保存两个槽。重复同一种剧情/槽操作也不允许。不要依赖已解锁 unlock 无 IO 的细节来组合不同类副作用。

这是单个剧情操作/单个文件的提交边界，不提供跨多个文件的事务。新场景生命周期可能另外规划 CG 写入，见下一节。JS 异常、命令格式错误、引用/范围错误或所需 IO 失败都会放弃本次计划；剧情进入失败时保留旧 Session 与音频。

### 10.8 UI 演出与音效方法速查

以下方法可用于普通 `ui(id).bind(...)` 回调；新生命周期仅按第 32 节白名单使用，不可在 `game.enter` 中调用；全部返回 undefined，不能同步获得抽中的声音或演出完成结果。

| 方法 | 参数 | 用法与约束 |
| --- | --- | --- |
| `ctx.present(screenId)` | string | 保存隐藏交接槽并打开 UI；不能嵌套交接，详见第 32 节 |
| `ctx.resume(frameId?)` | 可省略的 string | 从隐藏续接点进入指定幕；省略时恢复待续位置，详见第 32 节 |
| `ctx.playSound(id)` | string | 播放 Audio 资源或 ui-media.sound 组；先过滤，再随机，空集静音 |
| `ctx.sequence(id)` | string | 启动 ui-media.sequence；一次事件最多一个，不允许演出重入或与剧情视频并行 |
| `ctx.soundEnabled(enabled)` | boolean | UI 音效总开关，独立持久化 |
| `ctx.soundActor(actorId, enabled)` | string、boolean | 已声明 character；false 禁用该角色的 UI 音效，true 恢复 |
| `ctx.soundExclude(assetId, excluded)` | string、boolean | 已声明 Audio 资源；true 排除，false 解除玩家排除，不能撤销作者 enabled=false |

布尔参数必须是 true/false，不能传字符串、0/1 或 BigInt。偏好只控制 UI 音效；剧情 voice/se/BGM 不受这些角色开关影响。完整 XML 属性、返回页面、示例与排错见 [第 31 节](#31-ui-演出过场视频与角色音效)。

## 11. 进入场景生命周期

```javascript
ui("$game").bind("game.enter", ctx => {
  if (ctx.game.frameId === "accept") ctx.unlock("cg.lin");
  ctx.setUi("showSpecial", ctx.vars.trust >= 2n);
});
```

示例要求 gallery 有 cg.lin、UI variables 有 bool showSpecial，剧情 variables 有 trust。

- 这是可选、全局且唯一的生命周期注册，不是每个 frame 各注册一次。
- 程序首次进入入口、真实剧情切换（含自循环）、成功读档、导入或重开时执行；看见的是候选场景和候选剧情变量。
- 只能收集 `unlock/setUi/message/transition/animate/portrait/live2d`。不能导航页面、保存/读档、改设置、再推进或选择，避免重入。
- item/value 为 null。game.complete 在普通进入时取决于文字速度是否为 0；读档为 true。
- 计划和必要的 profile 写入成功后才提交 Session 与进入音频；失败保留旧状态。初次启动失败则不能建立正常游戏会话。
- 存档历史重放不触发此回调，避免校验旧剧情时产生副作用。
- 工程热更新也会校验当前候选场景的 game.enter 并刷新 UI 局部状态，但不提交新的持久 CG 解锁或重播瞬时动作；UI 栈重建，当前句补全。它不同于普通剧情进入，具体保留/重置范围见第 20.2 节。
- `--check` 会用初始 Session、UI 初值、默认设置和默认 CG 进度验证入口的生命周期计划，但不执行该计划的写盘，不穷举以后场景。不要把调用次数当游戏状态。

## 12. 常用功能组合

本节片段以 [demo UI](../games/demo/ui.xml) 与 [demo UI 脚本](../games/demo/scripts/ui.js) 的名称为基础。按需复制时需同时提供引用的 screen、变量、资源和事件注册。

### 12.1 自有存档与覆盖确认

UI 声明 int pendingSlot（initial=1、min=1、max=200）和 string confirmText、status；定义 save 与 confirm-save 两个模态页面。save 页的 repeat source=slots，存入按钮 event=save-slot；确认按钮 event=confirm-save，取消按钮 event=close。repeat 只展示当前十槽页，分页命令见第 19.1 节。

```javascript
ui("openSave").bind("open-save", ctx => ctx.open("save"));
ui("saveSlot").bind("save-slot", ctx => {
  if (ctx.item.exists) {
    ctx.setUi("pendingSlot", ctx.item.slot);
    ctx.setUi("confirmText", `确认覆盖槽位 ${ctx.item.slot}？`);
    ctx.open("confirm-save");
  } else {
    ctx.save(ctx.item.slot);
    ctx.setUi("status", "存档完成");
    ctx.close();
  }
});
ui("confirmSave").bind("confirm-save", ctx => {
  ctx.save(Number(ctx.ui.pendingSlot), true);
  ctx.setUi("status", "覆盖完成");
  ctx.close(); // 确认页
  ctx.close(); // 保存页
});
```

取消只 close 确认页，不调用 save。多个返回按钮共用同一事件名时，把同一个函数分别绑定到各自控件。

### 12.2 读档页

```xml
<button id="loadSlot" width="110" height="32" text="读取"
        enabled="item.loadable" event="load-slot" />
```

该节点置于 slots 模板中：

```javascript
ui("loadSlot").bind("load-slot", ctx => {
  ctx.load(ctx.item.slot);
  ctx.setUi("status", "读档完成");
  ctx.close();
});
```

loadable 只是界面缓存，最终读取仍会重新校验文件；不能把按钮禁用当成存档校验的替代。

### 12.3 CG 画廊与大图

gallery 模板用 `visible="item.unlocked"` 显示缩略图，`visible="not:item.unlocked"` 显示锁定文字，查看按钮 `enabled="item.unlocked" event="view-cg"`。UI 声明 string cgImage（初值必须是有效 image ID）、cgTitle，并定义 cg-view 模态页：

```xml
<image id="cgFull" width="1280" height="720" image-bind="ui.cgImage" fit="contain" />
<label id="cgCaption" x="40" y="20" width="1000" height="40" bind="ui.cgTitle" />
```

```javascript
ui("viewCg").bind("view-cg", ctx => {
  if (!ctx.item.unlocked) return;
  ctx.setUi("cgImage", ctx.item.image);
  ctx.setUi("cgTitle", ctx.item.title);
  ctx.open("cg-view");
});
```

大图页的返回按钮复用 close 事件即可。解锁应在合适的 game.enter 或独立 UI 回调中完成，不能在 `frame(...).choice` 回调中调用 ctx.unlock。

### 12.4 自定义信息/设置页面

添加一个 screen，包含 label/image/button，注册 open 回调，即可制作作者介绍、操作帮助等页面；无须增加 Rust 窗口。剧情派生条件可在 game.enter 中更新 UI bool 后绑定 visible；只有 UI 点击决定的页签、提示和计数可在普通 `ui(id).bind(...)` 回调中维护。没有逐帧自定义脚本钩子。

## 13. 持久化与内容兼容

默认使用该故事的用户数据目录，Windows 通常为 `%LOCALAPPDATA%\kakera\kakera-avg\data\<story_id>`，以启动日志的“存档目录”为准。`--save-dir <目录>` 可直接指定隔离目录，此时不会额外按 story_id 创建子目录；调试不同游戏时应分别选目录。以下文件都在该目录内，不由 JS 指定路径。

| 状态 | 存储 | 读剧情槽后 | 进程重启后 |
| --- | --- | --- | --- |
| 剧情位置、变量、历史 | slot-1.json … slot-200.json，schema 9 | 替换为槽内容 | 默认从入口启动，用户读槽后恢复 |
| 自动保存 | auto-1.json … auto-10.json，schema 9 | 替换剧情，自动槽本身不写回 | 保留十份轮换文件 |
| 文字速度、自动保存/播放延迟、静音、四通道音量、Ctrl 快进范围 | settings.json | 不被槽替换 | 恢复设置 |
| CG 解锁 | profile.json，schema 1 + story_id | 不被旧槽撤销 | 恢复解锁 |
| 已读进度（Ctrl 只快进已读的依据） | read.json，schema 1 + story_id + frame ID 集合 | 不被槽替换 | 恢复已读集合 |
| UI 变量、页面栈 | 普通游戏仅内存；活动交接另存隐藏槽 | 普通槽不重置，回调可显式改变 | 有效活动隐藏槽恢复保存的 UI；否则恢复 XML 初值与 initial 页 |
| UI 角色/单条音效偏好 | ui-audio.json，schema 1 | 不被剧情槽替换 | 恢复偏好 |
| 剧情与 UI 交接 | flow-checkpoint.json，schema 1，内嵌 save 9 | 活动交接不允许普通读档 | active 时恢复 UI 与待续点；inactive 不重播 |
| 逐字进度、音频指针、JS 闭包/堆 | 不入档 | 当前句完整、音频重建 | 不恢复 |

schema 9 剧情槽包含当前位置、变量和最多 1000 条历史，以及历史起点变量检查点。裁剪历史时推进检查点；读取时重放保留历史的条件/更新/目标，并核对终点变量，全部通过才提交。

更改场景 ID、路由语义、剧情变量声明或更新规则可能让旧历史不再可重放；需要显式提升 content-version，而非悄悄接受。只调整 UI 布局/皮肤通常不改变剧情兼容版本。当前 profile 校验 story_id 和 CG 清单，但不含 content-version；移除已解锁 CG ID 会使旧 profile 报错，应保留稳定 ID。

v0.9.0 严格读写 schema 9，包含 generation（手动档为 0；自动档为轮换序号）。旧 schema、story_id 或 content_version 不匹配均拒绝，保留当前 Session 和原文件。示例见 [v0.9.0-save.example.json](v0.9.0-save.example.json)。不自动迁移历史槽。

设置/CG 损坏时使用默认值/初始解锁并提示，加载时不自动重写原文件。以后明确保存或新的解锁可能替换该文件。各 JSON 总量上限 2MiB，原子写入使用同目录临时文件及平台替换；失败保留旧文件，不承诺所有文件系统的掉电持久性。存档一致性校验不是防篡改签名。

## 14. 校验、预算与排错

### 14.1 XML 通用规则

未知元素、未知/错位属性、命名空间、重复 ID、缺失引用、资源类型不匹配都拒绝加载；不是浏览器的“忽略无效属性”行为。大多数 XML 诊断带文件、行、列，整体数量/入口等错误可能只带文件。普通结构节点不可含正文，允许格式缩进和注释。

### 14.2 JavaScript 执行环境

JS 使用项目锁定的 rquickjs/QuickJS 系列运行时，不是 Node.js 或浏览器：

- 按 scripts 顺序加载普通严格脚本；支持普通函数、对象、数组、BigInt 等语言计算。不是 ES module；没有 require、宿主模块加载器、DOM、window、document、fetch、文件/网络/进程 API 或计时器。
- Date、WeakRef、FinalizationRegistry、SharedArrayBuffer、Atomics 不开放；Math.random 调用报错。
- 回调须同步完成；Promise 返回、pending Promise 任务与异步回调不受支持。不能用异步任务绕过返回值或预算检查。
- **每次求值创建新隔离环境并重新执行缓存脚本完成注册**。全局计数器、闭包、对象修改不会跨事件保存。源码缓存避免读盘，不等于 JS 全局状态持久化。
- when 可能在初始检查、目标提交、读取/校验历史等阶段重复执行，主剧情回调也会在历史重放中执行。不要依赖次数、时钟、随机或隐藏状态。
- 错误保留外部 JS 文件名/栈；Rust 侧校验错误可能只给出目标、变量或命令原因。

预算汇总：

| 项目 | 当前上限 |
| --- | --- |
| JS 文件数 | 1–32 |
| JS UTF-8 源码总量 | 1MiB |
| 单次求值 | 200ms 或超过 10000 次中断检查时停止，包含源码注册 |
| JS 堆/栈 | 16MiB / 256KiB |
| 一次剧情回调更新 | 128 项 set/add |
| 一次 UI 回调命令 | 32 条 |
| UI XML | 源文件累计及合并后各 1MiB；最多 128 文件/16 层引入 |
| UI 页面数/页面栈 | 1–64 / 最多 8 层 |
| UI 节点数/嵌套深度 | 全部 screen 总计 1024 / 8 层，screen 子节点为第 1 层 |
| CG | 256 项 |
| 剧情/UI 变量 | 各 128 项；字符串单值 4096 UTF-8 字节 |
| ContentSource 内容 | read 内存入口 128MiB；open/copy_to 流式入口 64GiB，解码器另有限制 |

200ms 是每次求值的中断预算，不是整次读档、所有条件求值或一帧总耗时保证；长历史重放会进行多次求值。

### 14.3 常见问题

| 现象/错误 | 检查方法 |
| --- | --- |
| 未知属性 onclick、src、style | button 用 event 和 image 资源 ID；没有网页事件/CSS；src 只放相应文件/资源声明 |
| UI 事件未注册 | 将 UI JS 加入 story scripts；核对大小写；节点不可引用 game.enter |
| 缺少场景触发器/选项不一致 | 每个 frame 完整注册，XML 全部选项各一个 `frame(...).choice`，包括条件隐藏项 |
| UI 回调必须返回 undefined | 不要返回字符串、命令对象或 ctx.setUi 以外的计算结果；使用花括号函数体 |
| BigInt 与 Number 运算错误 | int 用 1n、2n；槽位/设置数值要求 Number，按其安全范围显式转换 |
| setUi 后立即读不到新值 | UI ctx 是输入快照，用局部变量保存新值；不同于剧情回调重新读取 ctx.vars |
| 台词未完成却看见选项 | choices repeat 加 visible=game.complete，或选项按钮 enabled=game.complete |
| 页关不掉或打开失败 | 根页不能 close；modal=true 页不能重复入栈；返回根页可 open 非模态页面 |
| 保存后菜单仍在 | save 不自动 close；在同一回调显式导航 |
| read/save/setting 混合失败 | 拆成不同用户事件；同一回调至多一个剧情/IO 类操作，只有同类 setting/unlock 可聚合 |
| 图片绑定错误，页面不可见也报错 | image-bind 的 UI 字符串初值/最终值必须是已有 image ID，不是空字符串或路径 |
| 文本溢出 | 检查字号、宽高；台词拆句或增大 label，不要静默依赖裁剪 |
| 变量值/台词中出现 `${...}` 原文 | XML 不做插值；UI 动态文字先通过 JS 写入已声明 string，再 bind=ui.xxx |
| 修改文件没变化 | 确认处于 --project 工程状态；展开“工程状态”查看错误，发布包需要重新打包/重启；只有验证成功的候选才会替换 |
| `--check` 通过但某按钮报错 | --check 不遍历每个交互或动态目标；实际操作对应路径并检查宿主提示/JS 栈 |

检查流程建议：先运行 --check，再实际检查中文/溢出、逐字/菜单暂停、条件组合、覆盖取消、完全退出重启后读档，以及 CG/设置独立性。仅改文档无需宣称重新完成所有 GUI/听感验收。

## 15. 旧格式说明

v0.9.0 只接受 story 8 / UI 3 / save 9。旧 story 1–7、UI 1–2、存档 1–8 均拒绝；没有 flow.xml、默认声明式执行器、兼容菜单、旧档导入或设置字段补齐。旧版本内容与接口说明仅在 doc/archive 及历史设计文档保留，当前程序不能运行它们。迁移需作者手工准备新契约并使用新的内容版本，不自动改写用户旧文件。

## 16. 实现位置与扩展边界

本手册使用的源码核对入口：

以下实现链接相对源码仓库解析，便携运行包不包含 Rust 源文件；游戏作者使用 XML/JS 接口无需打开这些实现文件。

| 内容 | 实现 |
| --- | --- |
| story、资源、visual、frame、脚本清单 | [loader.rs](../src/loader.rs) |
| 变量声明与类型校验 | [variables.rs](../src/loader/variables.rs) |
| frame/ui/ctx、类型传输与只读快照 | [bridge.js](../src/script/bridge.js) |
| QuickJS 加载、注册/目标验证及预算 | [script.rs](../src/script.rs) |
| UI 节点/绑定白名单、按钮 route、默认值 | [interface.rs](../src/interface.rs) |
| UI include 装配、预算、源位置映射 | [source.rs](../src/interface/source.rs) |
| 数据集与 game 字段 | [data.rs](../src/interface/data.rs) |
| 节点实际绘制、事件输出 | [render.rs](../src/interface/render.rs) |
| 命令验证、页面栈、UI 状态、CG | [host.rs](../src/interface/host.rs) |
| IO/剧情提交与生命周期装配 | [custom.rs](../src/app/custom.rs)、[app.rs](../src/app.rs) |

新增页面、皮肤、布局、UI 状态及已有宿主能力的交互只需改 XML/JS。新增原生控件或宿主能力需要扩展 Rust 的明确类型、解析与校验，不能在 XML 中任意执行 Rust，也不提供通用插件框架。

目前不包含 CSS/DOM、富文本/台词插值、作者自定义 shader/遮罩/任意关键帧、任意立绘坐标、异步脚本、回滚、全尺寸截图导出、音频淡变/精确进度恢复或可视化编辑器。可运行的完整创作参考始终是 [games/demo](../games/demo/story.xml)，历史目录只是相应旧契约示例。

本文示例已切换为当前 v0.9.0 契约（story 8/UI 3/save 9）；历史 v0.7.1 的示例执行记录只见归档。当前实际运行检查以 games/demo 与本版验收记录为准，片段需按各节说明补齐上下文。

## 17. 内置转场与图片效果

这些效果使用当前 story schema 8 / UI schema 3。单纯视觉变化无需提升 content-version；当前无效果内容采用默认呈现。

### 17.1 场景转场

在目标 frame 内声明一次：

```xml
<frame id="question" visual="room.lin" speaker="lin">
  <transition kind="dissolve" duration="600" easing="ease-in-out" />
  <text>要一起去看看吗？</text>
  <!-- options / audio 按原契约配置 -->
</frame>
```

| 属性 | 必填/默认 | 限制 |
| --- | --- | --- |
| `kind` | 必填 | 下表中的内置名称 |
| `duration` | 默认 600 | 0–5000 的整数，毫秒；0 立即完成 |
| `easing` | 默认 ease-in-out | linear / ease-in / ease-out / ease-in-out |

| kind | 效果 |
| --- | --- |
| `none` | 立即显示目标 |
| `dissolve` | 新旧场景分层透明度渐变 |
| `fade-black` / `fade-white` | 旧画面淡至黑/白幕，再显现新画面，中点为全黑/全白 |
| `wipe-left` / `wipe-right` | 新画面从右/左边缘向左/右揭开 |
| `wipe-up` / `wipe-down` | 新画面从下/上边缘向上/下揭开 |
| `slide-left` / `slide-right` | 新旧画面共同向左/右移动一个画面宽度 |
| `slide-up` / `slide-down` | 新旧画面共同向上/下移动一个画面高度 |

效果作用于 XML scene 节点内的背景与立绘，保持该节点裁剪；不会把 HUD、文字和菜单一起移走。没有 scene 的自定义页面不会显示剧情转场。dissolve 使用现有纹理的分层混合，不提供完整场景离屏合成或自定义 GPU 后处理。

转场的时间属于 App 呈现状态：剧情/变量/历史与音频在成功进入时已提交，画面随后过渡；失败的进入不会启动新效果。自循环同样播放。转场期间不累计逐字时间，`game.complete=false`，`game.transitioning=true`，选项仍需遵循原门控。第一次继续/空格只跳过转场，下一次继续按补全/推进规则处理。

菜单、宿主提示和失焦暂停全部效果计时，恢复不补算。保存只保存已经提交的目标场景；读档直接显示最终画面，不重播转场和进入动画。重新开始按正常入口效果呈现。动画指针不进入存档。

### 17.2 图片预设与引用

story 根元素可有一组 effects（与旧 flow 内的变量 effects 不同）：

```xml
<effects>
  <image-effect id="photo.memory" filter="sepia" opacity="0.9" />
  <image-effect id="photo.soft" filter="blur" blur="4" />
  <image-effect id="photo.pixel" filter="pixelate" pixel-size="24" />
  <image-effect id="portrait.enter" animation="fade-in" duration="800" />
  <image-effect id="portrait.mirror" flip-x="true" scale="1.1" rotation="-5" tint="#DDEEFF" />
</effects>
```

最多 64 项，ID 必填且唯一。一个预设可组合静态过滤、变换与一个进入动画；无继承/嵌套/任意关键帧。引用方式：

```xml
<!-- story 的 visuals 内 -->
<visual id="room.memory" background="bg.room" background-effect="photo.memory">
  <portrait character="lin" image="portrait.lin" slot="center" effect="portrait.enter" />
</visual>
<!-- ui 的 screen/panel 内 -->
<image id="memoryPreview" width="480" height="270" image="bg.room" effect="photo.memory" fit="contain" />
<panel id="softPanel" width="600" height="300" image="ui.panel" effect="photo.soft" />
```

background-effect 及 portrait.effect 在加载时展开为完整显示状态。UI 的 effect 只支持 image/panel；scene 从当前 frame 获得图片效果，button 不接受 effect。panel 的效果仅作用其图片，不变换子控件。填色 panel 无图片，因此不会显示图片效果。

| image-effect 属性 | 默认 | 合法值/含义 |
| --- | --- | --- |
| `id` | 必填 | 引用名 |
| `filter` | none | none / grayscale / sepia / blur / pixelate，每项选一种 |
| `blur` | 4 | 1–12 整数；仅 filter=blur 接受 |
| `pixel-size` | 16 | 1–64 整数；仅 filter=pixelate 接受；近似像素块尺寸 |
| `opacity` | 1 | 0–1 有限数值，与素材透明度相乘 |
| `tint` | #FFFFFFFF | #RRGGBB 或 #RRGGBBAA，与 UI color 及图片颜色相乘 |
| `scale` | 1 | 0.25–4，围绕图片布局矩形中心缩放 |
| `rotation` | 0 | −360–360 度；正值在屏幕坐标中顺时针，围绕中心旋转 |
| `flip-x/flip-y` | false | true/false，纹理水平/垂直翻转 |
| `animation` | none | 下一节的内置名称 |
| `duration` | 400 | 0–5000 整数毫秒，0 为直接到最终状态 |
| `strength` | 12 | 0–128 有限数值，设计坐标单位，用于震动/滑入位移 |
| `easing` | ease-in-out | linear / ease-in / ease-out / ease-in-out |

灰度和棕褐保持原 alpha；模糊使用预乘 alpha 处理，避免透明区域的隐藏颜色造成边缘色晕；像素化使用缩小/放大采样。过滤在资源准备阶段处理：D3D11 用 HLSL 生成 GPU 缓存，CPU fallback 使用原算法；绘制只读取缓存，不执行 IO 或解码。工程更新成功后重建相关缓存。每个声明的过滤参数组合会为共享图片表准备变体，额外缓存总量最多 256MiB；超限启动报错，应减少组合或图片尺寸。缩放/旋转/翻转不额外生成纹理。

### 17.3 图片进入动画

| animation | 行为 |
| --- | --- |
| `none` | 无动画 |
| `fade-in` | 从透明到预设最终 opacity |
| `zoom-in` | 从最终 scale 的 85% 放大到最终大小 |
| `zoom-out` | 从最终 scale 的 115% 缩小到最终大小 |
| `shake` | 确定性双轴衰减震动，最终偏移为 0 |
| `flash` | 图片区域闪白，白色覆盖逐渐消失 |
| `slide-left/right/up/down` | 从反方向 strength 位移滑至正常位置 |

动画均为单次，结束回到预设的基准变换/透明度，没有永久偏移、无限循环或队列。strength 不控制 zoom 幅度、fade-in 或 flash。场景素材预设从本次成功进入计时；UI 图片预设在场景进入或页面栈变化时重新计时，不是逐帧检测 visible 切换。新页面若 pause=true 也会暂停计时，需播放菜单动画时应按需求设 pause=false。所有 UI 图片仍按节点及祖先区域裁剪。

### 17.4 JavaScript 选择转场与播放动画

```javascript
// 只可在 game.enter 中调用；基于候选场景/变量选择本次转场
ui("$game").bind("game.enter", ctx => {
  if (ctx.game.frameId === "accept") ctx.transition("fade-white", 900, "ease-out");
});

// 普通 UI 事件，target 是 XML 节点 ID
ui("shakeButton").bind("shake-preview", ctx => ctx.animate("memoryPreview", "shake", 600, 20));
```

| 方法 | 参数与默认值 | 校验与提交 |
| --- | --- | --- |
| `ctx.transition(kind, duration=600, easing="ease-in-out")` | kind 与 XML 同名；duration 为整数 Number 毫秒 | 仅 game.enter；覆盖目标 frame 的 XML 转场，本次进入最多一次；none 可取消 XML 转场 |
| `ctx.animate(target, kind, duration=400, strength=12, easing="ease-in-out")` | target/kind/easing 为 string；duration 为整数 Number；strength 为有限 Number | 普通 `ui(id).bind(...)` 回调或 game.enter 均可；目标必须是非 repeat 内的 scene，或带 image/image-bind 的 image/panel |

target 不接受素材 ID、CSS 选择器、repeat 实例索引或普通 button/label；重复模板节点无法唯一定位，故拒绝。多个命令指向同一个目标时，最后一次替换其活动动画，不排队。scene 的 JS 动画作用整个场景区域；图片/面板只改变自身图片，不移动文字和点击区域。关闭后不可见的节点仍按共同呈现时钟计时，不延迟到再次可见才开始。

这些方法不执行 JS 动画循环，只收集经过 Rust 校验的瞬时呈现计划。仍计入每个回调的 32 条命令预算，可与合法的切页/设置等组合；不新增剧情/IO 操作类别。非法 kind/参数/target 导致整批计划失败，不先更新 UI 或重启动画。game.enter 允许 unlock/setUi/message/transition/animate/portrait/live2d，仍禁止导航和重入剧情。历史重放不播放效果，成功读档也不重播这些瞬时动画。

可运行示例见 demo 的“效果演示”页：四种过滤预览、图片/场景动画按钮，以及全部方向转场按钮。演示转场会显式重新开始剧情，作者正式游戏可按剧情目标配置，不需要额外 Rust 代码。

演示页附有 0–5000ms 时长滑杆及“暂停预览”：后者重开剧情并打开历史，便于观察暂停状态；历史中可选择“跳过转场”返回，或正常返回继续播放。该操作是 demo 自己通过 XML/JS 组合的行为，不是新的内置页面命令。

## 18. 人物立绘动作

本版新内容使用 story schema 8、UI schema 3、存档 schema 9。动作只作用 scene 中的 left/center/right 立绘，不作用 UI image；UI 图片继续使用第 17 节的 `ctx.animate`。XML/JS 通过 ContentSource 快照读取；工程中监控修改并验证更新，发布中使用固定资源包。

### 18.1 XML 属性与最终姿态

```xml
<visual id="lin.fallen" background="bg.room">
  <portrait character="lin" image="portrait.lin" slot="center"
            pose="fallen-right" enter="fall-right" exit="fade-out"
            duration="1200" strength="32" easing="ease-out" />
</visual>
```

| 属性 | 默认 | 取值与含义 |
|---|---|---|
| `pose` | `normal` | `normal` 正常；`flipped` 水平镜像；`fallen-left/right` 向左右旋转 90°且下移 64 设计单位；`hidden` 隐藏 |
| `enter` | `none` | 本帧每次成功进入时播放的动作，见下表；自循环也重播 |
| `exit` | `none` | 旧立绘被移除或同槽换人物/图片时自动播放，只能 `none/fade-out/exit-left/exit-right/exit-up/exit-down` |
| `duration` | `600` | enter/exit 共用，整数毫秒，0–5000；0 直接最终态 |
| `strength` | `32` | enter/exit 共用，有限数值 0–128；控制震动/弹跳幅度，单位为 1280×720 设计坐标 |
| `easing` | `ease-in-out` | `linear/ease-in/ease-out/ease-in-out`，enter/exit 共用 |

每个 Frame 的 XML 声明完整最终显示状态。动作结束、读档或动作被替换后以 `pose` 为准；不隐式继承前一帧朝向。修改长期朝向、倒地或隐藏状态，应进入声明对应 pose 的 Frame。`effect` 仍可叠加；最终镜像与 effect.flip-x 组合，旋转角度与 effect.rotation 相加。

### 18.2 动作列表

| `enter` / JS action | 过程 | 所需最终 pose |
|---|---|---|
| `none` | 无动作，立即显示最终姿态 | 任意 |
| `fade-in` | 透明度从 0 到最终值 | 除 hidden |
| `fade-out` | 正常站姿淡出至隐藏 | hidden |
| `enter-left/right/up/down` | 从指定侧画面外滑入最终姿态 | 除 hidden |
| `exit-left/right/up/down` | 正常站姿向指定侧滑出并淡出 | hidden |
| `shake` | 水平/垂直衰减震动，最终复位 | 除 hidden |
| `bounce` | 向上弹跳一次，最终复位 | 除 hidden |
| `turn` | 水平压缩穿过侧面，再展开到目标镜像方向 | normal / flipped |
| `spin` | 水平压缩/镜像模拟两圈，最终回到目标方向 | 除 hidden |
| `fall-left/right` | 从站姿旋转到 ±90°，同时下移 64 | 对应 fallen-left / fallen-right |

滑入/滑出距离固定 4096 设计单位，不由 strength 控制；场景区域裁剪保证不会画到菜单外。turn 从最终朝向的反面开始；重播会再次从反面开始。spin 为平面翻转模拟，不是 3D 或骨骼动作。摔倒绕图片中心旋转，不做地面碰撞；美术应保留合理透明边距。

`enter="exit-left" pose="hidden"` 是新帧中显式退场：仍需给出人物、图片和槽位，开始时使用正常站姿。希望从旧帧的镜像/倒地姿态退场，可在旧立绘设置 `exit`，下一帧省略该立绘，系统会以旧帧最终姿态生成短暂退场快照。相同人物和图片仍占同槽时不自动退场；移动到另一槽会触发旧槽 exit 和新槽 enter。

### 18.3 JavaScript 接口与原子校验

```javascript
// 普通 UI 回调：摇动当前中间槽，结束后保持 XML 姿态。
ui("linShake").bind("lin-shake", ctx => ctx.portrait("center", "shake", 800, 24, "ease-out"));

// 与选择组合：宿主验证选择成功后的候选场景。
// 选项 fall 的目标 Frame 必须有 center 槽及 pose="fallen-right"。
ui("fallChoice").bind("fall-choice", ctx => {
  ctx.portrait("center", "fall-right", 1200);
  ctx.choose("fall");
});

// 在已有唯一 game.enter 回调内添加；同一完整注册键不能重复绑定。
ui("$game").bind("game.enter", ctx => {
  if (ctx.game.frameId === "surprised") ctx.portrait("center", "bounce", 700, 28);
});
```

签名：`ctx.portrait(slot, action, duration=600, strength=32, easing="ease-in-out")`。slot/action/easing 为 string，duration 为整数 Number，strength 为有限 Number。slot 只能 `left/center/right`，动作与参数见上表。允许普通 `ui(id).bind(...)` 回调和 game.enter；剧情 `frame(...).advance/choice` 的 ctx 不提供此方法。

该命令只收集宿主动作计划，不执行脚本动画循环。计入每次回调 32 条 UI 命令预算，原 QuickJS 时间/内存/命令限制不变。同一槽后一个命令替换前一个，不排队；不同槽可同时播放。`none` 可取消本槽动作并回到声明姿态。

宿主验证槽位存在、姿态匹配、参数范围后再执行整个批次。失败不提交 UI 变量、设置、存档写入、剧情或动作；与剧情推进/选项/读档/重开组合时验证候选 Frame。若输入只补全文字或跳过转场而未进入新场景，验证当前 Frame。隐藏槽存在仍可执行匹配的退场动作；不存在的槽报错。不能通过此方法永久隐藏、修改图片或绕过 XML 最终状态。

### 18.4 生命周期、存档与绘制

- 立绘动作不阻止逐字、选项与推进；新的剧情进入取消旧动作，从旧帧最终姿态建立必要的退场快照。全场转场继续按第 17 节阻止逐字与选项。
- 暂停菜单、通用提示和失焦均暂停立绘动作，恢复后不补算。页面切换不重置立绘动作；scene 可在另一页继续展示同一时钟。
- 读档直接恢复 XML 最终姿态，不重播 enter、exit 或 game.enter 的瞬时动作；重开正常播放。存档不含动作、时钟、退场快照或 UI 页面栈。
- 同场可绘制旧人物退场和新人物入场。全场转场的旧层裁剪/覆盖优先，自动退场时间最多为该转场时长；转场跳过也清理旧快照，避免人物重新出现。
- 网格镜像/旋转/透明度由已有缓存纹理绘制，不逐帧读盘、解码或生成新纹理。无关键帧编辑器、碰撞、物理、动作队列或任意槽位布局。

### 18.5 可运行样例

demo 正常进入问题页后选择“立绘动作演示”，打开专用 XML 舞台；右侧列表可滚动，包含全部演示分支和“返回剧情”。“重播当前动作”通过 `ctx.portrait` 实现；“移除立绘自动退场”演示旧立绘 exit，空场重播不执行动作。

动作中可打开历史暂停，也可保存。完全退出后读档，再点 HUD 的“立绘舞台”可查看倒地/镜像/隐藏最终姿态；页面栈不入档。新增分支保留原四变量及原剧情路由，内容版本仍为 3；旧历史仍可重放。


## 19. v0.6 分页、自动保存、自动播放、视频与 Live2D

### 19.1 两百个手动槽与分页

手动槽号为 **1–200**，每页 10 个，共 20 页。`repeat source="slots"` 只提供当前页；其 item.slot 是实际全局槽号，第 20 页为 191–200。菜单只读元信息，`loadable=true` 表示基本身份/格式可读，完整变量、历史检查点和脚本重放在 load 边界执行；深层损坏仍可能在加载时拒绝。原文件和当前剧情都不因此改变。

```xml
<!-- 放入已有 screen；页脚位置按自己的布局调整 -->
<label id="pageText" x="20" y="580" width="500" height="32" bind="saves.pageText" />
<button id="prevPage" x="540" y="580" width="100" height="36"
        text="上一页" enabled="saves.canPrev" event="slots-prev" />
<button id="nextPage" x="650" y="580" width="100" height="36"
        text="下一页" enabled="saves.canNext" event="slots-next" />
<slider id="jumpPage" x="760" y="580" width="200" height="36"
        min="1" max="20" value="saves.page" event="slots-page" />
```

```javascript
ui("prevPage").bind("slots-prev", ctx => ctx.slotsPage(ctx.saves.page - 1));
ui("nextPage").bind("slots-next", ctx => ctx.slotsPage(ctx.saves.page + 1));
ui("jumpPage").bind("slots-page", ctx => ctx.slotsPage(Math.round(ctx.value)));
// 范围外直接报错，不会钳制到第一页或最后一页。
```

页码为 UI 瞬时状态，与保存剧情槽无关。分页命令可与 open/close/setUi 组合；game.enter 中禁止换页。普通保存需自行实现覆盖确认，自动槽不能被 ctx.save 覆盖。

### 19.2 自动保存

默认开启。首次建立剧情、Advance、Choose、自循环、重新开始等**成功进入场景后**写一次；补全文字、UI 切页、普通动画、设置变化和成功读档均不写。它记录已提交 Session，不重复重放整个历史。XML UI 为必需，自动档由独立设置控制；不存在其他呈现路径。

- 独立 `auto-1.json` … `auto-10.json`，按 generation 单调递增并轮换。菜单按 generation 从新到旧排列；槽号不是时间顺序。系统时间变化不改变轮换顺序。
- 自动文件使用与手动档相同的原子替换；写失败保留旧文件，保留已经成功提交的新剧情，并提示且关闭本次运行的 autosave 开关，避免反复失败。用户修复存储问题后可在设置重新开启。
- `repeat source="autosaves"` 提供已存在的自动槽；坏槽保留可见条目但 loadable=false。禁止覆盖或删除自动档的 JS 接口。
- `ctx.loadAuto(slot)` 与手动 load 相同：候选 Session、可用选项、进入命令和媒体准备全部通过才提交。读档后自动播放停止，视频/模型从声明起点重建。

```javascript
ui("autosaveToggle").bind("autosave-setting", ctx => ctx.setting("autosave", ctx.value));
ui("loadAutoSlot").bind("load-auto-slot", ctx => {
  ctx.loadAuto(ctx.item.slot);
  ctx.close(); // 按实际页面栈决定关闭几层
});
```

### 19.3 自动播放

```xml
<button id="auto" x="260" y="20" width="150" height="36"
        bind="game.autoText" event="toggle-auto" />
```

```javascript
ui("auto").bind("toggle-auto", ctx => ctx.autoPlay(!ctx.game.autoPlaying));
ui("autoDelay").bind("auto-delay", ctx => ctx.setting("auto_delay", ctx.value));
```

`ctx.autoPlay(enabled)` 只接受 boolean，属于瞬时呈现命令，禁止在 game.enter 中调用。文字完整、转场结束、当前语音完成后，等待 auto_delay 秒提交一次 Advance；不会自动补全正在显示的句子。无设备或 --no-audio 时语音视为已结束。

选项页和 end 自动停止；不会选择默认分支。手动继续/选择、读档、重开停止自动模式。菜单/提示/失焦和视频期间暂停计时，不积累等待时间。每次更新用户输入优先，至多执行一个剧情操作。auto_delay 保存到 settings.json，autoPlaying、等待时钟不入档。

### 19.4 UI 视频、结束与跳过

视频资产仍在 story.assets 声明 `<video id="movie.op" src="assets/video/op.mp4"/>`；播放节点只允许出现在 UI 的 ui-media.sequence 内：

```xml
<ui-media>
  <sequence id="movie.op.sequence" finish="close" skippable="true">
    <video asset="movie.op"/>
  </sequence>
</ui-media>
```

UI video 只接受 asset，skippable 写在 sequence，结束行为写在 finish；没有 advance 属性。鼠标主键/空格跳过整个演出，完成与跳过共用一次性结束路由。`finish="close"` 要求启动页有下层页；要自动续接剧情，在 `ui(页面ID).bind('screen.close', ...)` 回调使用 ctx.resume。完整示例见第 31–32 节。

视频音轨使用 video_volume/muted，播放期间暂停剧情 BGM/语音/音效。菜单的剧情暂停不暂停 UI 演出；失焦、宿主提示和调试暂停同时冻结视频时钟与音轨。隐藏槽恢复时从零播放视频，不保存解码指针，不提供 seek、外挂字幕或网络流。

Windows x64 使用系统 Media Foundation，验证 H.264/AAC MP4（720p/30fps）；其他格式取决于系统解码器。视频最多 16 段，单文件流复制最多 64GiB，最大 1920×1080、约 60fps、300 秒；视频音频转为 48kHz 双声道浮点 PCM，总缓存 256MiB。所有视频读取走 ContentSource，至多一个有界解码工作线程，App 更新时上传，绘制只读；长视频的音轨预解码会增加启动时间。

### 19.5 Live2D SDK 与模型资源

需要官方 Windows x64 **Live2DCubismCore.dll**；本版对接用户提供的 CubismSdkForNative-5-r.5，通过真实 moc3 形变与网格渲染验收。Core 是宿主的受信任原生依赖，只能由命令行、CUBISM_CORE 环境变量、程序旁固定 DLL 或源码内固定宿主 runtime 提供，游戏 JS 不可加载 DLL。

```powershell
# 默认 demo 已带独立 Live2D 测试入口，无需生成独立游戏
cargo run --locked
cargo run --locked -- --check
# 显式使用静态替代图
cargo run --locked -- --no-live2d
# 用其他本机 SDK 生成独立副本，已有目录不会覆盖
./tools/prepare-live2d-demo.ps1 -Sdk C:/CubismSdkForNative-5-r.5 -Output target/live2d-copy
```

工程状态 DLL 搜索顺序：`--cubism-core` → `CUBISM_CORE` → exe 旁固定文件名 → 源码根固定 `runtime/live2d/windows-x64/Live2DCubismCore.dll`；发布状态只使用显式配置/环境变量/程序旁 DLL，不回退源码 runtime；不是读取 story 或 model3 指定的 DLL。有模型且未禁用时，缺 DLL/模型错误会明确失败；只有显式 --no-live2d 才使用 fallback。禁用选项仍读取模型结构/素材，无法替代 Core 的 moc3/参数有效性校验。

用户追加要求已将 Haru 和 Windows Core 集成至本地默认 demo 与新便携包。模型位于 games/demo/assets/live2d/Haru，SDK/模型原许可在 assets/licenses/LIVE2D-SDK-LICENSE.md；Core 原许可与可再分发清单在 runtime/live2d（便携包中为 licenses）。本次没有公开发布；第三方内容不是自制 CC0 素材，分发要求以所附原条款为准。引擎未复制 Cubism Framework 实现，Core 由官方 DLL 提供。

问题页选“Live2D 测试”进入 live2d.stage；“切换表情 F02”进入 live2d.expression；“查看参数姿态”进入 live2d.angle。每个都是独立完整 Frame，支持普通存读档。点击动作在 UI choose 事件中将 ctx.live2d 与一次 ctx.choose 原子组合，返回时不修改剧情变量。对应 XML 与回调均在 games/demo，可直接参考。

```xml
<live2d id="haru" src="assets/live2d/Haru/Haru.model3.json" />
```

model3 的 FileReferences.Moc/Textures/Motions/Expressions/Pose 及辅助文件均从 ContentSource 读取；内部相对路径以 model3 目录为基准，仍不允许绝对路径、反斜杠、.. 或越界链接。将整个模型目录放入游戏资源，不只复制 model3.json。KPAK 来源实现同一接口，不需要把包内部路径暴露给脚本。

### 19.6 Live2D XML 与 JavaScript

```xml
<!-- visual 内：image 是必需的静态替代图；与普通立绘共用槽位和姿态 -->
<portrait character="lin" image="portrait.lin" slot="center" pose="normal">
  <live2d asset="haru" motion="Idle:0" expression="F01">
    <parameter id="ParamAngleX" value="0" />
  </live2d>
</portrait>
```

| 字段 | 规则 |
| --- | --- |
| `asset` | 必填，引用 live2d 资源，不是 model3 路径 |
| `motion` | 可省略；`model3 动作组:从零开始的索引`，如 Idle:0；尊重该 motion3 的 Meta.Loop |
| `expression` | 可省略；model3 Expressions 中的 Name，如 F01 |
| `parameter` | 可重复，id/value 必填；ID 唯一，最多 128 项；有限数值，绝对值 ≤10000；启用 Core 时还必须存在且在模型 min/max 内 |

每槽一个实例，随进入事件重建，不暗中继承前帧。每次更新从参数默认值开始，叠加 XML 初值，再应用当前 motion3 与 expression3，最后 Core 校验/钳制参数并更新网格。motion 覆盖同名初值；expression 的 Add/Multiply/Overwrite 再作用在动作结果上。无 motion 时仍显示真实模型静态形变。XML 的立绘 enter/exit/pose 和 ctx.portrait 对合成纹理照常生效；灰度、模糊等预计算 filter 不支持 Live2D，加载阶段拒绝。位置/缩放/翻转/染色/透明度保留。

```javascript
ui("waveButton").bind("wave", ctx => ctx.live2d("center", "TapBody:0"));
// 也可在 game.enter 中对候选场景触发已登记动作。
```

`ctx.live2d(slot, motion)` 两个 string：slot 只能 left/center/right，动作名最长 256 UTF-8 字节且必须存在于该槽模型。按候选 Frame 校验：槽不存在、仅有普通图片或动作未知，都拒绝整批计划，旧 Session/音频/动作保留。可与一次剧情操作组合；不开放设置任意模型路径、参数写入、动态表情、鼠标追踪或任意 JS tick。

JS 动作立即替换该槽当前覆盖动作，不排队，**播放一次**（即使资源 Loop=true），结束后恢复 XML 初始动作的呈现时间。不同槽可同时播放。菜单/失焦暂停，读档只恢复 XML 声明，从起点重建，不保存 Core 指针、动作进度或 JS 覆盖动作。UI/runtime 的 game.live2dEnabled 可用于显示模型专用按钮；game.enter 应依据候选 frameId 设计命令，不能依赖尚未建立的播放实例状态。

### 19.7 Live2D 支持范围与预算

已实现：Core 参数/顶点形变、绘制顺序、贴图、普通/加法/乘法混合、正向/反向裁剪蒙版、乘色/屏幕色、透明度；motion3 线性/贝塞尔/阶梯/反阶梯、Parameter/PartOpacity 与 Model Opacity/EyeBlink/LipSync 曲线；expression3 Add/Multiply/Overwrite；基础 Pose 互斥部件/链接切换。

当前 Windows 优先原生 D3D11：网格、裁剪蒙版、颜色/透明度和混合在 GPU 完成，最长边 **1024 像素**、最高约 **60 次/秒**更新；输出直接作为 UI 原生纹理，常规播放不执行 GPU→CPU 读回。Cubism Core 参数/形变仍使用官方 CPU ABI。CPU fallback 保留最长边 **512 像素**、约 **30 次/秒**和单帧 3200 万采样预算；两种路径最多 32 组蒙版。GPU 不可用/失败会提示并切到 CPU，Core 本身错误或 CPU 预算错误仍提示冻结最后画面。

限制：无 physics3 物理求解、完整 Pose 渐变、动作淡入淡出/优先级/事件、动作附带声音播放、自动呼吸/眨眼/口型音量分析、模型点击命中与鼠标跟随、Cubism 新版 offscreen/高级混合。模型包含 offscreen 或不支持的混合模式会拒绝；Physics/DisplayInfo/UserData 等辅助 JSON 只读取校验，不代表已模拟。motion3 中明确写出的 EyeBlink/LipSync 曲线可用，不等于自动驱动。

模型预算：最多 8 个，moc3+解码贴图合计 256MiB；每模型贴图最多 16 张、单边 8192、合计 128MiB；moc3/模型内存各 64MiB，drawables≤2048。动作≤256，曲线总数≤16384、片段≤200000；表情≤128、参数总量≤8192；JSON 单文件 8MiB。启动 --check 验证全部 XML 模型声明及入口动作命令，后续动态回调仍在提交边界验证。

本版验收使用本地 Haru 样例；不能据此承诺所有 Cubism 模型、平台与渲染特性。后续要提高画质/性能，可替换呈现层合成器，领域数据、脚本与 ContentSource 契约保持解耦。


## 20. 工程热更新、KPAK 发布与渲染后端

工程与发布共用同一套 XML/JS，没有 `reload()`、`ctx.pack`、`ctx.readFile` 或脚本切换运行模式的 API。模式、包路径、Core 和渲染器由宿主命令行选择。完整操作流程见 [独立使用指南](project-release-pack-guide.zh-cn.md)。

### 20.1 运行状态与命令行

```powershell
# 工程：直接编辑 XML/JS/图片/字体/音频/视频/模型，自动验证更新
cargo run --locked -- --project games/demo
# --project 与 --package 不能混用
cargo run --locked -- --project games/demo --renderer cpu
# 发布：只读取包，绝不监控散装目录
cargo run --locked -- --package target/game.kpak --cubism-core runtime/live2d/windows-x64/Live2DCubismCore.dll
cargo run --locked -- --package target/game.kpak --cubism-core runtime/live2d/windows-x64/Live2DCubismCore.dll --check
```

| 播放器参数 | 行为 |
| --- | --- |
| `--project <目录>` | 工程入口；不能与另一个工程/包入口重复或混用 |
| `--package <文件>` | 发布入口，只读取指定 KPAK |
| `--renderer auto\|d3d11\|cpu` | 默认 auto；行为详见第 20.5 节 |
| `--check` | 验证资源后退出，不进入游戏、不监控、不写存档/设置/CG |
| `--save-dir <目录>` | 指定包外用户数据目录，不再自动附加 story_id |
| `--cubism-core <DLL>` | 指定受信任的宿主 Core；不属于游戏资源路径 |
| `--no-live2d` | 显式使用静态替代图，仍读取并校验模型资源结构 |
| `--no-audio` | 禁止设备输出，仍解码/验证音频与视频音轨 |
| `--help` / `-h` | 打印帮助后退出 |

以上路径参数相对当前工作目录，也接受绝对路径；有空格时加引号。无入口参数时，先寻找 **exe 所在目录** 的 `game.kpak`，不是当前目录；存在即进入发布状态（损坏则报错，不回退工程），否则尝试编译时源码根的 `games/demo`。显式 `--project` 可覆盖默认包选择，便携分发必须携带包，不能依赖源码兜底。

--check 不建立监控/窗口/音频设备，默认仅检查数据与资源；Windows 明确指定 --renderer d3d11 时还验证硬件设备及 HLSL 编译，不代表已穷举视觉画面。检查结束返回 0，启动/校验错误返回 1；`--no-audio` 不能绕过坏音频，`--no-live2d` 不能绕过缺失的模型文件。

Windows GUI 子系统程序在 PowerShell 中直接调用可能异步返回；需要自动化等待检查结果时使用 `kakera-avg.exe ... --check | Out-Host` 后读取 `$LASTEXITCODE`，或 `Start-Process -NoNewWindow -Wait -PassThru` 的 ExitCode。Cargo 运行会等待子进程。具体示例见使用指南第 3.1 节。

发布包的 Core DLL 与许可放在 exe 旁，不放进资源包作为可执行依赖；模型/纹理/脚本/字体等内容在 KPAK 内。存档、设置、CG 进度在包外用户数据目录，--save-dir 规则不变。没有从包内执行 DLL 或 JS 任意 IO 的接口。Windows 播放器为 GUI 子系统，发布正常启动不创建或附着控制台；发布启动失败使用原生错误对话框。工程会连接父控制台，必要时建立工程日志窗口；--check/--help 只尝试连接已有控制台并保留重定向，不额外弹窗，kakera-pack 继续是控制台工具。

Core 查找顺序为显式 `--cubism-core` → `CUBISM_CORE` 环境变量 → exe 旁 `Live2DCubismCore.dll`；只有工程及打包工具额外兜底源码固定 `runtime/live2d/windows-x64`。已显式指定的坏路径不自动跳过。相同 story_id 默认共享用户数据，工程与发布不会自动隔离，调试时应分别传 `--save-dir`。

### 20.2 工程监控与提交

使用原生目录通知监控整棵工程树，约 200ms 去抖合并连续保存；后台使用固定快照解析 XML/JS、验证全部资源，完成后再核对工程指纹。构建期间又发生修改时，过期候选不会提交。实际更新时间为去抖加资源准备时间，较大视频/字体/模型会延长等待；不是保证每次保存后 200ms 内完成。

当前有效内容持续运行。候选必须匹配 story_id/content-version，并通过当前 Session 的完整变量/历史重放、条件选项及候选进入命令/媒体校验后，才一次替换当前内容。坏 XML、JS 异常、缺图/坏媒体、变量/路由冲突都会保留旧会话和声音。修复并再次保存会重试；可展开左侧“工程状态”查看状态/错误和当前像素后端，也可手动重新验证。

成功更新保留剧情位置、变量和历史；逐字直接补全当前句，停止自动播放，重建视频/Live2D/音频及纹理，UI 栈/局部变量从新声明重建。game.enter 只用于校验并刷新 UI 局部状态；不写自动档、不新增持久 CG 解锁、不重播 JS 瞬时动画。它是内容替换事件，不能依赖回调执行次数记录剧情。当前画面作为恢复后的最终态建立；下一次正常进入仍以此画面作为转场起点，不会重播本次进入效果。

内容完整有效但与当前 Session 不兼容的候选保留在工程窗中，可点击“从入口重新载入（丢弃当前进度）”使用新初值；语法/资源错误须先修复或“重新验证资源”，不能强制采用坏内容。story_id 改变需重启并选择独立数据目录。不会因编辑变量声明而偷偷重置进度；从入口重载也不迁移旧档或清空独立 CG/设置。发布状态没有工程窗，也不会响应外部散文件修改或暗中更换正在运行的包；换包后重新启动。

编辑器/构建/用户数据不作为游戏内容：.git/.idea/.vscode/target/saves/node_modules、临时文件、settings/profile/read/save.json 与 slot-/auto-*.json 被排除。正常工程引用这些排除路径会因资源缺失而失败；不要将剧情资源命名为保留的用户数据文件。

### 20.3 工程打包工具

```powershell
cargo run --locked --bin kakera-pack -- games/demo target/game.kpak
# 输出已存在默认拒绝，明确 --force 才原子替换
cargo run --locked --bin kakera-pack -- games/demo target/game.kpak --force --level 22
# 独立工具与播放器参数独立
./target/release/kakera-pack.exe games/demo target/game.kpak --level 19
./tools/package.ps1
```

输入必须有 story.xml。输出必须在工程目录之外，避免被自身监控/收集；默认 KPAK 2，可 --format 1 保留小包；Zstandard 级别 19，可选 1–22。先验证故事、UI、JS、字体、图像、音视频、模型与入口计划，再验证工程未变化，生成并自检 KPAK，最后以同目录临时文件+原子替换提交。输出已存在、坏资源、校验期间变化或替换失败均保留旧包；无自动删除/清理用户文件。工具支持 --cubism-core 和 --no-live2d，后者只做禁用 Core 时的资源结构校验，不能代替真实模型验证。

| kakera-pack 参数 | 默认/规则 |
| --- | --- |
| `<工程目录> <输出.kpak>` | 两个必填位置参数，写在选项前；相对当前工作目录，父目录可自动创建 |
| `--format <1|2>` | 默认 2；1 为保持兼容的小包格式，2 为独立分块流式包 |
| `--force` | 允许原子覆盖已有输出；不传则拒绝 |
| `--level <1..22>` | 默认 19；它是压缩级别，不是版本号或图像质量 |
| `--cubism-core <DLL>` | 本次打包校验使用的宿主 Core |
| `--no-live2d` | 仅改变本次校验方式，不删除模型或在包中记录永久禁用标志 |

工具退出 0 表示成功、1 表示失败；没有 `--help` 选项，不带参数打印用法并失败退出。不支持播放器的 --renderer/--save-dir/--check/--no-audio。用 --no-live2d 生成的包正常播放时仍需要 Core，或播放器再次显式禁用。打包先将 next 链上的剧本合并为包内 story.xml，并移除链上其余源 XML；JS 不合并；UI 的根级 include 另行展开为单个完整入口并移除实际子文件，源工程不改写。v0.8.3 先记录引擎真实读取的依赖，并附带约定许可证与显式 include 文件；不再扫描收集其余普通文件。未引用的设计稿/AI 权重/编辑器元数据默认不入包。严格清单与候选一致性规则见第 29 节。

在所有 frame.id 均显式固定时，同样的所选工程字节、同一压缩级别与同一工具实现产生相同包字节，不写时间戳。逻辑路径排序，KPAK 1 完全相同文件共享块并整体 solid 压缩，KPAK 2 以固定 1MiB 原文块跨文件去重并独立压缩；不加密，不有损转码素材。MP4/PNG 已经压缩，二次收益有限；级别 22 可能耗时/内存更多。KPAK 1 整体解压到内存；KPAK 2 只常驻索引并按需读取/验证块。工程和 KPAK 2 打包使用磁盘暂存，不全量存于 RAM；需要临时磁盘空间。播放器仍预先准备已声明的解码资源；本版没有场景级解码惰性加载或网络流。

默认发布脚本输出 target/package/kakera-avg-v0.9.0，包含播放器、game.kpak、Core 和许可/手册，不复制 games/demo 散装内容。kakera-pack.exe 留在 target/release，供工程作者使用。工具本身没有公开上传/发布行为。

脚本支持 `-Project <工程>`、`-Output <新目录>`、`-CompressionLevel <1..22>`、`-Format <1|2>`（默认 2），相对路径以仓库根为准；输出目录已存在就拒绝，失败目录保留供诊断。它复制仓库固定 Core，没有单独的 Core/禁用选项。新目录示例：

```powershell
./tools/package.ps1 -Project games/demo -Output target/package/demo-review -CompressionLevel 19
```

包格式和预算见下一节；完整发布目录结构、手动组装、存档隔离与排错见 [使用指南](project-release-pack-guide.zh-cn.md)。

### 20.4 KPAK 1 / KPAK 2 二进制格式

KPAK 2 的 80 字节头、64 位偏移、1MiB 原文块、尾部索引、按需/完整验证和预算详见 [完整规范](kpak-format.zh-cn.md)。包及逻辑总量最多 1TiB、单文件 64GiB、100000 文件、索引 128MiB。普通打开只验证索引，访问的块先验证再交付；--check 完整扫描所有唯一块。KPAK 1 的下述字节格式保持不变，小包仍可读取：

所有整数为 little-endian，头长 64 字节：

| 偏移 | 长度 | 值 |
| --- | --- | --- |
| 0 | 8 | `4B 50 41 4B 0D 0A 1A 00`（KPAK magic） |
| 8 | 4 | version=1 |
| 12 | 4 | flags=0，未知位拒绝 |
| 16 | 8 | Zstandard 压缩帧长度 |
| 24 | 8 | 解压后 payload 长度 |
| 32 | 32 | payload 的 SHA-256；用于损坏检测，不是签名/来源认证 |
| 64 | 可变 | 恰好一个标准 Zstandard frame，禁止尾随数据/第二帧 |

payload 顺序为：u32 文件数；每项 u16 路径 UTF-8 字节长、路径字节、u32 块索引；u32 块数；每块 u64 字节长与原始字节。文件按路径严格递增，路径唯一且大小写不碰撞；块顺序为文件遍历时首次出现次序，同内容去重。所有块必须被引用，目录必须含 story.xml；不接受指向块外部的偏移或孤立尾部数据。

限制：10000 文件、路径最多 1024 字节、单文件 128MiB、压缩与展开各不超过 512MiB、逻辑文件总字节不超过 512MiB。路径必须为正常相对路径，拒绝反斜杠、盘符、绝对路径、空段、./../、尾随点/空格、控制字符及 Windows 特殊路径字符。工程遍历拒绝符号链接。解压长度有硬上限，长度/版本/摘要/索引/截断错误直接失败，不能靠伪造头无限分配。

### 20.5 D3D11 与 CPU fallback

Windows 主窗口现在使用原生 D3D11 swapchain 绘制 egui 生成的网格，正常运行不使用 wgpu/D3D12。转场、位移/缩放/旋转/翻转/闪白等由 CPU 计算呈现时间和网格参数，GPU 执行光栅与混合；静态图片滤镜由 HLSL 生成缓存，Live2D 的蒙版/顶点贴图采样/颜色/混合由同一设备完成。字体/静态图片只在准备或变更时上传，动态 GPU 结果直接注册原生纹理。

| --renderer | 行为 |
| --- | --- |
| auto（默认） | 硬件 D3D11 优先；无法建立硬件设备时用 WARP 窗口 + CPU 滤镜/Live2D |
| d3d11 | 必须成功建立硬件设备，否则明确失败；运行中像素后端错误仍保护性回退 CPU |
| cpu | 保留旧 CPU 滤镜/模型算法；窗口仍由 D3D11 呈现，硬件不可用则 WARP |

运行时像素操作失败会保留 CPU 实现作为回退并报告原因；窗口设备/Present 失败尝试一次 WARP 重建，恢复缓存纹理和当前模型显示，不修改剧情/历史/存档。WARP 再失败则明确退出，避免无限重试。未声称已在所有显卡/驱动上复现设备丢失。

GPU Live2D 最长边 1024、最高约 60Hz；CPU 最长边 512、约 30Hz。普通/加法/乘法、正反蒙版、隐藏蒙版源、乘色/屏幕色均保留；CPU/GPU 会有采样、精度与模糊核差异，不保证逐像素相同。Cubism Core、图像/视频解码和必要的像素格式转换仍属于 CPU，不能将 GPU 渲染说成 GPU moc3 求解或硬件视频解码。

Rust 扩展入口为 `render::PixelBackend`（filter/model/clear）、`ModelDraw`、`Image::Cpu/Gpu` 和宿主纹理生命周期；领域层无 D3D11 对象。`ModelDraw::model` 是该模型的稳定身份，新后端缓存贴图必须按 `(model, 贴图序号)` 索引，不得用贴图内存地址或裸指针作键（地址会在模型重载后被复用）。Linux/Vulkan 与 macOS/Metal 仅预留这些接口，本轮未实现相应原生像素后端或跨平台视频。开发/验收入口见 [v0.7 策略](v0.7-implementation-plan.zh-cn.md) 和 [验收记录](v0.7-validation.zh-cn.md)。

## 21. 多文件剧本与打包合并

当前 **story schema 8**。单文件和多文件都固定从 story.xml 开始；每份剧本必须显式声明下一份或文件链结束。文件链在加载阶段一次装配成完整故事，不在玩家跨章节时临时读取。旧 schema 不进入这一装配流程。

### 21.1 入口与章节

| 项目 | story.xml 入口 | 后续章节 XML |
| --- | --- | --- |
| 根元素 | story | story |
| 根属性 | schema="8"、id、content-version、entry，全部必填 | 仅 schema="8"，禁止重复工程身份/入口属性 |
| interface | 恰好一项，引用全局 UI XML | 禁止 |
| scripts/assets/effects/variables/characters/visuals/frames | 可分散声明 | 可分散声明 |
| next | 恰好一项 | 恰好一项 |

每份文件的同类分组至多一组。全链合并后要求一组 scripts/assets/characters/visuals/frames；没有人物或视觉预设时可在入口写空 characters/visuals。variables/effects 可省略。根和分组不允许未知属性/元素、命名空间或多余普通文本，具体子节点沿用前文规则。

同类 ID 在整条链内唯一，例如所有 frame ID 不得重复；资源、人物、视觉与 frame 仍分别使用各自命名空间。脚本清单路径也不能重复，合计最多 32 项。JS 按链顺序与每个 scripts 内的顺序执行，所有文件共享已登记的全局故事；没有按章节隔离的变量、场景或资源 ID。

### 21.2 next 精确格式

```xml
<next src="chapters/chapter-02.xml"/>
<!-- 最后一份必须写： -->
<next end="true"/>
```

| 属性 | 规则 |
| --- | --- |
| src | 非空、游戏根相对的 .xml 路径，扩展名必须小写；最多 1024 UTF-8 字节；禁止与 end 同时出现 |
| end | 只能为字面字符串 true；禁止 src；false/TRUE/1 均不接受 |

next 不能带子元素、普通文本或未知属性。缺失、重复、src/end 同时出现和两个属性都缺失都会报错；末尾文件也不能省略 next。即使当前文件位于 chapters 子目录，继续到另一章节仍写 `chapters/chapter-02.xml`，不按当前 XML 所在目录相对定位。

最多 128 份剧本，源 XML 累计及合并结果分别最多 16MiB。缺文件、非法 UTF-8/XML、链循环、大小写重复引用或混合 schema 均拒绝。只有 next 可达文件参与装配，文件夹名称和文件名排序不决定剧情或装配顺序。

next 是文件装配关系，不是 frame 的默认 Advance：跨文件仍通过 `frame(...).advance/choice` 返回 frame ID；文件链 end="true" 也不能代替 `frame(...).end`。

### 21.3 三文件协作示例

以下示例使用第 3 节的字体/图片、ui.xml 和 scripts/ui.js。将剧情 JS 替换为本节最后的脚本；三个 XML 文件均须保存为 UTF-8。

story.xml：

```xml
<story schema="8" id="chapter-example" content-version="1" entry="opening">
  <interface src="ui.xml"/>
  <scripts>
    <script src="scripts/story.js"/>
    <script src="scripts/ui.js"/>
  </scripts>
  <assets>
    <font id="dialogue" src="assets/fonts/chinese.otf"/>
    <image id="bg.room" src="assets/bg/room.png"/>
  </assets>
  <characters/>
  <visuals><visual id="room" background="bg.room"/></visuals>
  <frames>
    <frame id="opening" visual="room"><text>入口：接下来进入第一章。</text></frame>
  </frames>
  <next src="chapters/first.xml"/>
</story>
```

chapters/first.xml：

```xml
<story schema="8">
  <frames>
    <frame id="chapter.first" visual="room"><text>第一位作者编写的场景。</text></frame>
  </frames>
  <next src="chapters/ending.xml"/>
</story>
```

chapters/ending.xml：

```xml
<story schema="8">
  <frames>
    <frame id="chapter.ending" visual="room"><text>第二位作者编写的结尾。</text></frame>
  </frames>
  <next end="true"/>
</story>
```

scripts/story.js：

```javascript
frame("opening").advance(() => "chapter.first");
frame("chapter.first").advance(() => "chapter.ending");
frame("chapter.ending").end();
```

全链先收集分组再解析引用，因此 frame 可使用后续文件的视觉/资源/人物；跨章条件和更新仍执行原有原子校验。多人协作应约定稳定 ID 命名空间（例如 chapter.first.*），不要把 XML 文件序号当场景 ID。

### 21.4 工程热更新与诊断

编辑任意章节或 next 都触发完整候选构建；校验失败保留旧 Session、音频和画面。语法、同类重复 ID、错误引用和属性诊断尽量给出源文件行列，例如 `chapters/first.xml:4:5`，不要求作者去查看内部合并文本。发布后的诊断针对包内 story.xml 坐标，因为包内不保留这些源章节。

章节拆分、挪动且保持 ID/变量/路由语义时，无需仅因文件组织改变提升 content-version；修改不兼容剧情契约仍必须提升。demo 现在为五文件链，content-version=9；存档重放与文件名无关。

### 21.5 打包结果

kakera-pack 验证源链后，在只读快照中将合并文档替换为 story.xml，移除链上其余源剧本 XML，再编码/自检并独立加载包内故事。源工程磁盘文件不改写。包内合并后的根仍是 story schema 8，只有一个 `<next end="true"/>`；不依赖原章节。

UI XML 不并入 story.xml：其根级 include 通过独立装配合并到原 UI 入口，并移除实际子 UI 文件（第 7.4 节）。JS、模型 JSON 与素材保留原逻辑路径。链外普通 XML 只有作为依赖或显式 include 才收集，不会通过“删除全部 XML”的方式误删 UI。文件链元数据不改变 KPAK 1/2 头或存档 schema；合并、校验或写入失败仍保留旧输出包。

更多命令与部署方式见 [工程/发布与打包指南](project-release-pack-guide.zh-cn.md)，当前实现与验证见 [落地策略](v0.8.3-implementation-plan.zh-cn.md) 和 [验收记录](v0.8.3-validation.zh-cn.md)。


## 22. v0.7.2 背景输入、HUD 与截图存档

本节新增能力使用 story 8 / UI 3 / save 9；容器为 KPAK 1/2。save 9 必须包含 screenshot 字段，JSON null 表示无窗口工具快照；不接受缺字段旧档。

### 22.1 screen 背景事件

```xml
<screen id="hud" modal="false" pause="false"
        background-click="background-advance"
        background-right-click="toggle-hud">
  <scene id="scene" width="1280" height="720" />
  <panel id="hudControls" width="1280" height="720"
         color="#00000000" visible="ui.hudVisible">
    <!-- 作者原有的对话、按钮、选项等放在此 panel 中 -->
    <label id="line" x="64" y="555" width="1146" height="78"
           bind="game.text" font-size="26" />
  </panel>
</screen>
```

在 ui 的 variables 内声明 `<bool id="hudVisible" initial="true" />`，在已登记的 JS 文件中注册：

```javascript
ui("hud").bind("background-advance", ctx => {
  if (!ctx.ui.hudVisible) {
    ctx.setUi("hudVisible", true);
    return; // 这一下仅恢复 HUD
  }
  if (ctx.game.canAdvance) ctx.advance();
});
ui("hud").bind("toggle-hud", ctx => {
  ctx.setUi("hudVisible", !ctx.ui.hudVisible);
});
```

两属性可独立省略，值为绑定到该页面 ID 的事件名；未注册名称、保留的 game.enter 均报错。事件 item/value 均为 null。不增加鼠标坐标、原始输入、路径或 IO 接口，也不在 Rust Controller 解释 HUD。

只对栈顶页面自己的属性派发，菜单不会继承 hud 的背景事件。舞台按 UI 宽高等比布局，黑边不属于背景。按钮、滑条、开关、整个 repeat 滚动区（含空白）优先阻挡；禁用控件仍阻挡，隐藏控件不阻挡。scene、image、label、普通 panel 没有独立点击回调，因此这些区域可视作背景；需要阻挡某块区域时可使用按钮/明确的页面布局，不存在 CSS pointer-events 或 DOM 冒泡。

按下与释放都必须在可交互背景；拖动、从控件移出释放、失焦/提示期间输入、工程浮窗上的点击不推进。一帧最多一个事件；控件事件优先，同时松开左/右键时右键背景事件优先。右键按钮没有默认事件，不会落入 HUD 的右键回调。

ctx.advance 继续使用原仲裁：活动转场先跳过、逐字未完成先补全，再次才推进；game.canAdvance=false 的完成选项/结尾不会替作者选择分支。HUD 隐藏不暂停文字/动画/自动播放，scene 必须放在隐藏 panel 外。HUD bool/页面栈不入剧情档，读档不会自动重置已有 UI 变量；若要强制显示可在作者读档回调中调用 ctx.setUi。空格继续沿用宿主原规则，HUD 隐藏也不改变空格行为。

### 22.2 时间与截图保存

`ctx.save(slot, overwrite?)` 和原自动保存设置无需新增参数。Windows 原生窗口手动存档必须取得当前呈现版本的截图，随后将 Session、时间与截图作为一个 JSON 原子写入；捕获/编码/写入失败不会提交同批 UI 状态或覆盖旧档。无窗口 SaveStore API 为测试/工具仍可生成无截图档。

自动保存先提交剧情，等当前版本的真实画面出现后再写。等待期间短暂阻挡新剧情输入，防止连续推进把旧截图配给新场景；自循环也使用新的呈现版本。自动截图或写入失败保留已提交剧情、旧文件并提示/停用 autosave，读档不会触发写回。失败后若要持久恢复自动保存设置，可在设置界面重新开启。

捕获范围为客户区，包含背景、立绘、台词及 HUD，不包含桌面/系统标题栏。手动保存优先选同场景打开菜单前的最近游戏画面；若没有该缓存，则使用同版本已呈现的窗口画面。自定义根页会作为游戏画面，若作者将保存菜单设计为根页而非模态页，截图可能包含该页。截图记录当时可见状态，自动保存可能捕获转场首帧；不伪造“转场完成后的最终图”。

Windows D3D11、CPU 像素模式及 WARP 窗口共享捕获入口；正常帧仅 GPU 拷贝，真正保存才回读/编码。宿主保留最近窗口与最近无菜单游戏画面，最多两份、每份 64MiB；场景版本不匹配拒绝使用旧图，设备重建清理缓存。其他宿主未提供捕获能力时，GUI 保存明确失败，自动保存提示并停用。

JSON 当前字段示意（其余字段见原 save 9 示例）：

```json
{
  "saved_at_unix_ms": 1792886400000,
  "screenshot": {
    "format": "png",
    "width": 320,
    "height": 180,
    "png_base64": "此处为完整的标准 Base64 PNG，示意文本不可直接加载"
  }
}
```

saved_at_unix_ms 是宿主生成的 Unix 毫秒时间，手动档在捕获时生成，自动档在剧情提交时生成并随后等待呈现。显示按查看时的系统本地时区，格式 `YYYY-MM-DD HH:mm:ss`；不可表示的值显示“无效时间”。这不是 ctx 的时钟 API。

截图等比 contain 到 320×180，黑边填充，不是全尺寸导出；PNG 最多 256KiB，Base64 使用标准字母表/填充。加载严格校验声明/实际尺寸、Base64、PNG 结构与解码预算（1MiB），仅接受宿主生成的 IHDR/IDAT/IEND 块，不接受额外元数据/尾随数据；损坏截图使槽位不可读。JSON 总大小最多 2MiB，截图无独立 sidecar，原子写入方式不变。不要手工用其他 PNG 替换字段绕过该契约。

缩略图只记录展示，不参与历史重放，也不恢复逐字时钟、音视频/Live2D 动画进度。载入仍按原有规则完整恢复剧情并显示最终姿态；视频从零开始。

### 22.3 slots / autosaves 的新字段与图片绑定

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `item.savedAt` | string | 有效槽的本地日期时间，空/坏/不兼容槽为空 |
| `item.hasScreenshot` | bool | 槽有通过校验的截图；screenshot=null 时为 false |
| `item.image` | string | 宿主缓存的预览纹理 ID，无图为空；不作磁盘路径 |
| `item.excerpt` | string | 台词预览整理成单行，最多 64 字符后加省略号；空/坏槽为提示 |

原 item.preview/title/slot/exists/loadable 保持语义，但 preview/excerpt 只含台词与状态文本，不再拼入内部 frame ID。手动 slots 只扫描当前十槽页；autosaves 按 generation 从新到旧最多十条。图像在刷新菜单资料/换页/保存时解码上传，换页释放旧预览，不在绘制或逐字 tick 读盘。外部修改用户档不会即时监控，需重新打开菜单/切页刷新。

```xml
<!-- 放在 source="slots" 或 source="autosaves" 的 repeat 模板内 -->
<image id="slotImage" x="16" y="48" width="128" height="72"
       image-bind="item.image" fit="contain" visible="item.hasScreenshot" />
<label id="slotNoImage" x="20" y="70" width="120" height="24"
       text="无截图" visible="not:item.hasScreenshot" />
<label id="slotTime" x="160" y="42" width="300" height="24"
       bind="item.savedAt" font-size="15" />
<label id="slotSummary" x="160" y="70" width="300" height="62"
       bind="item.excerpt" font-size="15" />
```

image-bind=item.image 仅允许 gallery/slots/autosaves 的图片模板；其他 repeat 不接受此字段。运行时预览 ID 使用宿主保留前缀，不在 assets 注册、不写进 KPAK、不接受脚本任意生成资源路径。模板 ID 必须与其他节点全局唯一。完整保存、读取和自动档布局见 demo ui.xml，背景/HUD 回调见 scripts/ui.js。

本节 PNG Base64 片段仅为结构示意；示例旧版检查记录与设计依据见 [v0.7.2 落地策略](v0.7.2-implementation-plan.zh-cn.md) 和 [验收记录](v0.7.2-validation.zh-cn.md)。

## 23. v0.7.3 按住 Ctrl 快进与已读进度

维护修复（2026-10-25）：首次复验发现的 Ctrl/自动播放仲裁、pause=false 模态页穿透与已读条数上限问题已修复，并加入 App 更新层与真实文件回归，见 [维护修复记录](v0.7.3-maintenance.zh-cn.md)。GUI 与音频等验证边界分别列出。

Ctrl 快进不需要专用 XML 节点或 JS API，使用设置项 `skip_all`、独立用户文件 `read.json`，以及槽位预览不再包含 frame ID（见第 9.3、22.3 节）。

### 23.1 快进行为

按住 `Ctrl` 即快进，宿主每约 40ms 执行一步，每步至多执行下列之一，顺序与“继续”键一致：

1. 正在转场时跳过转场；
2. 台词未显示完整时补全当前句；
3. 文字完整且路由允许推进时提交一次 Advance，进入下一场景。

停止与暂停：

- 选项页、结束页不推进，也不会替玩家选择分支；全部选项隐藏时同样停住。
- 视频播放期间不快进；跳过视频仍需按钮或继续键。若 Ctrl 仍按住，视频自然结束后继续快进。
- 菜单（包括 modal=true、pause=false）、宿主提示、窗口失焦、自动保存等待呈现期间暂停；恢复后重新计时，不补算暂停时间。pause=false 允许底层动画继续，不允许 Ctrl 穿透页面栈。
- Ctrl 是显式输入，优先于自动播放；松开 Ctrl 后自动播放若仍开启，会重新按 `auto_delay` 等待。
- 与普通输入一致，每次更新最多提交一个剧情操作；不改变历史、变量与 `frame(...)`/`ui(...)` 回调语义。

### 23.2 只快进已读与全部快进

`settings.skip_all` 决定快进范围：

| 值 | 行为 |
| --- | --- |
| `false`（默认） | 只有当前场景已读时才从它推进；进入未读场景后立即停下，让该句正常逐字显示 |
| `true` | 不检查已读，连续快进，直到选项、结尾、视频或玩家松开 Ctrl |

- 通过 `ctx.setting("skip_all", boolean)` 修改，也可用 `settings.skip_all` 绑定在 XML 上；demo XML 设置页提供该开关。当前 settings.json 必须包含全部字段；缺字段文件提示后使用完整默认值，不做补齐迁移。
- 只读模式只影响“能否从当前句继续”：已读句仍会被补全并推进；未读句一停下就恢复正常显示与推进，不会自动跳过玩家尚未看过的内容。
- 想快速看完新内容可临时打开全部快进；demo 设置页已有该开关，作者也可按自己的界面接线。

### 23.3 已读判定与 read.json

- 某场景的台词**完整显示过一次**即记为已读；读档后当前句立即完整，视为已读。逐字中、视频遮住对话框时不算。
- 已读集合保存在用户数据目录的独立 `read.json`，不属于任何剧情槽：`{"schema_version":1,"story_id":"…","frames":["opening","question"]}`。读档、重开、改设置都不回退它；删除该文件等价于全部未读。
- 写入采用同目录临时文件与平台原子替换，约 3 秒节流并在退出时补写；失败只记录日志并保留待写标记，不打断剧情。文件损坏或 schema/story_id 不匹配时提示一次并按未读处理；未知 frame ID 保留，不参与故事校验。
- 预算：最多 100000 个 frame ID、单个 ID ≤ 256 字节，读取复用 JSON 2MiB 上限。达到条数上限后保留原集合（包括未知 ID），忽略新的已读标记；写入前再次验证条数/ID，JSON 超预算或原子替换失败保留旧文件和待写标记，不回写自身无法读取的文件。`read.json` 被工程监控与 kakera-pack 排除，不进入发布包。
- 该文件只服务 Ctrl 快进；不提供 JS 查询已读集合、逐句已读回调或逐槽记录。

### 23.4 槽位预览去 frame ID

存档/读档页（`repeat source="slots"` / `"autosaves"`）的 `item.preview` 与 `item.excerpt` 现在只含台词与状态文本；内部 frame ID 仍保存在槽位 JSON 并参与加载校验，但不再出现在界面或数据集中。截图、时间与身份校验沿用当前 save 9；不接受旧版本槽位。

## 24. v0.8 固定比例窗口与全屏

Windows 窗口客户区固定为 16:9，默认 1280×720、最小 640×360（逻辑尺寸）。拖动任意边/角时，宿主扣除标题栏和边框后约束客户区；物理像素按 16×9 的整数单位调整，避免窗口缩放造成可见黑边。最大化按钮关闭，系统或程序改变窗口尺寸时也维护比例。

全屏开关属于独立 Settings，不属于剧情槽。界面与交互使用 XML/JS：

```xml
<toggle id="fullscreen" width="600" height="38"
        text="全屏显示" value="settings.fullscreen" event="fullscreen"/>
```

```javascript
ui("fullscreen").bind("fullscreen", ctx => ctx.setting("fullscreen", ctx.value));
```

事件名自定并须注册；完整样例在 demo 设置页。fullscreen 只接受 bool，默认 false，和同批设置一起验证、原子保存成功后由宿主在更新边界切换。坏命令或设置写入失败不切换窗口。启动读取完整设置后恢复模式。

全屏为当前显示器无边框模式，退出恢复本次进入前的客户区大小和窗口位置。非 16:9 显示器全屏仍等比显示，可能有留边；本版不裁切 UI、不拉伸画面、不切换显示器分辨率。窗口位置只在本进程内恢复，不写入剧情存档；设置里的 fullscreen 会跨进程保存。

新增页面/布局/现有功能组合优先只改 XML/JS。窗口比例和原生全屏是本版确需宿主扩展的能力，Rust 不接收脚本窗口句柄、任意系统命令或显示器路径。


## 25. v0.8.1 KPAK 2 与创作边界

资源引用、XML/JS API、story 8/UI 3/save 9/content-version=9 为当前基线。文件流/分块解压属于 XML/JS 不具备的宿主能力，本版扩展 Rust 内容边界；游戏作者无需更改剧情或 UI 脚本。

默认 kakera-pack 生成 KPAK 2，--format 1 可生成兼容小包；播放器自动识别 1/2，未知版本明确失败。该兼容不恢复旧剧情/界面/存档。设计和实际结果见 [策略](v0.8.1-implementation-plan.zh-cn.md) 与 [验收](v0.8.1-validation.zh-cn.md)，二进制细节和流式/内存边界见 [KPAK 规范](kpak-format.zh-cn.md)。


## 26. v0.8.2 点九图与图集区域

### 26.1 专用 nine-patch 资源与节点

```xml
<!-- story.xml 的 assets 中；不写成普通 image 资源 -->
<nine-patch id="ui.bubble" src="assets/ui/bubble.9.png"/>
```

```xml
<!-- ui.xml 的 screen/panel/nine-patch 内；坐标为 UI 设计单位 -->
<nine-patch id="dialogueBackground" x="36" y="505" width="1208" height="180"
            image="ui.bubble" density="1">
  <label id="dialogueText" width="1000" height="76" bind="game.text" font-size="26"/>
</nine-patch>
```

资源与 UI 都用专用 nine-patch 标签，不能用普通 image/panel 的图片引用代替。src 必须以 `.9.png` 结尾，读取的是 Android Studio 保存的源 PNG，一像素标记边框仍在；APK 中已经编译/去边框的 NinePatch 二进制不在支持范围。

格式依据：[Android 官方 NinePatch 概述](https://developer.android.com/develop/ui/views/graphics/drawables#nine-patch)、[Draw 9-patch 编辑器](https://developer.android.com/studio/write/draw9patch)。上/左边框定义伸缩段，下/右定义内容区。本实现的具体约束：

- 原 PNG 至少 3×3，含边框单边仍受 8192 像素预算。标记是完全不透明黑色；其余边框像素为透明或不透明白色，角点不能黑色。拒绝抗锯齿灰色/半透明黑色标记。
- 上和左每轴必须有 1–8 段伸缩区，支持多个不相邻段。额外空间按原伸缩段长度比例分配；固定段/角保持设计尺寸。目标小于固定段总尺寸时压缩固定段、将伸缩段降为零，矩形不会反转。
- 下和右每轴至多一段内容标记；未标记时使用该轴伸缩区外包范围。加载解析后去掉标记边框，GPU 只上传内容图一次；标记线不进入绘制。
- 子节点原点在当前大小下映射后的内容区左上角，且裁剪于内容区。下/右内容标记控制 padding；子节点的 x/y 在该原点基础上计算，不需要手动加上图片标记的一像素边框。
- density 为“源像素/设计单位”，默认 1，范围 0.125–8。2 倍素材可写 density="2"；固定段和内容 padding 按它折算，再随整个 UI 的窗口/DPI 比例缩放。字体和子节点设计单位不受 density 改写。
- 可用通用 id/x/y/width/height/color/visible/enabled，加 image/density；不接受 region、fit、image-bind、radius 或 effect。当前 ctx.animate 不接受 nine-patch 目标；可在父级组合已有页面/可见性效果，透明度使用 color 的 alpha。

同一资源可用于多个大小的背景，复用同一纹理。每个节点最多 17×17 个源分段组合（每轴至多 8 个伸缩段），只生成网格，不产生中间大位图。demo 对话框将原 1208×180 纹理改为 64×32 内容纹理，固定边框在不同宽度下保持形状；演示入口：效果演示 → 点九 / 图集。

### 26.2 图集 region 与 JS 绑定

图集继续用普通 image 资源；只注册整张图一次：

```xml
<!-- story assets -->
<image id="ui.atlas" src="assets/ui/sprites.png"/>
```

```xml
<!-- ui variables -->
<string id="spriteRegion" initial="0,0,64,64"/>
<!-- screen 内 -->
<image id="statusIcon" x="20" y="20" width="96" height="96"
       image="ui.atlas" region-bind="ui.spriteRegion" fit="contain"/>
<image id="fixedIcon" x="140" y="20" width="96" height="96"
       image="ui.atlas" region="64,0,64,64"/>
```

```javascript
ui("showGreen").bind("show-green", ctx => ctx.setUi("spriteRegion", "64,0,64,64"));
ui("showRed").bind("show-red", ctx => ctx.setUi("spriteRegion", "128,0,64,64"));
```

region 是四个逗号分隔的整数 `x,y,width,height`，允许数字旁空格；原点为原始图像左上角，单位是源像素。x/y 非负、宽高为正，不允许小数/负数/溢出/空值。区域必须完全在当前源图内，字符串最多 128 字节。region-bind 必须指向 UI string 变量，与 region 互斥，不能绑定 setting、剧情变量或 item 字段。

普通 image、带图片的 panel 和 button 皮肤均接受 region/region-bind。没有图片时不能声明区域；image-bind="ui.xxx" 可与区域组合，更换图片时一并验证新图尺寸。button 的同一区域也用于 hover-image/disabled-image，三个资源都要容得下该区域。不支持 item.image（画廊/槽位运行时预览）与区域组合；repeat 中显式 image="ui.atlas" 的模板可用，UI 区域变量改变会作用所有该模板实例。

ctx.setUi 只修改 UI 草稿，提交前验证最终区域、当前图片和皮肤尺寸；坏区域或错误类型让整批命令失败，包含同批的剧情、保存或设置操作都不执行，旧 UI/Session 保留。已解码尺寸与区域计划均被缓存；绘制不解析区域字符串、不执行 JS、不切图/解码/重新上传纹理。

contain/cover 按所选区域的宽高比计算，stretch 拉伸所选区域；现有 opacity、镜像、旋转及图片动画围绕该区域工作。采样 UV 收进区域最外层像素中心，减少线性过滤相邻格串色；1 像素区域采样其中心。滤镜仍对整张图集缓存一次，因此跨像素模糊可能混入相邻格，素材需留间隔或不用该组合。此能力不自动拼接散图或做纹理压缩。

## 27. v0.8.2 缺省场景 ID 与作者别名

用户已确认采用稳定作者别名，避免让脚本预先知道随机 UUID。示例：

```xml
<story schema="8" id="named-demo" content-version="1" entry="opening">
  <!-- 其余 scripts/assets/interface/characters/visuals 等按当前契约补齐 -->
  <frames>
    <frame name="opening"><text>第一句。</text></frame>
    <frame name="ending"><text>结束。</text></frame>
  </frames>
  <next end="true"/>
</story>
```

```javascript
frame("opening").advance(ctx => {
  // ctx.frameId === "opening"，工程和发布一致
  return "ending";
});
frame("ending").end();
```

id 缺省时 name 必填；两者都缺少会给出源文件位置错误。显式 id="" 仍错误。name 非空且不含空白，全故事唯一，不能与其他场景 ID/name 冲突。也可同时显式写 id="固定契约值" name="作者名"；入口 entry、`frame(...).advance/choice/end` 注册及返回目标可使用名字或实际 ID，但同一场景不能通过两个名字重复注册。

工程运行/--check 对缺省 id 临时使用稳定 name，热更不会每次随机化。KPAK 1/2 打包仅在候选合并 XML 中为每个缺省 id 的 frame 插入随机 UUID v4，保留 name；显式 ID 不变。候选会重新加载，验证 XML/JS 注册、选项契约及资源，成功后原子写包；动态目标继续在执行边界校验，不承诺穷举所有分支。源 XML/JS 不改写，没有自动顺序推进或默认分支。

在已打包内容中，Model/Session/save.frame_id/历史/read.json 保存实际 UUID 或显式 ID，控制器及历史重放按它校验。剧情 ctx.frameId 与 UI game.frameId 有 name 时暴露名字，否则暴露实际 ID；history.id 的可读标签也优先名字。真实存档不按名字猜测或迁移。

同一个包重复启动、读档、重放的 UUID 固定；再次从无 id 的源 XML 打包会产生新 UUID，因此这类包不保证逐字节确定性或跨构建槽位兼容。工程临时 name 档与发布 UUID 档也不同。内容作者必须在重新生成随机 ID 导致契约变化时提升 content-version；需要跨构建稳定存档/已读记录时，显式固定 id。没有缺省 id 的工程继续按原确定性打包规则工作。

## 28. 可配置窗口标题

2026-10-26 追加，应用版本仍为 v0.8.2，数据 schema 不变。窗口标题属于 XML UI 的宿主属性，既有 ctx.setUi 即可修改，不增加窗口句柄或 IO 权限。

静态标题直接写在 ui.xml 根节点：

```xml
<ui schema="3" initial="hud" title="我的游戏">
  <screen id="hud" modal="false" pause="false"/>
</ui>
```

需要 JS 修改时，绑定 UI string 变量：

```xml
<ui schema="3" initial="hud" title-bind="ui.windowTitle">
  <variables>
    <string id="windowTitle" initial="我的游戏"/>
  </variables>
  <screen id="hud" modal="false" pause="false">
    <button id="renameTitle" width="240" height="40"
            text="修改标题" event="rename-title"/>
  </screen>
</ui>
```

```javascript
ui("renameTitle").bind("rename-title", ctx => ctx.setUi("windowTitle", "我的游戏 · 特别篇"));
// 也可在已有 game.enter 回调内按进入的场景设置；请合并进原回调，勿重复注册。
ui("$game").bind("game.enter", ctx => ctx.setUi("windowTitle", `我的游戏 · ${ctx.game.frameId}`));
```

- title/title-bind 互斥；二者都不写时默认 Kakera AVG，不再附加渲染后端后缀。title-bind 只接受已声明的 UI string，不接受 game/settings/item 绑定或 JS 表达式。
- 初值和提交后的最终标题必须为 1–256 个 Unicode 字符，不能全为空白或包含控制字符（换行、制表符、NUL 等）。XML 中 &、引号等使用正常实体转义；JS 使用普通字符串。长度按 Unicode 标量值计算，不按 UTF-8 字节计算。
- 与同批 UI 命令一起验证。非法类型、过长/空标题、其他命令失败、剧情/媒体验证失败或 IO 失败均不提交新标题；整批中间值被有效最终值替换时，以最终值为准。宿主只读取已提交的缓存值，在更新边界更改原生标题，Windows 仅值改变时调用窗口 API。
- 标题不写入剧情槽或 settings。退出后重启使用 XML 初值，再执行 game.enter；热更按既有 UI 重建规则处理。读档/重开沿用当前 UI 变量，随后执行 game.enter；若需要随场景恢复标题，在该回调里赋值。历史验证重放不修改窗口。
- 全屏时系统标题栏可能隐藏，标题仍更新；退出全屏后显示当前值。工程与 KPAK 1/2 发布使用同一配置；坏热更保留旧标题。

demo 启动/进入场景后显示“午后物语 · 场景名”；效果演示 → 点九 / 图集 → 切换窗口标题，可在两个中文标题间切换。实现及本轮验证见 [策略](window-title-implementation-plan.zh-cn.md) 与 [验收](window-title-validation.zh-cn.md)。


## 29. v0.8.3 编辑器调试与发布清单

story 8/UI 3/save 9、KPAK 1/2 均不变。本节是宿主/工具能力：现有 XML/JS 无法安全提供进程通信和发布资源选择，因此扩展 Rust；游戏 JS 仍无网络/路径/随机/时钟或任意原生能力。

### 29.1 调试启动与运行状态

```powershell
([guid]::NewGuid().ToString('N') + [guid]::NewGuid().ToString('N')) |
  Set-Content -Encoding ascii target/debug-token.txt
./target/debug/kakera-avg.exe --project games/demo --save-dir target/debug-saves `
  --debug-listen 127.0.0.1:19200 --debug-token-file target/debug-token.txt --no-audio
```

四个参数须显式成组提供，只允许 loopback；--package/--check 拒绝调试。令牌 32–128 ASCII 字母/数字/-/_，独立文件末尾可换行。初始暂停并关闭本次自动保存，隔离目录用于避免影响玩家数据。远程 IDE 通过安全隧道连接本机端口，播放器没有公网监听/TLS。

协议为认证 TCP JSON Lines v1，支持 status、pause/resume、breakpoints、advance/choose、reveal/skip_transition。控制命令必须带最近 state_revision；ID 全局递增，最近 8 个完全相同字节的请求可复用响应，超时先查询状态。请求/响应与队列有界，未知字段拒绝。状态含当前/候选 frame、变量、可用选项、UI 栈/标题、视频/效果与暂停原因。

场景断点接受 ID/name，全部条件、更新、生命周期和媒体候选验证成功后才在提交前暂停；旧状态保留，advance/resume 只提交候选一次。暂停阻挡剧情输入并冻结呈现/媒体时间，热更替换延后。没有 JS 行级步进、条件断点、eval、改变量、任意跳转或事件订阅。完整协议、限制与重试语义见 [播放器调试 API v1](editor/10-player-debug-api.zh-cn.md)。这不是 DAP，编辑器适配器尚待实现。

### 29.2 发布依赖与附加文件

kakera-pack 两种格式均以真实 Bundle 加载所读文件构建依赖闭包，包含剧本链、UI、JS 和所有已声明资源及模型辅助文件，不以当前幕/JS 路由可达性裁剪。默认附带 assets/licenses/、licenses/ 中的普通文件；作者不应将私人内容放在这两个约定目录。

工程根可选 kakera.package.json：

```json
{"schema":1,"include":["extras/credits.txt"]}
```

仅允许 schema/include，schema 固定 1；最大 1MiB/100000 条，路径最大 1024 UTF-8 字节。include 只能是游戏内现存普通文件，不支持目录/通配；重复/大小写冲突、缺失、越界或符号链接拒绝。清单自身不入包，也不能 include 自身。保留既有路径/用户数据规则与容器预算。没有自定义 ignore 参数。

候选只复制所选发布源文件；固定候选重载必须得出相同依赖集合，合并/UUID 不改源工程。输出完整块自检、独立加载、所选字节及清单/许可证列表复查通过后才原子写入。未引用普通文件不自动收集，显式 include 并不意味着宿主会执行附加 DLL/EXE。工程热更完整快照与底层一般容器 API 不受该 CLI 收敛策略替代。

### 29.3 编辑器与 AI 边界

编辑器采用用户配置的 Chat Completions 兼容 AI API，允许自部署/远程服务，模型 ID 不固定。凭据使用秘密存储，远程发送范围需向作者展示；AI 只输出候选 XML/JS 和资源关联，由确定性校验与 diff 接受流程落盘。本版仅交付播放器/打包器宿主支撑，未实现编辑器、AI 客户端或 VS Code DAP 适配器。方案、接口和平台结论见 [编辑器文档](editor/README.zh-cn.md)。


## 30. v0.8.4 按钮 UI 路由

按钮可直接声明 `route="open:settings"` 打开页面，或 `route="close"` 关闭当前顶层页面，无需注册仅用于导航的 JS 回调：

```xml
<button id="settingsButton" width="120" height="40" text="设置" route="open:settings" />
<button id="backButton" width="120" height="40" text="返回" route="close" />
```

route 仅允许 button 使用，与 event 互斥；纯音效按钮可仅写 sound（第 31 节）；空值、未知动作、缺失目标、空白目标或与 event 同时设置均拒绝。目标是 screen ID，不是剧情 frame 或文件路径。跨 include 文件的页面在完整 UI 合并后统一校验，错误保留作者文件行列。

open 沿用页面 modal：true 压栈并暂停（依 pause），false 替换根页面栈。close 只能关闭顶层页，不能关闭唯一根页；重复模态和超过八层仍拒绝，旧 UI 状态保留。隐藏/禁用按钮、下层页面、提示与调试暂停仍遵循原输入阻挡规则，每次更新最多一个意图。

路由产生类型化 UI 事件，经既有 host::prepare 事务验证后提交，不执行 JS，不开放剧情跳转或 IO。需要赋值、确认等复合逻辑时继续使用 event + `ui(id).bind(...)`/ctx.open/close。UI 3/story 8/save 9、KPAK 1/2 不变，两种包保留 route 属性。demo 的设置入口和设置页返回已采用直接路由。完整结果见 [v0.8.4 验收](v0.8.4-validation.zh-cn.md)。


## 31. UI 演出、过场视频与角色音效

UI 演出从 v0.8.4 提供；本手册按 v0.9.0 的 story 8/UI 3/save 9 说明。完整边界见 [落地计划](ui-presentation-implementation-plan.zh-cn.md) 与 [验收](ui-presentation-validation.zh-cn.md)。

### 31.1 视频统一放在 UI

v0.9.0 起，frame 不再接受 video，剧情中途插播也使用 UI sequence。普通视频演示可 finish=return 回到原页或 finish=open:ID 打开目标页；需要剧情/页面互相唤起时，使用第 32 节的生命周期及隐藏交接槽。视频不再自行向 Controller 提交 Advance。

### 31.2 声明式演出

`ui-media` 可通过已有 include 合并，多文件声明统一校验：

```xml
<ui-media>
  <sound id="ui.character">
    <clip asset="voice.lin" actor="lin" weight="2"/>
    <clip asset="se.knock" weight="1"/>
  </sound>
  <sequence id="opening" finish="return" skippable="true">
    <fade target="welcomePanel" from="0" to="1" duration="800"/>
    <sound ref="ui.character"/>
    <video asset="video.intro"/>
    <fade target="welcomePanel" from="0" to="1" duration="800"/>
  </sequence>
</ui-media>
<screen id="welcome" sequence="opening">
  <panel id="welcomePanel" width="1280" height="720" color="#182638">
    <label id="welcomeText" width="800" height="60" text="欢迎"/>
  </panel>
</screen>
```

顶层页面首次进入自动启动 screen.sequence；同页重绘、finish=return 和剧情自循环不反复触发。JS 的 `ctx.sequence("opening")` 显式启动一次，不能重入。fade 对 UI 控件/容器应用继承透明度，包含文字、按钮与子控件；目标必须为非 repeat 模板节点的全局 ID。from/to 为有限 0–1（默认 0/1），duration 为 1–10000 毫秒（默认 400），采用 smoothstep。步骤顺序执行，完成保留最终透明度，离开页面或剧情切换时清除；不支持并行轨道、循环、任意 JS 计时回调或动画完成 JS 回调。

演出期间底层输入、自动播放、快进与保存受阻。skippable 默认为 true，点击主鼠标按钮或空格立即完成整个序列、设置最终透明度并执行结束路由一次；未执行的声音/视频不补播，该输入不穿透到剧情。false 时不能跳过。UI 时钟独立于菜单的剧情暂停，失焦/提示/调试暂停冻结。已有自动档等待呈现完成后才可启动，显式提前启动报错；自动页面触发会等待。热更候选延后到演出结束，成功替换不重播当前页自动演出；设备重建取消演出。运行失败取消并提示，保留当前剧情和未消耗的隐藏槽，不执行结束路由。

每份 UI 最多 64 个 sound 组、每组 64 候选、64 个 sequence、每个 64 步，演出总长度（含视频）不超过 600 秒。所有视频/音频仍通过 ContentSource 及原解码预算准备。时间、随机结果、播放指针和淡入进度不入 Model、存档或 UI 变量。

### 31.3 单一音效、随机组与排除

```xml
<button id="tap" width="180" height="44" text="敲门" sound="se.knock"/>
<button id="voice" width="180" height="44" text="随机台词" sound="ui.character"/>
<button id="backWithSound" width="180" height="44" text="返回" route="close" sound="se.knock"/>
```

sound 属性仅用于 button，可单独使用，也可与 event 或 route 组合；event/route 仍互斥。音效在路由/回调整批验证并提交成功后才播放。JS `ctx.playSound("ui.character")` 或 `ctx.playSound("se.knock")` 同样有效。为避免跨 IO 回滚歧义，声音/演出命令只能与 UI 局部赋值、页面命令及本节音效偏好组合，不能同批混合普通剧情推进、存读档、settings/profile 等操作，也不能在 game.enter 中调用；页面关闭回调的 resume 为独立结束事务，不与新声音/序列同批。

clip.asset 必须是已声明 Audio，actor 可省略，存在时必须是 character ID；同一资产在所有组中的角色归属必须一致。weight 为 1–1000 整数，默认 1。enabled 为作者控制的 bool，默认 true；任何声明中 enabled=false 的资源对整个 UI 音效系统禁用，不能经另一组或直接资源引用绕过。sound/sequence ID 最多 128 字节且不能含空白；sound 组 ID 不得与 Audio 资源 ID 重名，同组不能重复资产。

宿主先按作者禁用、总开关、玩家排除资产、玩家禁用角色过滤，再在余下候选中无偏带权抽取；全被排除时静音，不回退到禁用资源。只有这一呈现层使用随机源，剧情 QuickJS 仍不开放随机/时钟。UI 声音最多 4 路，超出停止最早的一路，遵循 settings.muted/se_volume/失焦；菜单仍可播放。角色禁用/资源排除提交后立即停止相应在播 UI 声音。只控制 UI 声音，不关闭剧情 frame 的 voice/se 或 BGM；若希望角色选择同样影响剧情配音，应另行设计剧情音频策略。

### 31.4 玩家偏好与绑定

```js
ui("roleToggle").bind("role", ctx => ctx.soundActor("lin", ctx.value));
ui("excludeToggle").bind("exclude", ctx => ctx.soundExclude("voice.lin", ctx.value));
ui("uiAudioToggle").bind("ui-audio", ctx => ctx.soundEnabled(ctx.value));
ui("watchButton").bind("watch", ctx => ctx.sequence("opening"));
```

只接受严格 bool 和已声明的角色/Audio ID。偏好保存为存档目录内独立 `ui-audio.json`（schema 1、story_id、enabled、disabled_actors、excluded 完整字段），原子替换，失败不提交本批 UI 状态/声音；坏文件提示并采用默认值，未知旧 ID 保留排除意图但不参与当前选择。不随剧情槽读档/重开回滚，不打包、不触发工程热更。存档仍只保存剧情变量/位置/历史。

只读 bool 绑定：`audio.enabled`、`audio.actor.lin`（true=启用）、`audio.excluded.voice.lin`（true=排除）。角色或资产 ID 中的点按整个 ID 查找，不拆层级；配合 toggle 的 value 与上述事件使用。实际示例见 games/demo/ui/presentation.xml 和 scripts/ui.js。


### 31.5 XML 参数速查

以下标签均使用严格属性白名单；资源引用填写 story.assets 的 ID，不填写文件路径。ui-media 放在 ui 根下，不放在 screen 内；每份源文件最多一组，可通过 include 汇总。

| 标签/属性 | 必填或默认 | 含义 |
| --- | --- | --- |
| `sound.id` | 必填 | 音效组 ID；与 Audio 资源 ID 不重名 |
| `clip.asset` | 必填 | 已声明 Audio 资源 ID；每组 1–64 个 clip，同组资源不重复 |
| `clip.actor` | 可选 | 已声明 character ID；同资源跨组必须保持一致归属，包括是否省略 actor |
| `clip.weight` | 默认 1 | 整数 1–1000；过滤后按剩余权重抽取 |
| `clip.enabled` | 默认 true | 作者是否允许 UI 播放；任何组声明 false 都会禁用此资源的 UI 播放 |
| `sequence.id` | 必填 | 演出 ID，供 screen.sequence / ctx.sequence 使用 |
| `sequence.finish` | 默认 return | return 保留启动时页面栈；close 弹出启动页；open:ID 按目标 modal 属性压栈或替换根页 |
| `sequence.skippable` | 默认 true | 是否允许鼠标主键/空格跳过整个序列 |
| `fade.target` | 必填 | 非 repeat 模板内的 UI 节点 ID；容器透明度传递给子节点 |
| `fade.from` / `fade.to` | 默认 0 / 1 | 有限数值 0–1；不自动读取控件此前透明度 |
| `fade.duration` | 默认 400 | 毫秒，有限数值 1–10000 |
| `video.asset` | 必填 | 已声明 Video 资源 ID；跳过策略来自所属 sequence，无 advance 属性 |
| `sound.ref`（sequence 内） | 必填 | Audio 资源 ID 或已声明音效组 ID，进入此步骤时触发一次 |
| `screen.sequence` | 可选 | 自动演出 ID；页面成为顶层时触发 |
| `button.sound` | 可选 | 点击成功提交后播放的 Audio 资源或音效组 ID |

根 ui-media 中的 `<sound id="...">` 定义组，sequence 中的 `<sound ref="..."/>` 执行播放，两者不能交换属性。一个 clip 就是固定单音效组；不需要角色归属或候选配置时也可直接播放 Audio ID。组外直接引用仍遵守该资源的角色/禁用配置。

例如组内两条音效权重为 2 和 1：两条均可用时概率分别为 2/3 和 1/3；排除第一条后第二条概率为 1；全部排除后静音。解除玩家排除不会覆盖作者的 enabled=false，也不会自动恢复仍被禁用的角色。

### 31.6 在已有工程中接入的完整示例

以下示例可直接加入 games/demo；自建工程需先在 story 中声明 `video.intro`、`video.credits` 两个 Video 资源、`voice.lin`、`se.knock` 两个 Audio 资源及 `lin` 角色。示例沿用已有非模态 `hud` 页面，不重复声明它；节点/组/事件名使用 api 前缀，避免和 demo 原页面重名。

**第一步：** 在现有 UI 入口根中添加 `<include src="ui/api-presentation.xml"/>`，再创建下面的子文件。入口 initial 保持原 hud；通过按钮打开演示页，页面自动淡入。

```xml
<!-- ui/api-presentation.xml -->
<ui schema="3">
  <ui-media>
    <sound id="api.roles">
      <clip asset="voice.lin" actor="lin" weight="2"/>
      <clip asset="se.knock" weight="1"/>
    </sound>
    <sequence id="api.reveal">
      <fade target="apiPanel" from="0" to="1" duration="800"/>
    </sequence>
    <sequence id="api.fade">
      <fade target="apiPanel" from="1" to="0" duration="600"/>
      <sound ref="se.knock"/>
      <fade target="apiPanel" from="0" to="1" duration="600"/>
    </sequence>
    <sequence id="api.intro" finish="return">
      <video asset="video.intro"/>
    </sequence>
    <sequence id="api.credits" finish="open:hud">
      <video asset="video.credits"/>
    </sequence>
  </ui-media>
  <screen id="api-menu" sequence="api.reveal">
    <panel id="apiPanel" x="150" y="90" width="980" height="540" color="#182638">
      <label id="apiTitle" x="30" y="30" width="800" height="44" text="演出与角色音效" font-size="28"/>
      <button id="apiBack" x="840" y="30" width="100" height="44" text="返回" route="close"/>
      <button id="apiFade" x="30" y="120" width="260" height="44" text="淡出与淡入" event="api-fade"/>
      <button id="apiIntro" x="330" y="120" width="260" height="44" text="片头：回到本页" event="api-intro"/>
      <button id="apiCredits" x="630" y="120" width="300" height="44" text="片尾：返回剧情界面" event="api-credits"/>
      <button id="apiSingle" x="30" y="200" width="260" height="44" text="固定敲门声" sound="se.knock"/>
      <button id="apiRandom" x="330" y="200" width="360" height="44" text="随机配音或敲门声" sound="api.roles"/>
      <toggle id="apiMaster" x="30" y="290" width="800" height="38" text="启用 UI 音效" value="audio.enabled" event="api-master"/>
      <toggle id="apiActor" x="30" y="350" width="800" height="38" text="启用凛的 UI 音效" value="audio.actor.lin" event="api-actor"/>
      <toggle id="apiExclude" x="30" y="410" width="800" height="38" text="排除 voice.lin 这一条" value="audio.excluded.voice.lin" event="api-exclude"/>
      <label id="apiHint" x="30" y="480" width="900" height="35" text="演出中点击或按空格跳过，角色偏好在重启后保留。"/>
    </panel>
  </screen>
</ui>
```

**第二步：** 在现有 hud 的可见、可交互容器内增加入口按钮；坐标按项目布局调整，不要放进只在某些剧情中显示的容器。

```xml
<button id="apiOpen" x="260" y="160" width="220" height="44"
        text="打开演出示例" route="open:api-menu"/>
```

**第三步：** 创建 JS 文件，并在现有 story.scripts 分组增加 `<script src="scripts/api-presentation.js"/>`。若把回调合入已加载 JS，则不必增加 script 项；同一控件事件不要重复绑定。

```js
// scripts/api-presentation.js
ui("apiFade").bind("api-fade", ctx => ctx.sequence("api.fade"));
ui("apiIntro").bind("api-intro", ctx => ctx.sequence("api.intro"));
ui("apiCredits").bind("api-credits", ctx => ctx.sequence("api.credits"));
ui("apiMaster").bind("api-master", ctx => ctx.soundEnabled(ctx.value));
ui("apiActor").bind("api-actor", ctx => ctx.soundActor("lin", ctx.value));
ui("apiExclude").bind("api-exclude", ctx => ctx.soundExclude("voice.lin", ctx.value));
```

三个开关的值均由只读绑定恢复，作者不需要再用 UI 变量复制偏好。关闭凛后，随机组只剩敲门声；排除 voice.lin 也会让它无法经直接资源引用播放。演出页返回按钮关闭当前模态页；片尾的 open:hud 目标必须是 modal=false 才会直接回到根剧情界面。finish=return 不会自动退出当前页，也不会推进剧情。

要由同一事件先开页面再播放，可将入口改为 event 并注册 `ctx.open("api-menu"); ctx.sequence("api.intro");`，这会显式启动片头而不再额外启动该页的自动淡入。不要同时保留入口的 route。通常采用示例中的“页面自动淡入、按钮显式播放”更容易组织演出。

### 31.7 排错入口

未知属性时先确认 video 只在 UI sequence 中、skippable 写在 sequence；frame.video、ctx.skipVideo 和 game.videoPlaying/game.videoSkippable 均已移除。淡出最终透明度会保留，需要回到可见界面时补一段 to=1 或路由到其他页。无声时检查 muted/se_volume、UI 总开关、角色、单条排除和作者 enabled。交接与生命周期排错见第 32 节。

## 32. v0.9.0 幕与页面生命周期、隐藏交接槽

本版应用版本 v0.9.0；story 8/UI 3 保留，存档为唯一当前 **save 9**，demo content-version=9，KPAK 1/2 不变。旧 save 8 不迁移、不补字段。旧 frame 视频接口只在归档手册说明，运行时不保留兼容分支。实现依据见 [落地计划](v0.9.0-implementation-plan.zh-cn.md) 与 [验收](v0.9.0-validation.zh-cn.md)。

### 32.1 注册生命周期

```js
frame("opening").enter(ctx => ctx.setUi("status", "进入开场"));
frame("opening").exit(ctx => ctx.open("opening-movie"));
ui("opening-movie").bind("screen.open", ctx => ctx.sequence("opening-sequence"));
ui("opening-movie").bind("screen.close", ctx => ctx.resume("question"));
```

| 注册方法 | 触发时机 | ID 与回调 |
| --- | --- | --- |
| `frame(id).enter(fn)` | 正常开始进入该幕；UI 进入新目标时也触发；恢复已进入的同幕不重放 | id 可用实际 ID 或稳定 name；同一实际幕不可按 ID/name 重复注册同种回调 |
| `frame(id).exit(fn)` | 控制器已验证可推进目标、变量和条件后，当前幕即将离开 | 自循环也触发；补全文字、无有效推进、隐藏选项失败不触发；end 不凭空产生退出 |
| `ui(id).bind("screen.open", fn)` | 页面进入候选栈；根替换、压栈、启动/隐藏槽恢复都适用 | screen 必须存在；只被上层覆盖不算关闭 |
| `ui(id).bind("screen.close", fn)` | 页面从候选栈移除 | 从顶层向下关闭；随后由下向上打开新增页面；仅重绘不触发 |

四种注册可省略，fn 必须是同步函数并返回 undefined；回调按每次独立 QuickJS 隔离执行。普通参数快照沿用普通 UI 回调：ctx.vars 只读，ctx.ui 为事件开始时 UI 变量，ctx.game.frameId 对具名幕给出 name。幕 exit 读取旧幕快照，幕 enter 读取候选幕快照；离幕交给 UI 期间剧情尚未提交，UI 读取保留的旧剧情，待续目标/变量由隐藏槽持有；进入幕触发 UI 时，该幕与 UI 一起提交，UI 读取该幕快照。

新生命周期允许 open/close/setUi/message/playSound/sequence/present；离幕 sequence 需要同批打开 UI 页，推荐只在 frameExit 中打开页面、在 screenOpen 中启动序列；**只有 screenClose 额外允许 resume**。普通 `ui(id).bind(...)` 回调也可 present/resume。生命周期不允许 setting、unlock、存读档、普通 advance/choose/restart、任意变量修改或 IO。原有 game.enter 保持自己的白名单；正常进入时顺序为旧 frameExit → game.enter → 新 frameEnter；exit 已打开交接页时，game.enter/新 frameEnter 延后到续接提交前。读档/隐藏槽恢复及历史校验不重放 frameEnter/Exit；隐藏槽恢复不重放 game.enter，保留隐藏 UI 变量；普通存读档仍沿用既有 game.enter 恢复规则。

页面回调也只生成候选计划，不在绘制时执行。同一事务所有回调合计最多 32 条命令、页面生命周期最多 16 次派发，一次最多启动一个序列、一个返回操作；循环开关、重复模态页、超限、非法目标或回调异常拒绝整个事务，旧剧情/UI/音频不部分提交。UI 返回及进入幕交接同样支持场景断点：断点暂停期间隐藏槽不提前写成已完成，继续成功提交后才更新。XML 节点 event 不可伪造这些保留生命周期事件。

### 32.2 剧情即将结束时插播视频

下面三个部分合入既有工程：story 已有 opening/question 的正常剧情路由，已声明 video.intro 和非模态 hud；在 UI 根加入 ui-media 与 screen，在已加载 JS 中加入回调。若已有同名注册，请合并到原回调而不重复声明。

```xml
<!-- UI 根的 ui-media 内；已有 ui-media 时只添加 sequence -->
<sequence id="opening-sequence" finish="close" skippable="true">
  <video asset="video.intro"/>
</sequence>
```

```xml
<!-- UI 根下；该页是模态页，hud 留在下层 -->
<screen id="opening-movie" pause="true">
  <label id="openingMovieHint" width="800" height="50" text="开场视频"/>
</screen>
```

```js
// 剧情推进仍在剧情 JS 中声明，变量更新会和待续位置一起保存。
frame("opening").advance("question");
frame("opening").exit(ctx => ctx.open("opening-movie"));
ui("opening-movie").bind("screen.open", ctx => ctx.sequence("opening-sequence"));
ui("opening-movie").bind("screen.close", ctx => ctx.resume());
```

流程：用户确认离开 opening → 验证 opening→question 的正常路由和更新 → exit 打开 UI → 原子保存隐藏槽 → screenOpen 自动启动视频 → 视频自然结束或被跳过 → finish=close → screenClose → resume 从隐藏槽续接 question → 执行 game.enter/frameEnter 并提交。确认期间显示旧幕，不提前执行 question 的进入音频；跳过输入被消费，不再推进 question。

`ctx.resume()` 恢复完整待续 Session，因此正常路由中的变量更新只应用一次；不要用普通自动槽/手动槽模拟这个交接。若要指定不同目标，可用 `ctx.resume("other-frame")`；目标按 ID/name 解析，保留待续变量，并在 history 中记录一条显式 UI 返回边，不伪造普通选项或路由。若目标就是待续位置，不重复增加历史边。指定目标不允许给变量赋值或指定路径。

### 32.3 UI 主动进入剧情

```js
ui("watchIntro").bind("watch-intro", ctx => ctx.present("opening-movie"));
ui("opening-movie").bind("screen.open", ctx => ctx.sequence("opening-sequence"));
ui("opening-movie").bind("screen.close", ctx => ctx.resume("opening"));
```

present 保存当前剧情及 UI 交接点，再按页面 modal 规则打开页面；已经有活动交接时禁止嵌套 present。普通 open 仍用于设置、画廊等页面；从幕生命周期打开页面、或通过 UI 事件启动含视频的 sequence 时，宿主也会建立隐藏交接槽。

resume 可在普通 `ui(id).bind(...)` 回调或 `screen.close` 回调使用；没有活动交接时，读取并完整验证磁盘隐藏槽（包括已完成的 inactive 状态）。省略目标时恢复其 continuation；没有隐藏槽时必须显式指定起始幕。剧情已开始后指定目标以隐藏 continuation 为基础，记录显式 UI 返回并保留变量。首次尚未进入剧情时必须显式指定已声明起始幕（或载入手动/自动档），以该幕、初始变量和一条历史建立新会话，不使用 inactive 隐藏槽。指定目标与自动关闭回调的无参 resume 合并时保留指定目标，两个不同显式目标则拒绝整批操作。未显式切页时恢复原页面栈；若从独立主菜单进入 hud，可在回调中先 `ctx.open("hud")` 再 `ctx.resume("opening")`。hud 必须为非模态根页面。

离幕时打开 UI 会保留旧幕和未提交的续接点；进入幕时打开 UI 则将该幕与 UI 一起原子提交，并暂停该幕。返回同一已进入幕只恢复显示，不再次调用 game.enter/frameEnter，避免无条件循环播放。返回不同幕才执行新的进入回调。

返回后新的 frameEnter 若继续打开另一 UI，可形成下一段交接；新的交接会写入同一固定隐藏槽，仍不嵌套解码线程。不要让同幕 frameEnter 与该页 screenClose 无条件互相调用，除非确实要循环播放。返回事务中的 game.enter 不能再混合 CG/设置等持久 IO；应把这些操作放在独立普通交互中，避免把隐藏槽与其他文件当作跨文件原子事务。

### 32.4 隐藏槽、恢复与故障

固定文件为存档目录内 **flow-checkpoint.json**，不开放路径或槽号参数，不显示在手动/普通自动存档页，也不占用 auto-1…10 的 generation。它不依赖 settings.autosave 开关；自动保存关闭时仍保存交接。

| 保存内容 | 说明 |
| --- | --- |
| schema 1 / active / exited / entered | 隐藏交接契约、是否待恢复、旧幕退出状态、待续幕是否已进入 |
| source / continuation | 两份 save 9 快照，分别用于恢复保留剧情与提交待续剧情；加载时完整重放验证 |
| stack / origin / variables | 当前页栈、交接前页栈、完整 UI 局部变量；标题/图集绑定重新验证并生成 |
| sequence | 手动启动的演出 ID；恢复时从头播放，不保存视频时间、音轨或 GPU/解释器对象 |

写入使用同目录临时文件和平台原子覆盖，整个 JSON 上限 2MiB。候选条件/脚本/页面/媒体准备和预算检查完成后才写入，成功后才改变呈现与 Session。写入失败保留旧隐藏槽、旧 UI 和剧情，不执行页面结束动作；超出预算也拒绝交接。成功返回写 active=false 墓碑，启动时不会把已完成交接再播放一次；随后普通自动保存仍按自己的规则工作。

启动发现 active 隐藏槽时，先校验故事/内容版本、两份历史、变量、页面栈和资源引用，再恢复 UI；frameEnter/Exit 不重放，screenOpen 会再次执行，演出从零开始。screenOpen 建议只写幂等 UI 初始化与播放命令，不用追加计数模拟业务进度。坏槽或旧内容版本提示后保留文件，显示初始 XML UI 等待显式选择，不悄悄导入旧格式。正常退出时不把活动隐藏槽标为已消耗，所以视频中关闭程序后重启仍可继续交接。

活动交接暂停底层剧情，禁止普通存读档/重开/设置及其他持久 IO，普通 UI 返回流程除外；UI 视频的失焦/提示/调试暂停仍有效。热更替换延后到交接结束。flow-checkpoint.json 排除出监控与两种资源包，发布时不复制玩家数据。

### 32.5 save 9 与迁移

每条历史新增必填 `resume_to`，普通路由为 null；UI 返回边为实际目标 ID，且 choice_id 必须为 null。加载时普通边重放原 JS 条件/更新/目标，UI 返回边验证目标存在并保持变量不变；终点变量仍必须与重放一致。历史裁剪也推进相同检查点，不因返回 UI 丢失原剧情变量或历史。

```json
{"frame_id":"question","choice_id":null,"resume_to":"opening"}
```

这是历史条目的结构示意，不是完整存档；当前完整示例见 [save 9 / content 11 示例](cg-save.example.json)。save 8 及缺少 resume_to 的输入拒绝，旧存档文件不会被自动迁移或删除。视频从 frame 移到 UI、路由或生命周期改变时，作者须提升 content-version；demo 已从 8 提升为 9。

### 32.6 排错

| 现象 | 检查 |
| --- | --- |
| frame 下 video 报错 | 删除该子节点，把视频放进 UI sequence，并按 32.2 注册交接 |
| 视频结束后没有剧情变化 | finish 只管理 UI；要续接剧情，在 screenClose 注册 resume |
| 无法关闭唯一根页 | 把视频页面作为模态页压在 hud 上，或 finish=open:目标根页 |
| 隐藏交接已存在 | 不要嵌套 present；先完成/取消当前页返回。普通 open 不要求再创建嵌套交接 |
| 生命周期不断开关页 | 检查 `screen.open` 回调的 close 和 `screen.close` 回调的 open；事务会在预算内拒绝 |
| 返回幕不存在/变量报错 | frame 参数使用有效 ID/name；检查正常路由变量更新及待续目标条件，失败保留隐藏槽 |
| 重启又播放视频 | 活动交接按设计从头重播；成功返回后 active=false，不应重复恢复 |
| 旧存档拒绝 | v0.9.0 仅 save 9；保留旧文件供历史版本使用，当前版本不迁移 |

## v0.9.1 末幕 frameExit 修正

末幕文字完成后，手动继续执行一次 frameExit；补全操作不执行。注册了末幕 frameExit 时 game.canAdvance 保留继续入口，成功退出后关闭。Ctrl 与自动播放仍在结尾停止。UI 交接恢复同一末幕不重放进入/退出；加载普通剧情槽或重开后可再次退出。隐藏槽 source 与 continuation 相同且 exited=true/entered=false 仅允许用于 End 路由。

### UI 显式唤起剧情与隐藏续接（v0.9.1 增量）

控件通过普通 `ui(id).bind(...)` 回调使用相同命令，自动演出通过 `screen.close` 回调使用相同命令，无需新增 XML 格式：

```javascript
ui("startStory").bind("start-story", ctx => { ctx.open("hud"); ctx.resume("opening"); });
ui("hiddenStory").bind("hidden-story", ctx => { ctx.open("hud"); ctx.resume("secret-entry"); });
ui("continueStory").bind("continue-story", ctx => { ctx.open("hud"); ctx.resume(); });
ui("opening-movie").bind("screen.close", ctx => ctx.resume("opening"));
```

目标必须为已存在的 ID/name（示例 secret-entry 需要作者定义）。剧情已经开始后，显式选幕沿用隐藏槽的变量与历史，不等于清空变量的 restart；仅目标不同才增加 resume_to 边。首次尚未开始剧情则必须选已声明起始幕，建立初始变量和单条历史（详见末尾多起始幕章节）。所有进入回调和媒体验证成功后，将实际起始幕/变量/历史同步到隐藏 source/continuation，随后提交剧情。无下一交接时写 inactive，下一次无参 resume 仍可显式读取其 continuation；inactive 不会在启动时自动恢复。坏槽、坏目标、冲突目标或原子写失败保留旧剧情/UI/文件。

## 专用 CG 资源与共享解锁

```xml
<!-- story.xml 的 assets：作者手工指定稳定标记 -->
<cg id="cg.promise" src="assets/cg/promise.png" />
<!-- visuals：cg 不可与 background 或 portrait 混用 -->
<visual id="promise.full" cg="cg.promise" />
<!-- frames -->
<frame id="promise" visual="promise.full"><text>约定的那一天。</text></frame>
```

资源会自动加入画廊。若需要标题，在 UI 的 gallery 中声明 `<cg id="cg.promise" image="cg.promise" title="约定"/>`，标记必须与专用资源相同。默认未解锁，可配置 unlocked 初值。普通 image 仍可作为作者手工配置的画廊收藏项；总数按画廊唯一 ID 统计。

```javascript
// 合并到作者现有 game.enter 回调，不能重复注册。
ui("$game").bind("game.enter", ctx => {
  if (ctx.game.cgId) ctx.unlock(ctx.game.cgId);
});
```

`game.cgId` 是当前专用 CG 的作者标记，普通画面为空字符串。解锁时机由作者 JS 决定，不是加载资源即解锁。共享 profile.json（schema 1/story_id）保存标记集合，同 CG 多次出现只计一次；普通/自动/隐藏存档不保存集合，读旧槽或重新开始不撤销解锁。历史验证不执行解锁回调。

UI 可绑定 `cg.total`、`cg.unlocked`（number）与 `cg.progress`（string，如“已解锁 1 / 3”）。例如 `<label id="cgCount" width="400" height="40" bind="cg.progress"/>`。CG 与普通图片复用解码、预算、缓存和 GPU 绘制，不逐帧读盘。默认 demo 的示例图复用既有房间素材，不声称新增绘制 CG 插画。

## 多起始幕与首次 UI 选择

story 8 的 `entry` 支持空白分隔列表，例如 `entry="main.opening secret.opening"`。每项必须是已存在场景的 ID/name，同幕重复（包括 ID 和别名重复）拒绝并报告源位置。所有入口参与初始条件、game.enter 计划及媒体命令验证；KPAK 1/2 保留列表及名字映射。单项也是相同契约，没有自动进入的默认项。

```javascript
// ui.xml initial 指向作者主菜单；hud 为非模态根页面。
ui("startMain").bind("start-main", ctx => { ctx.open("hud"); ctx.restart("main.opening"); });
ui("startSecret").bind("start-secret", ctx => { ctx.open("hud"); ctx.restart("secret.opening"); });
ui("loadStory").bind("load-story", ctx => { ctx.open("hud"); ctx.load(ctx.item.slot); });
```

启动先打开 initial UI，不执行剧情进入、音频、自动档或已读记录。调试 status 的 game_started 为 false，frame_id/frame_name 为 null、history_length 为 0；选择或载入成功后恢复正常剧情状态。首次 `ctx.resume(frame)` 和 `ctx.restart(frame)` 都必须显式选择声明的入口，建立初始变量及仅含该幕的一条历史；不会加入“默认幕→选择幕”的 resume_to。首次无参 resume/restart、推进、选项、保存及自动播放拒绝。UI 可先播放 sequence，screenClose 也能显式 resume 起始幕；尚无剧情时 UI 演出不建立虚构的剧情隐藏槽。

已有有效 active 隐藏槽表示之前已选择并交接，仍按第 32 节完整恢复；inactive 槽不自动开始剧情，也不影响首次显式新开。普通手动/自动槽可以从任意有效存档位置进入，保留变量及历史，不受起始幕集合限制。

剧情已开始后，`ctx.resume(frame?)` 继续沿用既有隐藏续接语义；`ctx.restart(frame)` 重新初始化变量/历史，适合从主菜单切换全新故事线。运行中无参 restart 只允许单入口游戏，多入口必须传参。隐藏路线是否显示、何时解锁由已有 XML visible/绑定与 JS 决定，声明入口本身不代表在菜单公开展示。

当前 demo 为 content-version=11；应用 v0.9.1、story 8/UI 3/save 9/profile 1/KPAK 1/2 不变。新增完整演示位于 games/demo/ui/title.xml 和 UI 脚本；验证结果见 [多起始幕验收](multiple-entries-validation.zh-cn.md)。

## 自定义确认弹窗与退出游戏

确认弹窗复用普通 XML 模态页，不是固定样式的系统 MessageBox，也没有额外的 dialog 标签或 ctx.confirm 接口。可直接参考 [共享确认页](../games/demo/ui/confirmation.xml)、[UI 状态](../games/demo/ui/state.xml) 和 [事件脚本](../games/demo/scripts/ui.js)。demo 的主菜单/HUD“退出游戏”使用有标题弹窗，“重新开始”使用无标题弹窗。

```xml
<!-- 放在 UI 3 根内；标题不需要时可直接删除该 label。 -->
<screen id="ask-exit" modal="true" pause="true">
  <panel id="askShade" width="1280" height="720" color="#000000A0" />
  <panel id="askPanel" x="340" y="210" width="600" height="300" color="#18232AF2" radius="16">
    <label id="askTitle" x="36" y="26" width="528" height="42" text="退出游戏" font-size="28" color="#F0CA89" />
    <label id="askText" x="36" y="92" width="528" height="84" text="确定退出游戏？" font-size="23" color="#F5F1E8FF" />
    <button id="askYes" x="36" y="210" width="248" height="52" text="退出" event="quit-game" color="#405B7FFF" font-size="22" radius="8" />
    <button id="askNo" x="316" y="210" width="248" height="52" text="取消" route="close" color="#43576FCC" font-size="22" radius="8" />
  </panel>
</screen>
```

调用按钮可用 `route="open:ask-exit"`，或 event 回调 `ctx.open("ask-exit")`。弹窗正文/标题/按钮文字可改为 bind；标题用 UI bool 绑定 visible，JS 在打开前通过 ctx.setUi 一次赋值。demo 的 askConfirmation 是作者 JS 函数，使用固定动作分支，不执行动态 JS 字符串。

背景可用 panel/image/nine-patch 和已声明资源 ID，色值的最后两位为 alpha（00 透明、FF 不透明）；正文/标题可自定义颜色、字号、布局，按钮可配置普通/悬停/禁用皮肤、颜色、圆角和字号。button 的 color 控制按钮底色或图片 tint，默认按钮文字为白色；需要单独定制按钮文字颜色时，使用空文字 button 与同位置的有色 label 组合。没有逐帧样式脚本或任意路径读取。

```javascript
ui("askYes").bind("quit-game", ctx => ctx.quit());
```

`ctx.quit()` 无参数，请求关闭当前游戏窗口，等价于系统关闭按钮。作者可以直接调用，也可以先经过上述确认页；引擎不会额外弹出固定确认窗口。命令只允许普通 UI 回调，不允许剧情回调、game.enter 或 frame/screen 生命周期自动关闭。整批绑定/页面回调/命令及媒体验证失败时不退出、不部分提交；同批不能混入保存、读档、设置、CG 写入或剧情推进，UI 演出/音效仍遵守原有互斥规则。

退出走正常宿主关闭和 shutdown：补写已读与设置，尝试完成已有待写自动档，然后释放音频/媒体/渲染资源；不会隐式保存一个新的手动槽，也不强制终止进程。退出窗口不等于剧情离幕，因此不触发 frameExit/screenClose；active 隐藏交接槽保留，下一次启动仍按原机制恢复。无新增 Model/存档字段或文件格式。

## 33. v0.10.0 动态图片查看组合

本需求不需要新的 Rust 图片 API，复用已有的 `image-bind`、UI string、`ctx.setUi`、页面栈和静态纹理缓存。推荐结构：

```xml
<screen id="cg-view" background-click="cg-background">
  <panel id="viewerBackground" width="1280" height="720" color="#000000FF" />
  <image id="cgFull" width="1280" height="720" image-bind="ui.cgImage" fit="contain" />
  <panel id="viewerBar" width="1280" height="88" color="#070D18EE" visible="ui.cgToolbar">
    <!-- 标题、计数、上一张/下一张、隐藏、返回 -->
  </panel>
</screen>
```

```javascript
ui("viewCg").bind("view-cg", ctx => {
  if (!ctx.item.unlocked) return;          // 不能只依赖按钮 disabled
  ctx.setUi("cgImage", ctx.item.image);    // 必须是已声明 image/CG 资源 ID
  ctx.setUi("cgTitle", ctx.item.title);
  ctx.open("cg-view");
});
ui("cgNext").bind("cg-next", ctx => {
  // 作者维护稳定收藏 ID -> 资源/标题表；不要假设存在 ctx.gallery。
  ctx.setUi("cgImage", table[index + 1].image);
});
```

规则：

- 一个查看页只使用一个固定 ID 的 image 控件；切换 CG 只改 UI 变量，不在 JS 中拼接 XML，也不为每张图复制 screen。
- 图片来源是已声明资源 ID，不是文件路径、URL、Base64 或纹理名；未声明、空值、路径、错误类型整批拒绝并保留旧页面。
- 同一资源可用于多个 gallery 收藏项；“当前收藏 ID”与资源 ID 分开，导航和记忆位置用收藏项 ID。未解锁项可由作者 JS 拒绝，禁用按钮无事件。
- 初始 `image-bind` 也须有效；没有图片时用 `visible="false"`，不要用空字符串表示隐藏。
- `fit="contain"` 保持整图可见，白色/透明留边由作者黑色或背景面板填充；`cover`/`stretch` 只能由作者明确选择。
- 工具栏隐藏用 UI bool，恢复用页面 `background-click`；第一次背景点击只恢复工具栏。返回只关闭当前页，不推进/补全剧情、不修改已读、CG 或 Session。
- 图像来自启动/候选准备的静态缓存；翻页不读盘、不解码、不重新上传纹理。图集 `region`/`region-bind` 仍按最终图像校验边界。
- KPAK 1/2 继续收集声明的资源和 UI 依赖；JS 表中的字符串不会让打包器自动发现文件，必须先声明资源。
- demo：gallery 打开同一 `cg-view`/`cgFull`，可切换“午后房间”“凛的立绘”“透明方图”；专用 CG 与普通 image 共用同一查看页。

## 34. v0.10.0 玩家快捷键

### 动作与默认值

| 动作 ID | 默认绑定 | 触发 | 有效上下文 |
| --- | --- | --- | --- |
| `advance` | `Space` | 单次按下边沿，忽略系统 repeat | 正常顶层剧情；跳过转场→补全→推进，每次至多一步 |
| `fast_forward` | `Ctrl`（按住） | 按住，约 40ms 一步 | 正常剧情且已读或 `skip_all`；选项/结尾停止，不代选 |
| `skip_sequence` | `Space` | 单次按下边沿，忽略 repeat | 可跳过的 UI 演出/视频；只被演出消费 |

绑定为规范 ASCII 字符串或 `null`（禁用键盘动作）。鼠标/按钮/取消与“恢复默认”始终保留。高级动作、宏、多段序列、任意按键流或系统热键注册均不支持。

键位主键 token：`Space`、`Enter`、`Tab`、`Backspace`、`Delete`、`Insert`、`Home`、`End`、`PageUp`、`PageDown`、`ArrowLeft`、`ArrowRight`、`ArrowUp`、`ArrowDown`、`KeyA`…`KeyZ`、`Digit0`…`Digit9`、`F1`…`F12`、`Minus`、`Equal`、`BracketLeft`、`BracketRight`、`Backslash`、`Semicolon`、`Quote`、`Backquote`、`Comma`、`Period`、`Slash`、`Numpad0`…`Numpad9`、`NumpadAdd`、`NumpadSubtract`、`NumpadMultiply`、`NumpadDivide`、`NumpadDecimal`、`NumpadEnter`。修饰键只有 `Ctrl`、`Alt`、`Shift`；保存时按 Ctrl、Alt、Shift 固定排序。

拒绝：Escape（保留给捕获取消）、Windows/Meta、Ctrl+Alt（含再加 Shift）、Alt+F4、Alt+Tab、Alt+Escape、Alt+Space、Ctrl+Escape、Ctrl+Shift+Escape、重复/未知 token、空白、裸 `+`、超 64 字节或多段组合。只有 `fast_forward` 可以绑定单个修饰键；其他动作禁止纯修饰键。

### 冲突

整张候选映射一起验证，不在写入过程中逐个覆盖：

- `advance` 与 `fast_forward` 不能使用同一触发；`fast_forward` 是单个修饰键（如 Ctrl）而 `advance` 使用带该修饰键的组合（如 Ctrl+KeyK）时按“按住前缀”拒绝，并提示先改快进键。
- `advance` 与 `skip_sequence` 使用 Space 合法，因为剧情与 UI 演出上下文互斥。
- 同批交换两个动作的绑定整体成功；任一非法则全部回滚。`null` 只禁用该动作。

### UI 命令与快照

| API | 行为 |
| --- | --- |
| `ctx.captureShortcut(action, resultEvent)` | 开始一次瞬时捕获；`action` 必须为稳定动作 ID，`resultEvent` 必须是绑定到 `$host` 的普通捕获结果事件，禁止生命周期事件 |
| `ctx.cancelShortcutCapture()` | 取消当前捕获，不写偏好 |
| `ctx.keybinding(action, bindingOrNull)` | 修改候选绑定；同一批最多三个动作，整批验证/原子写后生效 |
| `ctx.resetKeybindings()` | 单独恢复三个默认绑定并原子保存，不能与单项修改同批 |
| `ctx.shortcuts` | 已提交只读快照：`bindings.advance/fast_forward/skip_sequence`、`advanceLabel`、`fastForwardLabel`、`skipSequenceLabel`、`captureActive`、`captureText` |

XML 可绑定 `shortcuts.advanceLabel`、`shortcuts.fastForwardLabel`、`shortcuts.skipSequenceLabel`、`shortcuts.captureText`（文本）、`shortcuts.captureActive`（bool）以及 `shortcuts.bindings.*`（规范文本）。显示 Label 不参与解析或存储。

### 捕获生命周期

1. 点击“更改”等普通 UI 按钮启动捕获；启动点击和当帧残留键先全部释放。
2. 武装后收集一个支持的组合；纯修饰键只在释放后确认（仅快进）。
3. 组合全部释放后，在下一个可提交边界派发一次普通 UI 事件：`ctx.item.action` 为动作 ID，`ctx.value` 为规范绑定字符串。
4. Escape 在任意阶段取消；失焦、换页、关闭页面、热更、普通读档/重开、再次捕获都会清掉旧候选，不把结果送到新页面。
5. 捕获结果事件不会自动写文件；作者用 UI string 保存草稿，点“应用”才调用 `ctx.keybinding`。清空绑定是显式按钮，不把第一次 Backspace 偷当删除。
6. 捕获进行时，普通剧情键、自动播放、UI 演出跳过和其他控件编辑不处理同一键；鼠标取消按钮可工作，其他鼠标事件被消费而不会同时触发原剧情。
7. 每次至多一个活动捕获和一个待投递结果；不注册全局键盘监听，不记录窗口外输入。

### 偏好文件

固定路径为当前存档目录的 `keybindings.json`，schema 1：

```json
{
  "schema_version": 1,
  "story_id": "kakera-demo",
  "bindings": {
    "advance": "Enter",
    "fast_forward": "Ctrl",
    "skip_sequence": null
  }
}
```

必须恰有 `advance`/`fast_forward`/`skip_sequence` 三个字段，值为规范字符串或 `null`；未知字段、重复 JSON 键、缺字段、错 story、非法/冲突绑定、超过 16KiB 一律拒绝。缺文件使用默认；坏文件保留原字节、提示一次并使用默认，不自动覆盖或迁移。保存使用同目录临时文件原子替换；写失败保留旧文件、旧映射和当前 UI。该文件不进入剧情槽、隐藏交接、自动档、工程监控或 KPAK；大小写变体同样排除。

### 输入所有权

宿主按以下顺序形成每帧唯一消费结果：失焦/提示/调试门控 → 快捷键捕获 → 文本/控件编辑 → UI 演出 → 顶层 UI 事件 → 单次剧情按键 → 按住快进 → 自动播放。低层不重复消费同一物理输入。失焦、捕获结束/取消、键位应用、热更、读档/重开和演出退出后要求相关键释放再武装；自动截图等短暂等待只暂停节拍，不要求每步松键。

输入优先使用物理键；字符文本/IME 不反推键位。当前 Windows egui-winit 桥无法区分部分小键盘键与主行键（如 Numpad0/Digit0、NumpadEnter/Enter），`NumpadAdd` 可识别；这些 token 在配置中仍合法，但捕获时可能保存对应的主行 token。该限制若需消除，需另加宿主原始键通道，不在本版宣称。

## 35. v0.10.0 HSL 方形选色器与玩家配色

### 颜色值

- 对外规范值为未预乘 sRGB RGBA：`#RRGGBBAA`，九个 ASCII 字符、十六进制大写。
- 作者静态初值/绑定输入可写 `#RRGGBB` 或 `#RRGGBBAA`（大小写均可）；六位补 `FF`。非法、空值、路径、CSS 名称、浮点对象一律拒绝，不默认为黑色。
- HSL 针对编码后的 sRGB 通道（字节/255），H∈[0,360)，S/L/A∈[0,1]；L 是 HSL 亮度，不是 HSV 的 V。360 度归零；量化按 `round(clamp(x,0,1)×255)`。
- `alpha=0` 的非零 RGB 必须保留；不经过预乘 Color32 存储。

### HSL 圆环与正方形

```xml
<color-picker id="backgroundPicker" x="40" y="110" width="256" height="282"
              value="ui.dialogueBgDraft" event="color-bg" alpha="true" />
```

| 属性 | 规则 |
| --- | --- |
| `id/x/y/width/height` | 沿用逻辑舞台坐标；建议 256×282（含 alpha 条）或 256×256（不含），至少 128×154 / 128×128 |
| `value` | 必需，只能绑定 `ui.<string>`；初始值也必须是合法颜色 |
| `event` | 必需，须绑定到该控件 ID 的普通回调 |
| `alpha` | 可选 bool，默认 true；false 隐藏 alpha 编辑条但保留输入颜色原有 alpha |
| `visible/enabled` | 现有 bool 绑定；禁用区仍阻挡舞台背景输入 |

控件不是容器，不接受子节点、图片、region、route、color 或未知属性；不能放在 repeat 中。整个 UI 至多 8 个 color-picker。色相环包围固定方向的正方形：横向从左到右为 S=0…1，纵向从上到下为 L=1…0。上边全白、下边全黑，左边为灰阶，右边中点为当前色相纯色。环拖动只改 H，方形内部任意位置均可命中与拖动，拖出后按两个坐标分别夹取到边界，alpha 条夹在 [0,1]。一次按下锁定子区域直到释放，释放位置参与最后一次候选；黑白边缘仍保留横向位置和色相。自身 RGBA 回显保留浮点位置，作者提交不同颜色时以绑定值为准。旧三角控件已替换，无形状选择或旧方案运行分支。

控件自身只绘制色相环、正方形和可选 alpha 条（条内棋盘、渐变、滑块）以及必需的焦点/禁用反馈；**没有内置颜色预览矩形，也没有内置 HEX 文本**。预览与十六进制由作者用 `color-bind` 面板/图片和 `bind` label 读取同一绑定独立组合，可放在控件左右、上方或另一页面，也可完全省略；半透明棋盘需要时同样由作者现有图片/面板组合。控件不引用这些外部节点，删除或移动它们不影响选色。

尺寸契约：`width/height` 是外矩形。色盘直径 `D = min(width, height − R)`，`R = alpha ? 26 : 0`（8 逻辑单位间距 + 18 高 alpha 条），整组内容在外矩形内水平/垂直居中；环外半径 `D/2−6`、内半径为其 0.82 倍，正方形边长 `外半径×0.78×√2`。alpha 条在色盘下方 8 处、高 18、左右各留 8、宽 `width−16`；`alpha=false` 时不创建、不绘制、不命中该行，也不保留其 26 高度。加载校验使用同一 `D≥128` 定义，不再接受旧 128×224 下限；256×320 等合法矩形继续按新规则居中布局。矩形内间隙/留白按下不改色、不启动抓取、不发事件，也不穿透到剧情。

拖动只产生候选颜色，不直接改变量。控件派发一个事件：`ctx.item` 为空，`ctx.value` 为规范九字符颜色。作者在回调里决定接受/拒绝，并调用 `ctx.setUi` 更新 value 绑定。JS 错误或整批其他命令失败时颜色/页面/IO 都不部分提交；标记回到已提交值，抓取结束。同一帧多个移动只产生一个事件；外部 setUi、打开页、读档、热恢复和改布局不会自动回发事件。

键盘：点击控件或从前后控件用 Tab/Shift+Tab 取得编辑焦点。Tab/Shift+Tab 在环、正方形、alpha（若启用）与前后控件间移动；环左右调整 H（默认 1°、Shift 10°），正方形左右调整 S、上下调整 L（默认 0.01、Shift 0.1），alpha 左右调整 A。Escape 结束本次编辑。控件拥有焦点时阻挡普通剧情键。首次点击当前颜色、控件内留白或禁用控件，即使没有颜色变化事件，也先消耗本帧输入，再仲裁剧情前进、按住快进和自动播放。隐藏、禁用、父容器隐藏或裁剪使控件失效时清理焦点/抓取；离开编辑后须释放旧按键再触发剧情，不回放编辑期间的按键。

### color-bind

`color-bind` 复用 `ctx.setUi`，不是新的绘图或路径接口。支持 panel、image、nine-patch、label、button、toggle；与静态 `color` 互斥。绑定源只能是 `ui.<string>` 或已声明的 `colors.<槽位ID>`。

- panel/image 作用 tint；nine-patch 作用到全部分段顶点；label 作用文本颜色（`game.text` 仍保留不可见后缀）；button 只控制背景/图片 tint，默认白字不被同色覆盖；toggle 作用勾选框文字。
- 不支持 scene 整体着色、repeat 容器着色、slider 样式、任意 JS 表达式、纹理像素或路径。
- 准备阶段解析所有绑定并缓存 RGBA；初始值、隐藏节点、setUi 最终批次、偏好变化和热更候选都重新验证；坏值整批拒绝，绘制不逐帧解析十六进制或重建纹理。
- 图集 region/region-bind 与 color-bind 可组合；滤镜仍按整图缓存。

### palette 与玩家偏好

```xml
<palette>
  <color id="dialogue_background" initial="#10253BFF" />
  <color id="dialogue_text" initial="#F4EAD8FF" />
</palette>
```

槽位 ID 为 `[a-z][a-z0-9_]{0,63}`，不含点，最多 32 个；全部 UI 装配后全局唯一，可在 include 中前向引用并保留原位置诊断。`palette` 不进入剧情变量/CG ID；初始值可被玩家覆盖。

编辑页打开时把已提交 `ctx.colors` 复制到 UI 草稿；预览用 `color-bind` 指向草稿，正式 HUD 指向 `colors` 槽位。取消只关闭页面，不写文件；应用通过 `ctx.colorPreference` 提交。

| API | 行为 |
| --- | --- |
| `ctx.colors` | 已提交配色的只读快照，键只能是声明槽位，值为规范颜色 |
| `ctx.colorPreference(slot, rgbaOrNull)` | 保存一个槽位覆盖；`null` 删除该覆盖、恢复作者默认，不删除文件 |
| `ctx.resetColors()` | 单独恢复全部作者默认，原子写空覆盖表；不能与逐项修改同批 |

固定文件为当前存档目录的 `ui-colors.json`：

```json
{
  "schema_version": 1,
  "story_id": "kakera-demo",
  "overrides": {
    "dialogue_background": "#FF2630FF"
  }
}
```

`overrides` 按设计是稀疏表；缺槽位使用作者默认。文件缺顶层字段、未知/重复槽位、非规范颜色、错 story/schema 或超过 16KiB 全部拒绝。缺文件使用默认；坏文件保留原字节并提示一次后使用默认，不自动重写。应用使用同目录临时文件原子替换，失败保留旧文件/已提交配色/UI/草稿；不得与 settings、快捷键、profile、剧情或退出混写。普通/自动/隐藏剧情槽和重开都不回滚已提交覆盖。热更新若删除仍被覆盖的槽位，报告不兼容并保留旧状态，不静默丢玩家配色。

该文件独立于剧情槽、隐藏交接、自动档和 settings.json；不进入工程监控或 KPAK，大小写变体同样排除。控件指针、浮点 HSL、焦点和抓取子区是瞬时状态，不写入任何偏好或存档。

demo：设置页“配色…”打开 `colors` 页，背景色 256×282 含 alpha 条、文字色 256×256 不含；`colors.xml` 里另有独立 `colorPreview` 面板、`colorPreviewText`、`bgValue`/`textValue` HEX label，全部绑定同一个 `ui.dialogueBgDraft`/`ui.dialogueTextDraft` 草稿，控件本身不再内绘制预览或 HEX。`games/demo/ui/hud.xml` 的对话点九图和台词 label 绑定 `colors.dialogue_background`/`colors.dialogue_text`。职责拆分依据见 [落地方案](hsl-picker-composition-implementation-plan.zh-cn.md) 与 [验收方案](hsl-picker-composition-validation-plan.zh-cn.md)，实际结果记录在后者。

## 36. v0.10.1 国际化

宿主提示使用内嵌 `locales/engine/zh-CN.ftl`，目前只提供中文，不受游戏开关控制。词条采用稳定 ID/拥有的参数；Model/Controller 只产生 Diagnostic，显示边界解析，游戏不能覆盖宿主词条。方向隔离符 U+2068/U+2069 保留；不要将最终文本作为 XML、JS、路径或资源 ID 使用。

### 36.1 作者显式开启与清单

story.xml 入口可加 `<localization enabled="true" src="languages.xml"/>`；缺省或 `enabled="false"` 关闭，false 禁止 src。关闭不读取清单/FTL/language.json，也不应用系统语言。只有入口可声明。

```xml
<localization schema="1" default-text="zh-CN" default-voice="ja-JP">
  <texts>
    <locale id="zh-CN" label="简体中文">
      <messages src="locales/zh.ftl"/><font ref="font.chinese"/>
    </locale>
    <locale id="en-US" label="English">
      <messages src="locales/en.ftl"/><font ref="font.latin"/>
    </locale>
  </texts>
  <voices>
    <locale id="ja-JP" label="日本語"><clip key="voice.opening" asset="voice.ja.opening"/></locale>
    <locale id="zh-CN" label="中文"><clip key="voice.opening" silent="true"/></locale>
  </voices>
</localization>
```

两组必须存在。texts 为 1–32 种，各有 1–16 个 messages 和 1–16 个 font；voices 为 0–32 种。默认值必须属于对应集合；voices 为空时禁止 default-voice，语音选择禁用，原字面量剧情 voice 保持原行为。font/asset 都是 story 已声明的资源 ID，clip.key 为剧情 audio.voice 使用的逻辑资源 ID；默认语音映射须完整覆盖。非默认缺 clip 用默认，silent=true 明确静音。未使用的语音键、重复映射、坏类型拒绝。

标识为 unic-langid 支持的规范 Unicode language identifier（如 zh-CN、en-US、ja-JP），至多 64 ASCII 字节；禁止下划线、und、扩展、私有附加或任意路径。系统值可规范化大小写/下划线，作者和玩家文件必须规范。各通道分别按“有效手动请求 → 系统有序列表第一个精确命中 → 作者默认”选择；不做地区/脚本猜测。旧偏好中已移除的规范 ID 保留请求并提示，直接使用作者默认；运行命令提交未声明 ID 则拒绝。

### 36.2 文字来源与参数

| 位置 | 互斥来源 |
| --- | --- |
| frame/text | 原正文或空 `<text key="opening"/>` |
| option | 原正文或 `text-key="choice"` |
| character | 原 name 或 name-key |
| 剧情变量 | 原 label 或 label-key |
| UI label/button/toggle | 原 text/文本 bind 或 text-key/text-key-bind |
| UI gallery/cg | 原 title 或 title-key |
| UI 入口窗口标题 | title/title-bind 或 title-key/title-key-bind |

toggle 的 value/布尔绑定仍控制勾选，与可本地化的文本标签分开。新文本属性只允许在表中位置；关闭时使用 key 或 text-arg 拒绝。无 key 的字面量始终原样显示。动态 key 仅可绑定 UI string；repeat 内可以用静态 key，不接受 item 提供任意 key。根标题和剧情等静态位置不能有外部参数。

```xml
<label id="count" width="500" height="50" text-key-bind="ui.messageKey">
  <text-arg name="n" bind="ui.count"/>
</label>
```

```ftl
count = 当前数量：{ $n }
```

每个 UI 文本最多 16 个 text-arg，name 不重复，value（字符串）/bind 二选一。绑定仅 ui bool/int/string、cg.total/cg.unlocked/saves.page/saves.pages、locale.text/locale.voice。bool 作为 true/false 字符串；int 保持数值，但超出 ±(2^53−1) 拒绝。所有语言及实际默认回退消息都必须满足完整参数集合。缺 key/缺参/多参/坏动态 key 整批拒绝。JS 继续用 ctx.setUi 改 key/参数，不提供 ctx.tr。

同一消息只在一个 bundle 内求解；选中语言缺根消息才整条回退到作者默认。坏引用/循环/语法错误不回退。支持正文、字符串/数值、消息/无参数术语引用、变量与 select；不支持属性导出/属性引用、带参数术语和自定义函数。全部语言总计至多 128 个 FTL/16MiB、单文件 1MiB、50000 条消息/术语、100000 条引用边（单 bundle），引用/解析嵌套至多 32；展开前保守预算拒绝指数增长。文本单参 4KiB、至多 16 参、输出 64KiB；剧情/UI/标题更小的既有限制优先。全语言静态/展开文本缓存合计 64MiB，最终 UI 文本缓存 64MiB。无外部参数的消息预先格式化；动态文本仅在候选刷新时格式化，绘制只读。

当前文本字体链优先，补充随包宿主字体；全部声明语言预检字面字形与初始标题/参数。动态缺字拒绝提交，不使用操作系统字体弥补。

### 36.3 选择页、事务与存档

```javascript
ui('applyLanguage').bind('apply-language', ctx => {
  ctx.locale('text', ctx.ui.textDraft || null);
  ctx.locale('voice', ctx.ui.voiceDraft || null);
  ctx.close();
});
```

仅普通 `ui(id).bind(...)` 回调可用；channel 只接受 text/voice，locale 为已声明规范 ID 或 null（跟随系统）。同批每通道至多一次，可附带 UI 草稿/提示/页面变化；禁止混合 settings/配色/CG/快捷键/剧情/存读档/退出或其他持久 IO，生命周期禁止调用。

只读 ctx.localeInfo 为 enabled、textRequested/voiceRequested、text/voice、textSource/voiceSource；无值是真实 null。XML 对应 locale 标量以空串表示 null，source 为 manual/system/default/disabled。textLocales/voiceLocales 列表仅有 id/label/selected/default。原始系统列表不开放给 JS；“跟随系统”由作者按钮提交 null，不伪造 ID。

独立 `language.json` 位于 SaveStore 目录：schema_version=1/story_id/text/voice 全字段必需，至多 16KiB，拒绝未知/重复字段；坏文件保留并提示，缺失或损坏按 null/null，启动不自动重写。每次应用先验证文本/字体/标题/映射/页面候选，再同目录原子覆盖，一次提交；失败保留旧 Session、UI、偏好、字体和声音。

effective 文本改变使当前句完整显示、更新截图 revision，不推进或触发幕事件；只换文本不重播声音。effective 语音改变只停止当前剧情 voice，下次真正入幕才播放新映射；BGM/SE 不重启。仅 requested 改变而 effective 不变不重置逐字。成功应用停止自动播放、清快进/捕获/picker 并要求松键。活动隐藏交接、sequence/视频、剧情转场、调试暂停或 pending 自动截图期间拒绝切换。

语言不进入 save 9/history/隐藏槽/CG/已读；已读跨语言共享稳定 frame ID。读档采用当前语言，手动与自动槽摘要按当前语言投影；已存截图保留保存时像素。隐藏槽恢复先加载独立偏好，再格式化保存的 key/参数。热更完整验证所有语言，保留请求/系统采样，坏候选回滚；兼容成功不重播音频。渲染重建重新安装当前字体链。

Windows 启动采样 GetUserPreferredUILanguages 有序列表，去除非法/重复项，最多 32 项；失败中文提示并默认。其他平台目前降级为空列表，使用作者默认。不存在 CLI 语言覆盖。

story 8/UI 3/save 9/profile 1/read 1、KPAK 1/2 及 demo content-version=11 不变。开启时两种包均收集全部声明语言依赖；language.json（含大小写变体）排除监控/打包/显式附加。纯翻译修改不改变剧情档案契约。

可运行演示生成方式见 [示例源文件](localization-example/README.md)。暂不支持 RTL 布局、复杂脚本塑形、逐语言图片/视频/模型、视频多音轨、剧情参数化或超现有解码预算的语音流式播放。

## 37. v0.11.0 新版 JS 注册 API

v0.11.0 用 `frame(id)` 与 `ui(id)` 选择器取代旧 `on.*` 全局注册表。**旧入口已删除，没有别名、运行时迁移或回退分支**：出现 `on.advance/choice/end/ui/frameEnter/frameExit/screenOpen/screenClose` 的脚本在启动、`--check` 和打包时带作者文件名与栈拒绝，必须由作者迁移后重建资源包。`ctx` 的参数、快照字段、命令白名单、事务与预算全部不变。

### 37.1 剧情路由

| 旧写法（已移除） | 当前写法 |
| --- | --- |
| `on.advance(frameId, run)` | `frame(frameId).advance(targetOrRun)` |
| `on.choice(frameId, optionId, run, when?)` | `frame(frameId).choice(optionId, targetOrRun, when?)` |
| `on.end(frameId)` | `frame(frameId).end()` |

```javascript
frame('opening').advance('question');
frame('question').choice('walk', ctx => {
  ctx.add('trust', 1n);
  return 'walk';
}, ctx => ctx.vars.trust < 3n);
frame('question').choice('finish', 'ending');
frame('question').choice('invite', 'ending', ctx => ctx.vars.trust >= 2n);
frame('walk').advance(() => 'question');
frame('ending').end();
```

- `frame` 只接受非空字符串 ID/name，仍由宿主解析真实 ID；不得按声明顺序生成默认路由。
- `targetOrRun` 有两种形式。**字符串**：`advance('next-frame')`、`choice('option-id', 'next-frame', when?)`，等价于“无变量更新、直接返回该字符串”的同步函数；**函数**：`ctx => 'next-frame'`，可先 `ctx.set/add` 再返回目标。两种都在注册时区分，静态字符串目标在完整装配后于加载期解析并校验存在，函数目标在执行边界校验。任何目标都经同一个 Controller 条件/更新/目标/历史事务，不因静态可知而跳过目标条件、隐藏选项或生命周期。
- 可选第三参数 `when` 只能是返回 boolean 的只读函数；`targetOrRun` 与 `when` 不互换，不按参数类型猜测省略目标。
- 拒绝空字符串、数字、bool、null、undefined、数组、对象、装箱 `String` 与其他非函数值，不做 `String()`/`toString()` 强转。函数返回值仍必须是同步非空原始字符串，Promise 等无效。
- 同一幕只允许一种路由；`choice` 可分多次声明不同 option。重复 option、重复 `advance`/`end`、混合路由、ID 与 name 指向同一幕均拒绝。路由必须完整覆盖 XML 场景，`choice` 集合必须与 XML options 相等；生命周期注册不构成路由覆盖，也不与路由互斥。
- 选择器只捕获稳定字符串，不是剧情对象，也不能读取当前 Session。允许保存选择器后重复注册，也可解构方法（`const { advance } = frame('walk')`）而不丢目标；`frame(id)` 单独出现不注册任何东西。
- `end()` 只声明 End 路由，不等于 `ctx.quit()`。

### 37.2 界面回调与事件名

`bind` 的 event 仍是 **XML 当前的作者事件名**，不是 DOM 的 click/change 类型：

```xml
<button id="continueButton" width="120" height="40" text="继续" event="advance" />
<slider id="textSpeed" width="300" height="36" value="settings.text_speed" min="0" max="80" event="text-speed" />
```

```javascript
ui('continueButton').bind('advance', ctx => ctx.advance());
ui('textSpeed').bind('text-speed', ctx => ctx.setting('text_speed', ctx.value));
```

- 注册键是结构化的 **(目标类别, 目标 ID, 事件名)**，不再按全局事件名索引：两个按钮可以保留同一 `event` 并各自绑定不同回调。共享逻辑请把同一个函数绑定到多个目标。
- 同一完整键重复注册拒绝，报告两次注册位置；不会最后覆盖或执行多个回调。
- 完整装配后校验：每个 XML 事件（控件 `event`、页面 `background-click`/`background-right-click`）必须恰好有一个注册；每个注册必须指向真实目标且事件与 XML 完全一致。未声明 event 的控件、只读节点（label/image/panel 等）、页面与控件同 ID、拼错大小写、未知目标、多余注册全部拒绝。
- `ui(id)` 可指向控件模板 ID 或页面 ID；`repeat` 只绑定模板子控件 ID，每行仍通过 `ctx.item`/`ctx.value` 取得当前条目。折叠、滚动、换页不重新执行注册。
- `button route="open:页面"/"close"` 继续走类型化页面栈事务，与 `event` 互斥，不能对它补 JS bind；纯声音按钮保持原规则。
- XML 的 `bind="game.text"` 是数据绑定，JS 的 `.bind(event, callback)` 是回调注册，两者含义不同。

```javascript
function closePage(ctx) { ctx.close(); }
ui('settingsBack').bind('close', closePage);
ui('historyBack').bind('close', closePage);
```

### 37.3 生命周期、虚拟目标与捕获结果

| 旧写法（已移除） | 当前写法 | 语义 |
| --- | --- | --- |
| `on.frameEnter(id, fn)` | `frame(id).enter(fn)` | 幕进入；UI 命令上下文 |
| `on.frameExit(id, fn)` | `frame(id).exit(fn)` | 幕离开；UI 命令上下文 |
| `on.screenOpen(id, fn)` | `ui(id).bind('screen.open', fn)` | 只接受页面目标 |
| `on.screenClose(id, fn)` | `ui(id).bind('screen.close', fn)` | 只接受页面目标 |
| `on.ui('game.enter', fn)` | `ui('$game').bind('game.enter', fn)` | 全局进入钩子，保留原白名单 |
| `on.ui(resultEvent, fn)`（捕获结果） | `ui('$host').bind(resultEvent, fn)` | 捕获结果回调 |

```javascript
frame('intro').exit(ctx => ctx.open('story-intro'));
ui('story-intro').bind('screen.open', ctx => ctx.sequence('story.intro'));
ui('story-intro').bind('screen.close', ctx => ctx.resume());
ui('$game').bind('game.enter', ctx => {
  if (ctx.game.cgId) ctx.unlock(ctx.game.cgId);
});
ui('captureAdvance').bind('capture-advance', ctx => {
  ctx.captureShortcut('advance', 'shortcut-captured');
});
ui('$host').bind('shortcut-captured', ctx => ctx.setUi('shortcutAdvanceDraft', ctx.value));
```

- `$game`、`$host` 是固定虚拟目标，不是 XML 节点、页面或变量；真实 UI 中禁止使用这两个 ID（否则报错），也不能新增其他虚拟目标。
- `$game` 只接受 `game.enter`；`$host` 只接受非保留的捕获结果事件名（`ctx.captureShortcut(action, resultEvent)` 的目标由宿主在 `$host` 查找，不搜索所有控件）。
- `game.enter`、`screen.open`、`screen.close` 及 `frame.enter:`/`frame.exit:`/`screen.open:`/`screen.close:` 保留编码不能作为普通 XML 事件或普通控件目标的事件名。
- `frame.enter/exit` **不是**剧情 set/add 上下文：它们只获得 UI 命令上下文，不能借新入口写剧情变量。`screen.open/close` 不能由普通 UI 事件伪造触发。
- 顺序、首幕不自动进入、Controller 先验证、末幕手动退出一次、自循环进入、同幕续接不重复进入、页面回调上下顺序、历史不触发生命周期全部沿用既有实现；演出 finish 仍只有 return/open/close 三种路由。

### 37.4 注册隔离与命名

- `frame`、`ui` 是不可写、不可配置的全局函数，选择器对象冻结；回调执行期间再次注册或以任何方式替换全局入口都失败，且不产生部分命令。
- 回调仍然每次在隔离 QuickJS 中执行：没有 Date、随机、异步任务或宿主 IO；脚本总字节 1MiB、16MiB 堆、256KiB 栈、200ms/10000 次中断、剧情更新 128 条、UI 命令 32 条、生命周期派发 16 次等预算不变。
- 注册位置来自作者调用行（`at <eval> (scripts/ui.js:4:28)`），装配错误同时给出 JS 位置与相关 XML 位置。

### 37.5 迁移清单

1. 剧情：把 `on.advance/on.choice/on.end` 逐条改为 `frame(id)...`，回调与原参数顺序保持一致。
2. 界面：为每个 `on.ui(event, fn)` 找到声明该 event 的控件或页面 ID，改写为 `ui('控件ID').bind(event, fn)`；同名事件现在必须逐目标绑定，共享逻辑抽成函数复用。
3. 生命周期：`on.frameEnter/Exit` → `frame(id).enter/exit`；`on.screenOpen/Close` → `ui(页面ID).bind('screen.open'/'screen.close')`。
4. `on.ui('game.enter')` → `ui('$game').bind('game.enter')`；捕获结果 → `ui('$host').bind(resultEvent, fn)`。
5. 用 `cargo run --locked -- --project <工程> --check` 复检；旧脚本会被拒绝，必须完成迁移后才能打包。KPAK 1 的容器兼容不代表旧 JS 内容兼容。

demo（`games/demo/scripts/story.js`、`ui.js`、`endings.js`）与国际化示例已迁移到新语法，可作为对照。story 8/UI 3/save 9/profile 1/read 1/KPAK 1、2 与 content-version=11 不变；新语法不写入存档，纯机械迁移不改变历史重放结果。实际验证见 [验收记录](js-api-redesign-validation.zh-cn.md)。