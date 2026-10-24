# 内置转场与图片效果落地策略

制定日期：2026-10-24，先于功能代码。依据现有 MVC、v0.2/v0.3、JS 触发器及 v0.4 策略，保持 ContentSource、隔离 QuickJS、严格 XML 与原子提交。

用户参考：https://zhuanlan.zhihu.com/p/447804085 。本次直接访问返回 403，内置浏览器读取超时，未取得正文；以下是本项目自主选择的常用 AVG 效果范围，不声称复现该文章全部 API。

## 1. 范围与接口

本轮作为 v0.5.0，兼容既有 story schema 1–5、UI schema 1 和存档 schema 3。新增 XML 仅在 story schema 5 开放，不改变原剧情 content-version；无效果的旧内容外观保持一致。

- 转场：none、dissolve、fade-black、fade-white、wipe-left/right/up/down、slide-left/right/up/down。frame 可带 `<transition kind="dissolve" duration="600" easing="ease-in-out" />`，duration 为毫秒，0 表示立即完成，最大 5000；缓动 linear/ease-in/ease-out/ease-in-out。
- story 可选 `<effects>`，包含最多 64 项 `<image-effect id="..." ... />`。预设含 filter（none/grayscale/sepia/blur/pixelate）、blur（1–12）、pixel-size（1–64）、opacity（0–1）、tint（十六进制颜色）、scale（0.25–4）、rotation（−360–360 度）、flip-x/y，以及 animation、duration、strength、easing。
- 图片动画：none、fade-in、zoom-in、zoom-out、shake、flash、slide-left/right/up/down，duration 默认 400ms，strength 默认 12（0–128 设计单位）。zoom 使用固定温和缩放，shake 用确定性衰减波形，无随机。
- visual 的 background-effect、portrait 的 effect、UI image/panel 的 effect 引用共享预设；静态样式是最终画面的一部分，加载时展开，读档可恢复。
- UI JS 新增 `ctx.transition(kind, duration=600, easing="ease-in-out")`（仅 game.enter）与 `ctx.animate(target, kind, duration=400, strength=12, easing="ease-in-out")`。target 为非 repeat 内的 scene/image/panel 节点 ID。仍通过纯计划校验，保留 32 条预算；不开放任意时间线/脚本 tick。

## 2. 状态及绘制

Model 只存拥有的效果配置，不含时钟、纹理、解释器或 IO。过滤后的图片在启动阶段从原始 RGBA 生成并上传缓存；绘制不做 CPU 过滤/解码/上传。额外过滤缓存上限 256MiB，禁止因作者预设组合无限增长。

App 在全部剧情/选项/生命周期验证及所需 IO 成功后启动转场，保留旧 Frame 作为呈现快照；失败不更换旧 Session、音频或动画。自循环也是明确进入。新 Session 与音频在成功进入时提交，转场仅控制画面。

转场只作用 scene 画面，HUD、菜单及台词由 XML 控制。转场期间不累计逐字时间，选项不可用；第一次继续/空格跳过转场，再次按原有补全/推进规则处理。菜单/提示/失焦暂停全部效果时间，恢复不补算。读档直接显示最终画面，不恢复或重播过渡；动画指针不入档。重新开始按正常入口呈现。

UI 预设进入动画在场景进入或页面栈变化后重新计时，JS 单次动画按目标替换；不保存 JS 对象、不在绘制执行回调。图片滤镜保持透明度，翻转/缩放/旋转由纹理网格完成，边界始终裁剪。

## 3. 验证与交付

先验证 XML 白名单、引用、范围、JS 权限及整批回滚；纯数学测试验证每个转场首尾/中点、缓动、动画最终回到基准、暂停/跳过/读档与失败保留。合成像素验证灰度/棕褐/模糊/像素化及 alpha；真实 egui 绘制验证裁剪和场景/HUD 分层。

demo 增加可点击效果演示，避免仅靠剧情少量场景观察。运行 fmt、Clippy、完整 locked 测试、--check、release，并实际观察窗口中的转场/滤镜、菜单暂停、跳过、16:9 与跨重启读档。同步 API 手册、README、AGENTS 与发布脚本；无法验证的项目明确列出。暂不实现自定义 shader、遮罩资源转场、任意关键帧或无限循环动画。

## 4. 实施记录

已实现 model/effects 纯配置、loader/effect_xml 严格解析、effects 确定性插值与瞬时播放器、effects/render 纹理网格与场景分层。过滤变体在 PreparedAssets 启动阶段生成，Model/存档无时钟或解释器对象；新 API 沿用 QuickJS 预算及纯计划校验。未新增依赖。

demo 保留七场景/四变量/三结尾与内容版本 3，新增第九个“效果演示”页面，提供过滤预览、全部转场按钮、单次动画、时长滑杆和暂停预览。GUI 的进入、重开、读档均经过原提交边界，动画失败不留下部分状态。

新增七项测试，最终完整回归 74 项通过；fmt、Clippy -D warnings、--check 与 release 通过。Windows 实际检查记录见 [效果验收](effects-ui-validation.zh-cn.md)。API 手册第 17 节、README、AGENTS 与发布脚本已同步；尚不支持自定义 shader/遮罩资源/离屏后处理/关键帧，音频与其他系统未新增验收。

