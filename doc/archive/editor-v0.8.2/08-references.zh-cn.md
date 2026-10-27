# 08 依据、API 文档与版本锁定

核对日期：2026-10-27。只使用本仓库内容和模型/推理项目官方或维护者的一手资料。链接指向可变化页面，实施时必须补齐不可变模型 revision 与推理二进制哈希；本文没有安装或运行这些依赖。

## 1. 仓库内现行手册

| 文档 | 阅读重点 |
| --- | --- |
| [XML/JS API](../../xml-js-api.zh-cn.md) | 第 4–11 节基本内容与脚本；第 14 节预算；17–19 媒体/动作；20–21 工程/章节；26–28 点九、图集、名字与标题 |
| [工程与发布指南](../../project-release-pack-guide.zh-cn.md) | 当前参数、发布/Core 搜索边界、打包收集规则 |
| [KPAK 1/2 格式](../../kpak-format.zh-cn.md) | 流式、索引/块校验、预算与失败保留 |
| [v0.8.2 策略](../../v0.8.2-implementation-plan.zh-cn.md) / [验收](../../v0.8.2-validation.zh-cn.md) | 图集/点九/UUID 的实现边界和已有证据 |
| [标题策略](../../window-title-implementation-plan.zh-cn.md) / [验收](../../window-title-validation.zh-cn.md) | UI title/title-bind 与实际窗口同步 |
| [v0.8.1 策略](../../v0.8.1-implementation-plan.zh-cn.md) / [验收](../../v0.8.1-validation.zh-cn.md) | 大内容快照、KPAK 2 和实际大包记录 |
| [v0.8 策略](../../v0.8-implementation-plan.zh-cn.md) / [验收](../../v0.8-validation.zh-cn.md) | 窗口与单一内容契约 |
| [原始架构依据](../../rust-mvc-plan.zh-cn.md) / [JS 计划](../../js-trigger-implementation-plan.zh-cn.md) | 分层历史，只理解设计动机，不恢复旧 schema/flow |
| [现行 demo](../../../games/demo/story.xml) / [UI](../../../games/demo/ui.xml) / [脚本](../../../games/demo/scripts/ui.js) | 实际资源/多文件/交互，不从历史文档示例猜资源齐全 |

当前库入口及源码定位见 [04](04-current-engine-api.zh-cn.md)。README 中记录的工作区基线包含暂存标题改动，开发冻结时需生成完整 source manifest。

## 2. 官方模型与推理 API

| 来源 | 支持本规划的事实/用途 | 使用范围 |
| --- | --- | --- |
| [Qwen3.8-27B 官方模型卡](https://huggingface.co/Qwen/Qwen3.8-27B) | 精确型号、许可证、图文能力、上下文与服务参数说明 | 模型身份和适配器，不能直接推出特定 Windows 性能 |
| [Qwen3.8 官方仓库](https://github.com/QwenLM/Qwen3.8) | 发布/集成入口、推理控制说明和官方模型链接 | 获取实施时的兼容说明；不把原版 Qwen3 参数套用到 3.8 |
| [ggml-org 的 Qwen3.8-27B-GGUF](https://huggingface.co/ggml-org/Qwen3.8-27B-GGUF) | 维护者转换模型与量化入口 | 原生 Windows 候选模型来源，仍要核对完整视觉组件和哈希 |
| [llama.cpp server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md) | 本地服务、聊天消息、流响应、结构输出、运行参数 | 适配器只开放最小推理面，不开放工具/模型自带服务控制 |
| [llama.cpp 多模态](https://github.com/ggml-org/llama.cpp/blob/master/docs/multimodal.md) | 文本模型与视觉组件的配套方式 | 多模态能力探测和图像输入测试 |
| [llama.cpp GBNF/JSON schema](https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md) | 结构化输出约束及限制 | 只约束格式，业务/资源/类型另检 |
| [vLLM v0.17 GPU 安装](https://docs.vllm.ai/en/v0.17.0/getting_started/installation/gpu/) | Linux/Windows 部署边界 | 本机 WSL2/Linux 参考后端；此链接不是承诺 0.17 能运行目标 3.8 模型 |

这里的 OpenAI 兼容 API 是本地推理项目的兼容协议，不要求使用云服务或购买外部 API。服务能力依其锁定版本，不从同名接口推断完全兼容。

## 3. 实施前必须补齐的清单

| 项目 | 当前规划值 | E0 需填写/验证 |
| --- | --- | --- |
| 目标模型 | Qwen/Qwen3.8-27B，用户已确认 | 官方不可变 revision、许可证文件、语言与视觉完整文件集 |
| GGUF 候选 | ggml-org/Qwen3.8-27B-GGUF，优先测 Q4_K_M 档位 | 实际存在文件的名称/哈希、转换来源、视觉组件、中文质量 |
| 推理后端 | Windows llama.cpp 首选候选 | 精确 release/commit、CPU/CUDA/Vulkan 等实际选用构建、驱动要求与视觉支持 |
| 参考后端 | 本机 WSL2/Linux vLLM | 支持目标模型的真实版本/镜像摘要，不采用旧文档版本作兼容保证 |
| 编辑器 GUI | Rust/egui，沿用当前依赖优先 | 编辑器独立窗及播放器外部预览原型 |
| JS 往返解析 | 待 E0 选型 | BigInt/源位置/未知代码保留/许可证与成本 |
| 引擎/模板 | 当前 0.8.2 + 标题工作区基线 | 二进制与源指纹、XML/UI 模板版本、适配测试 |
| 调试协议 | 拟议 KakeraEditorDebug v1 | 实现后实际 capabilities、传输与受限工程启动约定 |

不为“看起来完整”伪造模型 commit、驱动号、token/s、显存结果或现成编辑器端点。实施证据需记录失败案例，不只记录一次成功截图。

## 4. 后续维护

主 API 手册是现行运行契约，本目录第四篇是编辑器所需摘要；更改运行接口时应同步两者。第五/第六篇是待实现设计，开发后逐项标为已实现并链接真实源码/测试，不能整体改一句“已完成”却保留不存在的命令。

模型与推理后端升级独立于游戏内容格式。引擎 story/UI/save 或资源预算变化时，更新能力清单、编译目标和迁移提示；不得为编辑器加入历史兼容解析器来绕过当前单一契约。
