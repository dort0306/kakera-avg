# JavaScript 触发器与内容来源改造

制定日期：2026-10-23。依据本轮需求，将运行样例从 flow.xml 规则改为 JavaScript 触发器与回调，正式接入 QuickJS。此前“本版不嵌入”的阶段决定由本轮明确请求更新；历史 v0.1–v0.3 契约继续保留和测试。

## 作者接口

新增 story schema 4：根声明 entry，scripts 列表引用外部 JS；台词、选项文字、资源及类型化变量继续放 XML。脚本按列表顺序在运行时读取，支持多个文件与共享辅助函数，不在编译时嵌入游戏内容。加载后使用内存中的固定内容版本；编辑文件后重启生效，不在绘制或每次点击时读盘。

```javascript
on.advance("opening", () => "question");
on.choice("question", "accept", ctx => {
  ctx.add("trust", 1);
  ctx.set("met", true);
  ctx.set("mood", "期待");
  return "walk";
}, ctx => ctx.vars.trust < 3n);
on.advance("decision", ctx => {
  if (ctx.vars.invited && ctx.vars.trust >= 2n) {
    ctx.set("mood", "开心");
    return "accept";
  }
  return "refuse";
});
on.end("accept");
```

每场景只能注册 advance、choice 或 end 一种类型；choice 的 option 必须与 XML 一一对应。回调同步返回有效目标 ID；when 可选且必须返回 bool。未知场景/选项、重复注册、漏注册、异常、异步返回和非法目标报错。动态目标在实际执行时检查，不能声称静态穷举了任意 JS 的分支。

ctx.vars 为只读值视图；int 全部使用 BigInt 防止 i64 精度丢失，bool/string 保持原类型。ctx.set/add 收集有序更新，add 接受 BigInt 或安全整数 Number。回调内后续读取可见本次草稿的更新；Rust 对每步类型、i64 溢出、范围、字符串长度及最大 128 次更新重新校验，失败不提交任何状态。条件回调禁止更新。

## 解耦与执行

- model 只保存数据、路由类型元数据及纯逻辑接口，不持有 QuickJS 对象；controller 只依赖 model，通过注入接口取得计划，提交仍由纯 Rust 执行。
- script 模块负责 QuickJS 初始化、注册、求值、错误位置和边界转换。每次求值重新建立隔离的 JS 上下文，闭包/全局修改不跨事件保存；所有可持久状态必须在声明变量中。
- 限制 JS 堆、栈和执行预算；不给文件、网络、系统、窗口或原生模块能力，禁用时钟、随机等不适合重放的入口。不引入异步执行或通用消息总线。
- App 在输入/进入、重开或读档成功的提交边界计算可用选项并缓存；绘制只读缓存，不执行 JS。错误通过现有提示展示，当前位置、变量、历史和音频保持原状态。
- 历史重放与检查点推进使用同一个逻辑接口，保存格式仍是纯 schema 3 数据。脚本修改若改变剧情语义必须提升 content-version；样例升为 3，旧样例存档明确拒绝，不尝试迁移解释器执行现场。

## 内容与打包接口

ContentSource 提供按游戏内相对路径读取字节的接口；DirectorySource 实现目录读取并限制路径及符号链接逃逸。XML、JS、图片、字体和音频均通过此入口；图像与音频解码从字节进行。内存来源用于验证该边界，未来资源包只需提供相同接口，无需让 Model、回调或解码器了解压缩包路径。不实现资源包格式、解压、加密、热重载或插件框架。

## 验证与交付

验证真实 QuickJS 分支、条件、更新回滚、隐藏选项伪造、全局隔离、异常/语法位置、循环超时、内存/栈边界、i64 精度、历史裁剪重放与跨进程读档；验证运行时改文件后重启生效、内存来源和目录来源结果一致、路径逃逸拒绝、旧 XML 规则回归。运行 fmt、Clippy、完整测试、--check、release 与便携包校验；实际窗口复查选项解锁、分支、历史及跨重启恢复。音频听感单独报告。

## 实施结果

2026-10-23 完成，crate 版本 0.3.1：

- 接入固定 rquickjs 0.14.0，其随包 C 引擎为 QuickJS-NG（QuickJS 分支），MSVC 实际构建通过；未接异步、原生模块或宿主 IO。新增许可证放 licenses，随发布包复制。
- 新增 `content.rs`、`script.rs` 和引擎内部 `script/bridge.js`。游戏源码从 ContentSource 读取；引擎接口实现自身随 Rust 编译，游戏 XML/JS 不嵌入二进制。
- `model::Logic` 注入声明式或 QuickJS 执行器；controller 仍仅依赖 model。目标选项条件也在提交前检查，异常保留旧场景、变量、历史、逐字与音频。
- 每个条件单独建立隔离上下文，避免一个条件的 JS 全局修改影响其他条件，保证可见性和实际选择检查一致。未知变量读取/写入、错类型返回、异步任务和动态非法目标均拒绝。
- 预算为每次求值 200ms 或 10000 次中断检查、JS 堆 16MiB、栈 256KiB；32 文件/源码总量 1MiB、128 项更新。预算包括源码注册；历史越长重放越慢，不声称整批重放只有 200ms。
- Schema 4 XML 只做显示数据、声明和外部脚本清单；样例使用 scripts/story.js 注册与 scripts/endings.js 辅助函数。旧 flow.xml 留作历史参考，不被 schema 4 读取，不进入新便携包。
- v0.3 XML 样例另存 doc/v0.3-example，旧原始文档与 v0.3 验收结果保留。当前样例 content-version 3，schema 3 存档仍是纯状态，拒绝旧内容版本。
- 自动测试共 56 项，其中新增 12 项覆盖真实 QuickJS 路由/隔离/资源限额/错误位置/i64/历史裁剪重放/运行时来源替换及全部素材内存来源解码。没有通过 mock 假装运行 JS。
- fmt、Clippy（-D warnings）、完整 locked/offline 测试、--check 与 release 构建通过。沙箱增量编译硬链接不可用时工具链回退复制，不是代码告警。
- 实际窗口已检查中文、邀请解锁、回调结尾、变量历史、重开、跨进程恢复、逐字门控、菜单暂停和 16:9。详见 [本轮窗口记录](js-trigger-ui-validation.zh-cn.md)。异常与另外两结尾由自动测试覆盖，实际听感、热拔插及其他系统未验收。
- tools/package.ps1 输出 target/package/kakera-avg-v0.3.1，携带 XML、外部 JS、素材、字体与许可证；在源码目录之外执行包内 --check 校验。不实现资源包格式或热重载，仅提供已接通且经内存来源验证的读取接口。

绑定接口依据 [rquickjs 官方文档](https://docs.rs/rquickjs/0.14.0/rquickjs/) 及本机下载的对应版本源码核对，依赖版本完整保存在 Cargo.lock。
