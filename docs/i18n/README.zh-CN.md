<div align="center">

# Mist

**受 macOS 启发的 Hyprland 桌面外壳，用 Rust 编写，在 CPU 上绘制。**

顶栏、程序坞、灵动岛、桌面小组件、Spotlight 风格搜索、控制中心、通知、锁屏和 polkit 代理，
全部在一个约 50 MB 的进程里，不需要 GPU 上下文。

[English](../../README.md) · [Polski](README.pl.md) · [Deutsch](README.de.md) ·
[Español](README.es.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

![许可证: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue)
![Rust](https://img.shields.io/badge/rust-2024-orange)
![Wayland](https://img.shields.io/badge/wayland-Hyprland-58e1ff)

![Mist 桌面](../img/desktop.webp)

<sub>离屏渲染（`mist-ui --desk-png`、`--dock-png`、`--png`、`mist-wall-bin --png`），使用演示数据。
在真实会话中，Hyprland 会对玻璃下方的背景做模糊。</sub>

</div>

## 为什么

大多数 Hyprland 外壳基于 Qt Quick（Quickshell）、GTK + GJS（AGS）或 WebView。它们都会创建 OpenGL
上下文，而在 NVIDIA 上每个 GL 上下文都要映射驱动：在画出第一个像素之前，**每个进程约 100–160 MB**。
Mist 最初是一个 Quickshell 配置（428 MB，9.5 % CPU），之后被逐个界面重写：

- **CPU 绘制的界面。** `mist-ui` 用 [tiny-skia] 和 [swash] 绘制到 `wl_shm` 缓冲区，由 Hyprland
  合成并模糊。只重绘发生变化的区域。
- **事件驱动。** 数据来自 Hyprland 套接字、D-Bus、`pactl subscribe`、inotify 以及常驻的 `mistd`
  后端。屏幕静止时没有任何定时轮询；隐藏的界面不做任何工作。
- **只有一个 GPU 进程，且仅在需要时运行。** `mist-wall`（EGL，GLSL 150）绘制动态壁纸和天气天空。
  使用照片壁纸时它会退出，由 `mist-ui` 自己绘制照片。

在作者的机器上：**整个外壳约 87–106 MB PSS**，播放音乐时占用单核 0.6 %。
详见 [docs/BENCHMARKS.md](../BENCHMARKS.md)（英文）。

## 功能

| | |
|---|---|
| **顶栏** | 当前应用和标题、系统托盘（StatusNotifierItem 宿主 + dbusmenu）、网络、蓝牙、音频、CPU / 内存、时钟、日历 |
| **程序坞** | 固定和运行中的应用、放大效果、带实时缩略图的窗口选择（`hyprland_toplevel_export_v1`），点击当前应用即最小化 |
| **灵动岛** | 媒体（MPRIS）及均衡器动画、计时器、来电（KDE Connect / ADB）、通知、音量 |
| **桌面** | 时钟、日出 / 日落、玻璃卡片（天气、Garmin 状态、跑步计划）、每个小组件的设置面板 |
| **面板** | Spotlight 风格搜索、开始菜单、Cmd+Tab 切换器、控制中心、通知中心、剪贴板历史、表情、会话菜单、壁纸库、截图工具栏 |
| **系统** | 通知服务、polkit 代理、锁屏（`ext-session-lock-v1` + PAM）、空闲（`ext-idle-notify-v1`、logind）、类似 Windows 11 的窗口贴靠 |
| **壁纸** | 照片（CPU）或实时 GLSL 场景和实时天气天空（`mist-wall`） |
| **mist-comp** *（实验性）* | 基于 Smithay 的自有合成器 + 登录界面，可脱离 Hyprland 运行 Mist |

## 架构

```
 Hyprland ── layer-shell ──┬── mist-ui      Rust，CPU（tiny-skia + swash），wl_shm，一个进程承载所有界面
                           │      ├── mistd stats | phone | calls | island | clipstore   （事件后端）
                           │      └── IPC: mist-ui-bin msg <目标> <函数> [参数]
                           └── mist-wall    Rust，EGL / GLSL 150，仅用于动态壁纸和天空
```

| 路径 | 内容 |
|---|---|
| `native/mist-ui` | 所有外壳界面（[native/README.md](../../native/README.md)） |
| `native/mist-wall` | 壁纸 GPU 进程 |
| `native/mist-comp` | 实验性合成器、会话和登录界面 |
| `native/common` | mist-ui 与 mist-wall 共用的代码（天文、预设） |
| `scripts/mistd` | 后端：系统状态、手机（KDE Connect、ADB）、来电、剪贴板存储、文件管理器 D-Bus |
| `shaders/` | 壁纸和天空着色器（`.frag` 源码，mist-wall 读取的 `.qsb` 包） |
| `*.qml`、`components/` | 最初的 Quickshell 版本，保留作参考；已不再启动 |

## 安装

Mist 是作者个人的日常桌面：Arch / CachyOS，Hyprland 0.55+（Lua 配置），NVIDIA 或 Mesa 显卡。
需要有阅读代码的准备。

**构建依赖：** `rust`（stable）、`pkgconf`、`libxkbcommon`、`pam`、`wayland`、`mesa`（EGL 头文件）；
`mist-comp` 另需 `libinput`、`libseat`、`libdrm`、`mesa`（gbm）、`systemd-libs`（udev）。

**运行依赖：** `hyprland`、`pipewire` + `pipewire-pulse`（`pactl`、`pw-play`、`parec`）、`wireplumber`
（`wpctl`）、`networkmanager`（`nmcli`）、`wl-clipboard`、`grim`、`slurp`、`polkit`、`qt6-shadertools`
（`qsb`，仅动态壁纸需要）、`noto-fonts`、`noto-fonts-emoji`。可选：`ddcutil`（DDC/CI 亮度）、
`kdeconnect` / `android-tools`（手机）、`wf-recorder`（录屏）。

**字体：** 界面按 SF Pro 设计（可变字体 `SF-Pro.ttf`，放在 `~/.local/share/fonts/SF-Pro/`）。Apple
仅授权在其自家平台使用，Mist 不附带。没有它时 Mist 使用 Inter Variable，其次 Noto Sans。

```bash
git clone <本仓库> ~/.config/quickshell/mist
```

```bash
~/.config/quickshell/mist/scripts/mistd/install.sh
```

```bash
~/.config/quickshell/mist/native/install.sh
```

克隆目录必须是 `~/.config/quickshell/mist`（路径目前仍是固定的，见
[docs/PUBLISHING.md](../PUBLISHING.md)）。`native/install.sh` 会把 `mist-ui-bin`、`mist-wall-bin`、
`mist-comp` 安装到 `~/.local/bin`，首次运行时还会安装守护脚本 `~/.local/bin/mist-ui`。

然后把 [examples/hyprland/mist.lua](../../examples/hyprland/mist.lua) 加入 Hyprland 配置（自启动、
快捷键、模糊规则），并移除其他状态栏、通知守护进程和 polkit 代理。

## 使用

| 按键 | 操作 |
|---|---|
| `Super` | 开始 |
| `Super + 空格` | Spotlight |
| `Super + Tab` | 应用切换 |
| `Super + I` | 控制中心 |
| `Super + A` | 通知中心 |
| `Super + V` | 剪贴板历史 |
| `Super + .` | 表情 |
| `Super + L` | 锁屏 |
| `Print`、`Super + Shift + S` | 截图（区域） |

一切都可脚本化：`mist-ui-bin msg mist toggle spotlight`、`mist-ui-bin msg ui stats`、
`mist-ui-bin msg dock click firefox`。日志：`~/.cache/mist/ui.log`。设置：`~/.config/mist/`。

## 开发

```bash
cd native && cargo build --release && cargo test --release
```

不影响当前会话的检查方式：

| 命令 | 输出 |
|---|---|
| `mist-ui --png out.png '{"app":"Firefox"}'` | 顶栏 PNG |
| `mist-ui-bin --desk-png dir` / `--dock-png out.png` | 桌面舞台 + 卡片 / 程序坞 |
| `mist-ui --test` | 在 HDMI-A-1 上叠加第二个顶栏 |
| `mist-wall-bin --png out.ppm dunes 1920 1080` | 动态壁纸的一帧 |
| `mist-ui-bin msg ui stats` | 每个界面的唤醒次数、绘制次数和绘制耗时 |
| `bench/measure.sh 30 mist-ui-bin mist-wall-bin mistd` | 运行中外壳的空闲 PSS / CPU |

请保持 `lto = false`（Arch 的 rustc 搭配系统 LLVM 在 LTO 下会崩溃）。见
[CONTRIBUTING.md](../../CONTRIBUTING.md)（英文）。

## 状态

一个人的日常桌面，双显示器，NVIDIA。功能表中的所有内容都是原生实现并每天使用；`mist-comp` 仍为实验性。
界面文字目前是波兰语（公开发布前缺少的内容见 [docs/PUBLISHING.md](../PUBLISHING.md)）。

## 致谢

[Hyprland]、[Smithay] 和 [smithay-client-toolkit]、[tiny-skia]、[swash]、[resvg]、[zbus]、
[Tabler Icons]（MIT）。最初的 QML 版本基于 [Quickshell]；[Noctalia] 曾作为备用方案和参考。

*Mist 与 Apple 无关。macOS、Dynamic Island、Spotlight 和 SF Pro 是 Apple Inc. 的商标，此处仅用于描述设计灵感。*

## 许可证

[GPL-3.0](../../LICENSE)。

[tiny-skia]: https://github.com/linebender/tiny-skia
[swash]: https://github.com/dfrg/swash
[resvg]: https://github.com/linebender/resvg
[zbus]: https://github.com/dbus2/zbus
[Hyprland]: https://hyprland.org
[Smithay]: https://github.com/Smithay/smithay
[smithay-client-toolkit]: https://github.com/Smithay/client-toolkit
[Tabler Icons]: https://tabler.io/icons
[Quickshell]: https://quickshell.org
[Noctalia]: https://github.com/noctalia-dev/noctalia
