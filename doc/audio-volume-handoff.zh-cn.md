# 游戏音量功能交接（总音量待实现）

日期：2026-10-31。当前已实现版本为 v0.11.2；本功能属于 v0.11.3 待实现规划，版本规划入口见根目录 [PLAN.md](../PLAN.md)，技术方案见 [版本计划](v0.11.3-implementation-plan.zh-cn.md)。本轮只核查和编写文档，不实现 master_volume。

## 1. 已核实的接口与用法

当前可运行的独立调节用法如下，值均为 0–1；事件定义与 XML event 关联，沿用当前 event.register API：

```xml
<slider id="musicVolume" x="40" y="100" width="560" height="36"
        value="settings.bgm_volume" min="0" max="1" event="music-volume" />
```

```javascript
event.register("music-volume", ctx => ctx.setting("bgm_volume", ctx.value));
event.register("voice-volume", ctx => ctx.setting("voice_volume", ctx.value));
event.register("se-volume", ctx => ctx.setting("se_volume", ctx.value));
event.register("video-volume", ctx => ctx.setting("video_volume", ctx.value));
event.register("mute", ctx => ctx.setting("muted", ctx.value)); // toggle 提供 bool
```

其余滑条将 value 分别绑定 settings.voice_volume、settings.se_volume、settings.video_volume。demo 已有完整示例，无需为独立音量新增宿主命令。UI 音效的角色/资源开关另存 ui-audio.json，只控制候选过滤，不等于音量设置。

## 2. 代码接入位置

| 文件 | 核查结果 / 下一步 |
| --- | --- |
| src/settings.rs | Settings 及默认值、有限值/范围检查、严格反序列化、原子保存；新增必需 master_volume，默认 1.0 |
| src/interface.rs | binding_kind 的 settings 数值白名单；加入 master_volume，保持类型及未知路径拒绝 |
| src/interface/host.rs | Command::Setting 的明确名称分派与 Settings 验证；加入 master_volume，不另开命令通道 |
| src/interface/data.rs、src/script.rs | 复核 Settings 的绑定数据与事件快照序列化路径，确保 XML/JS 均读到新字段且无第二份白名单遗漏 |
| src/audio.rs | AudioEngine::configure、configure_ui、configure_ui_video 当前都直接使用通道音量；统一乘 master，特别防止 UI 视频后续 configure 覆盖前一次设置 |
| src/app.rs、src/app/custom.rs | App 音频配置及 Operation::Settings 写盘/提交顺序；确认成功写盘及候选准备之后才影响 Sink |
| games/demo/ui/settings.xml | 当前含文字速度、自动播放、四通道、总静音与其他开关；重排布局后加入总音量 |
| games/demo/scripts/ui.js | 新增事件，例如 master-volume → ctx.setting("master_volume", ctx.value) |
| doc/xml-js-api.zh-cn.md | 第 9 节绑定表及设置说明、第 13 节持久化说明，落实后加入总音量并标明旧设置拒绝 |

可用一个小型纯增益函数防止三条播放路径语义不一致，不引入通用混音框架。所有已有音频来源必须覆盖；没有设备也要成功保存和读取偏好。保留先过滤后随机、全排除静音和最多四个 UI 音效 Sink 的既有规则。

## 3. 拟议作者写法（当前不可运行）

```xml
<slider id="masterVolume" x="40" y="100" width="560" height="36"
        value="settings.master_volume" min="0" max="1" event="master-volume" />
```

```javascript
event.register("master-volume", ctx => ctx.setting("master_volume", ctx.value));
```

例如 bgm=0.4、voice=0.8、se=0.6、video=0.5，master=0.5 时有效增益依次为 0.2、0.4、0.3、0.25，UI 音效为 0.3。master 恢复 1 时四个分项值仍为原值。界面百分比显示如有需要通过既有标签/绑定组合实现，不新增音频能力。

## 4. 事务与持久化边界

沿用收集命令 → 准备并验证全部候选 → settings.json 原子写入 → 一次提交 UI/Settings → App 更新 Sink。写盘前不能试调实际声音；不允许总音量改变绕过其他坏命令或混合剧情/其他持久 IO。若需拖动预览，使用既有滑条提交节奏，不在绘制回调做 IO，不引入第二套预览偏好状态。

严格处理 bool/string/null、非有限值及越界数字；验证应在 f64 转 f32 前确认输入范围，避免略大于 1 或略小于 0 的输入经舍入变合法。此次只扩展音量相关校验，不泛化修改无关设置。

旧 settings.json 缺少必需字段时提示并按当前默认回退规则处理，不恢复历史兼容解析器。普通存读档、重开、inactive 隐藏续接均不能覆盖独立设置；活动隐藏交接禁止修改设置。工程热更不得重置当前音量或重播声音；发布 KPAK 不携带玩家 settings.json。

## 5. 必要验证与完成标准（待执行）

1. 增益：覆盖 master=0/1、乘积、静音/取消静音、分项为零及总音量往返后分项值不变；确认 UI 音效与视频也使用同一规则。
2. 契约/回滚：真实 JS 事件与 XML 数值绑定接通；拒绝错误类型、非有限/越界输入；同批有效总音量加无效命令整批失败；原子替换失败后文件、UI、Settings 与声音都不变。优先扩展 tests/ui_v04.rs 和 tests/save_roundtrip.rs，避免重复穷举无关功能。
3. 持久化：完整新设置往返、缺新字段旧设置及未知字段拒绝；完全退出重启保留 master/分项/muted；普通读档和重开不覆盖。优先复用现有测试夹具。
4. 播放：沿用 tests/audio.rs、tests/ui_presentation.rs 中相关路径，验证调整不触发 enter/重建/重播；新声音首次播放即用已提交增益。纯计算/解码检查不依赖设备，真实 Sink 行为另验。
5. GUI：实际设置页中文、16:9、总音量及分项滑条可用、无重叠；菜单/失焦/调试暂停保持原行为；视频/角色 UI 音效过滤无回归。UI 演出期间若设置入口被交接规则阻挡，先改设置再启动视频，不为验证绕过事务限制。
6. 设备与听感：分别记录真实设备流是否成功，以及音乐/语音/剧情音效/UI 音效/视频的主观音量变化、静音和恢复；没有实测不能以纯测试代替。

实现后运行 cargo fmt --check、cargo clippy --locked --all-targets -- -D warnings、与改动相关的 locked 测试、cargo run --locked -- --check；两格式 demo 包独立重载，确认 XML/JS 与新绑定保留且不携带偏好。最后执行 cargo build --locked --release --bins，报告版本及两个绝对路径。GUI、设备、听感未做项逐项记录；不宣称本轮计划交付已经验收上述功能。
