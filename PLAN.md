# 版本规划

本文件是版本规划入口。当前已实现应用版本为 **v0.11.3**；未实现内容不改变当前 API、数据契约或发布版本。

## 版本状态

| 版本 | 状态 | 范围 |
| --- | --- | --- |
| v0.11.3 | 已实现、已验收 | 总音量与 2D 粒子效果；见 [v0.11.3 验收](doc/v0.11.3-validation.zh-cn.md) |
| v0.11.2 | 已实现、已验收 | UI 组合标签与 repeat 通用分页；见 [v0.11.2 验收](doc/v0.11.2-validation.zh-cn.md) |

应用版本只在功能实际实现并交付时递增；修订规划文档或整理 README 不改变应用版本与发布脚本默认目录。

## v0.11.3 已实现

- [x] **总音量**：`settings.master_volume` / `ctx.setting("master_volume", value)`，与音乐、语音、音效、视频分项相乘并覆盖 UI 音效；保留四项独立音量与总静音。契约见 [API 第 39.1 节](doc/xml-js-api.zh-cn.md)。
- [x] **2D 粒子效果**：UI XML 声明 `<particles>` 发射器与参数，JS 受约束地 `start/stop/clear/burst`，提供雨雪、花瓣、光点等演出。契约见 [API 第 39 节](doc/xml-js-api.zh-cn.md)。

范围与不做项（保持）：

- 总音量不新增 UI 音效独立滑条、角色专属音量、混音总线、淡入淡出或分贝曲线，不开放任意脚本音频播放。
- 粒子只做 2D 图片粒子，不做碰撞/物理流体、3D、任意 shader、脚本 tick 或逐粒子写入。

实现依据与结果见 [v0.11.3 实现计划](doc/v0.11.3-implementation-plan.zh-cn.md) 与 [v0.11.3 验收](doc/v0.11.3-validation.zh-cn.md)；story 8 / UI 3 / save 9 / profile 1 / read 1 / 隐藏槽 1 / KPAK 1、2 与 demo content-version=11 均未改变。

## 后续候选（未排期）

以下只是候选项，未排期，不承诺版本或接口：

- **编辑器与 AI 辅助**：已完成。VS Code 可视分镜编辑器（Custom Editor/Webview/DAP 适配）与可配置 Chat Completions 兼容 AI 助手已在独立仓库 `kakera-editor-vscode` 落地，本引擎只按既有调试/发布接口配合；历史规划文档归档在 [doc/archive/editor-v0.8.2](doc/archive/editor-v0.8.2/README.zh-cn.md)，引擎侧不再新增编辑器专用能力。
- **渲染与平台**：Vulkan/Metal 原生后端只有接口，当前只交付 Windows x64 的 D3D11 + CPU fallback。
- **国际化**：RTL、复杂文本塑形、逐语言图片/视频/模型与视频多音轨未支持。
- **媒体**：网络流、包内直接 seek、精确音画同步未支持；Live2D physics3 求解、完整动作混合与自动口型未支持。
- **粒子扩展**：碰撞、物理流体、3D、自定义 shader、剧情层（随台词切换）粒子未支持。

## 规划规则

- 未实现的下版本规划在 PLAN.md 里；各版本实现计划与验收记录仍放 `doc/`，不把规划写成当前 API。
- 规划修订不递增应用版本，也不提前改变 story/UI/save/content-version。
- 完成标准以对应验证记录为准；未实机执行的 GUI、DPI、设备与听感项不得算作通过。
