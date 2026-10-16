<div align="center">

# Mist

**macOS にインスパイアされた Hyprland 用デスクトップシェル。Rust で書かれ、CPU で描画します。**

バー、Dock、Dynamic Island、デスクトップウィジェット、Spotlight 風の検索、コントロールセンター、通知、
ロック画面、polkit エージェントを、GPU コンテキストなしの約 50 MB の 1 プロセスで動かします。

[English](../../README.md) · [Polski](README.pl.md) · [Deutsch](README.de.md) ·
[Español](README.es.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

![ライセンス: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue)
![Rust](https://img.shields.io/badge/rust-2024-orange)
![Wayland](https://img.shields.io/badge/wayland-Hyprland-58e1ff)

![Mist のデスクトップ](../img/desktop-en.webp)

<sub>オフスクリーン描画（`mist-ui --desk-png`、`--dock-png`、`--png`、`mist-wall-bin --png`）、デモデータ使用。
実際のセッションでは、ガラスの下を Hyprland がぼかします。</sub>

</div>

## なぜ

多くの Hyprland 用シェルは Qt Quick（Quickshell）、GTK + GJS（AGS）、または WebView の上で動きます。
どれも OpenGL コンテキストを開き、NVIDIA では GL コンテキストごとにドライバがマップされるため、最初の
1 ピクセルを描く前に **1 プロセスあたり約 100〜160 MB** を使います。Mist は Quickshell の設定
（428 MB、CPU 9.5 %）として始まり、画面ごとに書き直されました。

- **CPU で描くサーフェス。** `mist-ui` は [tiny-skia] と [swash] で `wl_shm` バッファに描画し、
  Hyprland が合成とぼかしを行います。再描画は変化した領域だけです。
- **イベント駆動。** データは Hyprland ソケット、D-Bus、`pactl subscribe`、inotify、常駐する `mistd`
  バックエンドから届きます。画面が静止している間はタイマーによるポーリングをせず、非表示のサーフェスは
  何もしません。
- **GPU プロセスは 1 つだけ、必要なときだけ。** `mist-wall`（EGL、GLSL 150）がアニメーション壁紙と天気の
  空を描きます。写真の壁紙では終了し、写真は `mist-ui` が描きます。

作者のマシンでの結果：**シェル全体で PSS 約 87〜106 MB**、音楽再生中で 1 コアの 0.6 %。
詳細は [docs/BENCHMARKS.md](../BENCHMARKS.md)（英語）。

Ryzen 7 5700X + RTX 4060、Hyprland 0.56.2、シェル全体アイドル時、[bench/measure.sh](../../bench/measure.sh)。

| シェル | 技術 | PSS | アイドル CPU | GL コンテキスト |
|---|---|---|---|---|
| **Mist native** | Rust, CPU, `wl_shm` | **87–106 MB** | 0.6 % | `mist-wall` |
| Mist native, `mist-ui` 単体 | | 36–50 MB | 0.6 % | – |
| Noctalia v5 | C++, cairo + EGL | 122 MB | 0.07 % | ✓ |
| 空の Quickshell（`PanelWindow` 1 つ） | Qt Quick | 124 MB | ~0 % | ✓ |
| Quickshell 版 Mist（旧 QML） | Qt Quick + QML | 428 MB | 9.5 % | ✓ |

## 機能

| | |
|---|---|
| **トップバー** | アクティブなアプリとタイトル、システムトレイ（StatusNotifierItem ホスト + dbusmenu）、ネットワーク、Bluetooth、音声、CPU / メモリ、時計、カレンダー |
| **Dock** | 固定・起動中のアプリ、拡大表示、ライブサムネイル付きウィンドウ選択（`hyprland_toplevel_export_v1`）、アクティブなアプリをクリックで最小化 |
| **Dynamic Island** | メディア（MPRIS）とイコライザー、タイマー、着信（KDE Connect / ADB）、通知、音量 |
| **デスクトップ** | 時計、日の出 / 日の入り、ガラスのカード（天気、Garmin のコンディション、ランニング計画）、ウィジェットごとの設定シート |
| **パネル** | Spotlight 風検索、スタート、Cmd+Tab スイッチャー、コントロールセンター、通知センター、クリップボード履歴、絵文字、セッションメニュー、壁紙ギャラリー、スクリーンショットツールバー |
| **システム** | 通知サーバー、polkit エージェント、ロック（`ext-session-lock-v1` + PAM）、アイドル（`ext-idle-notify-v1`、logind）、Windows 11 風のスナップ |
| **壁紙** | 写真（CPU）、またはリアルタイム GLSL シーンと実際の天気に合わせた空（`mist-wall`） |
| **mist-comp** *（実験的）* | Smithay ベースの独自コンポジター + グリーター。Hyprland なしで Mist を動かすため |

### 言語

UI は**ポーランド語・英語・ドイツ語・スペイン語**に対応。「デスクトップをカスタマイズ」（デスクトップの時計を右クリック）
→ 外観 → 言語、または `MIST_LANG=en`。「自動」は `$LANG` に従い、なければ英語。翻訳表は
[native/assets/i18n.tsv](../../native/assets/i18n.tsv)（1 行 1 文字列、1 列 1 言語）。日本語の列の追加を歓迎します。

### デスクトップカード

- **天気**：[Open-Meteo](https://open-meteo.com)、アカウント不要。カードの背後の空は実際の天気を反映します。
- **Garmin のレディネス**と**ランニング**：ローカル InfluxDB 経由で自分の Garmin Connect データを表示。設定は
  [docs/guides/garmin.md](../guides/garmin.md)（英語）。

## 構成

```
 Hyprland ── layer-shell ──┬── mist-ui      Rust、CPU（tiny-skia + swash）、wl_shm、全サーフェスを 1 プロセスで
                           │      ├── mistd stats | phone | calls | island | clipstore   （イベントバックエンド）
                           │      └── IPC: mist-ui-bin msg <対象> <関数> [引数]
                           └── mist-wall    Rust、EGL / GLSL 150、ライブ壁紙と空のみ
```

| パス | 内容 |
|---|---|
| `native/mist-ui` | シェルの全サーフェス（[native/README.md](../../native/README.md)） |
| `native/mist-wall` | 壁紙用 GPU プロセス |
| `native/mist-comp` | 実験的なコンポジター、セッション、グリーター |
| `native/common` | mist-ui と mist-wall の共有コード（天文計算、プリセット） |
| `scripts/mistd` | バックエンド：システム統計、スマートフォン（KDE Connect、ADB）、通話、クリップボード、ファイルマネージャーの D-Bus |
| `shaders/` | 壁紙と空のシェーダー（`.frag` ソース、mist-wall が読む `.qsb`） |
| `*.qml`、`components/` | 元の Quickshell 版。参考として残しており、現在は起動されません |

## インストール

Mist は作者が毎日使っている環境です：Arch / CachyOS、Lua 設定の Hyprland 0.55 以降、NVIDIA または
Mesa の GPU。コードを読む前提でお使いください。

**ビルド：** `rust`（stable）、`pkgconf`、`libxkbcommon`、`pam`、`wayland`、`mesa`（EGL ヘッダー）。
`mist-comp` には加えて `libinput`、`libseat`、`libdrm`、`mesa`（gbm）、`systemd-libs`（udev）。

**実行時：** `hyprland`、`pipewire` + `pipewire-pulse`（`pactl`、`pw-play`、`parec`）、`wireplumber`
（`wpctl`）、`networkmanager`（`nmcli`）、`wl-clipboard`、`grim`、`slurp`、`polkit`、`qt6-shadertools`
（`qsb`、ライブ壁紙のみ）、`noto-fonts`、`noto-fonts-emoji`。任意：`ddcutil`（DDC/CI で輝度調整）、
`kdeconnect` / `android-tools`（スマートフォン）、`wf-recorder`（画面録画）。

**フォント：** UI は SF Pro（可変フォント `SF-Pro.ttf` を `~/.local/share/fonts/SF-Pro/` に配置）を前提に
デザインされています。Apple は自社プラットフォームでの使用のみを許諾しており、Mist には含まれません。
ない場合は Inter Variable、次に Noto Sans を使います。

```bash
git clone <このリポジトリ> ~/.config/quickshell/mist
```

```bash
~/.config/quickshell/mist/scripts/mistd/install.sh
```

```bash
~/.config/quickshell/mist/native/install.sh
```

クローン先は `~/.config/quickshell/mist` である必要があります（パスはまだ固定です。
[docs/PUBLISHING.md](../PUBLISHING.md) を参照）。`native/install.sh` は `mist-ui-bin`、`mist-wall-bin`、
`mist-comp` を `~/.local/bin` に、初回はガードスクリプト `~/.local/bin/mist-ui` もインストールします。

次に [examples/hyprland/mist.lua](../../examples/hyprland/mist.lua) を Hyprland の設定に追加し（自動起動、
キー割り当て、ぼかしルール）、他のバー、通知デーモン、polkit エージェントを外してください。

## 使い方

| キー | 動作 |
|---|---|
| `Super` | スタート |
| `Super + Space` | Spotlight |
| `Super + Tab` | アプリ切り替え |
| `Super + I` | コントロールセンター |
| `Super + A` | 通知センター |
| `Super + V` | クリップボード履歴 |
| `Super + .` | 絵文字 |
| `Super + L` | ロック |
| `Print`、`Super + Shift + S` | スクリーンショット（範囲） |

すべてスクリプトから操作できます：`mist-ui-bin msg mist toggle spotlight`、`mist-ui-bin msg ui stats`、
`mist-ui-bin msg dock click firefox`。ログ：`~/.cache/mist/ui.log`。設定：`~/.config/mist/`。

## 開発

```bash
cd native && cargo build --release && cargo test --release
```

実行中のセッションに触れずに確認する方法：

| コマンド | 出力 |
|---|---|
| `mist-ui --png out.png '{"app":"Firefox"}'` | トップバーの PNG |
| `mist-ui-bin --desk-png dir` / `--dock-png out.png` | デスクトップのステージ + カード / Dock |
| `mist-ui --test` | HDMI-A-1 の実際のバーの上に 2 本目のバー |
| `mist-wall-bin --png out.ppm dunes 1920 1080` | ライブ壁紙の 1 フレーム |
| `mist-ui-bin msg ui stats` | サーフェスごとの起床回数、描画回数、描画時間 |
| `bench/measure.sh 30 mist-ui-bin mist-wall-bin mistd` | 実行中シェルのアイドル時 PSS / CPU |

`lto = false` を維持してください（Arch の rustc とシステム LLVM の組み合わせは LTO でクラッシュします）。
[CONTRIBUTING.md](../../CONTRIBUTING.md)（英語）も参照。

## 状況

一人の日常環境、モニター 2 台、NVIDIA。機能表の内容はすべてネイティブで毎日使われています。
`mist-comp` は実験的です。公開前に足りないもの（設定可能なパス、Apple 素材の除去）：
[docs/PUBLISHING.md](../PUBLISHING.md)。

## クレジット

[Hyprland]、[Smithay] と [smithay-client-toolkit]、[tiny-skia]、[swash]、[resvg]、[zbus]、
[Tabler Icons]（MIT）。元の QML 版は [Quickshell] で作られ、[Noctalia] は予備と参考にしました。

*Mist は Apple とは無関係です。macOS、Dynamic Island、Spotlight、SF Pro は Apple Inc. の商標であり、
ここではデザインの着想を説明するためにのみ記載しています。*

## ライセンス

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
