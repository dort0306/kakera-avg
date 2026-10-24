# Live2D 本地运行依赖

windows-x64/Live2DCubismCore.dll 来自用户提供的 C:/CubismSdkForNative-5-r.5。固定宿主路径供源码启动使用；tools/package.ps1 将 DLL 放到 exe 旁，并复制本目录的 Core 原许可和可再分发文件清单到包内 licenses。不得从游戏 XML/JS 提供的任意路径加载原生库。

完整 SDK 未复制进仓库。模型资源与 SDK/模型条款分别见 games/demo/assets/live2d/Haru 和 games/demo/assets/licenses/LIVE2D-SDK-LICENSE.md。本次仅本地集成/测试，未公开发布；分发要求以所附原许可为准。
