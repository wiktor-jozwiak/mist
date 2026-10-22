<div align="center">

<img src="../img/banner.svg" alt="Mist" width="100%">

**macOS に着想を得た Wayland 用デスクトップシェル。Rust で書かれ、CPU で描画します。**

[![CI](https://img.shields.io/github/actions/workflow/status/wiktor-jozwiak/mist/ci.yml?branch=main&style=flat-square&label=CI)](../../.github/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/wiktor-jozwiak/mist?style=flat-square&color=3b82f6)](https://github.com/wiktor-jozwiak/mist/releases)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-8d8590?style=flat-square)](../../LICENSE)
[![Discord](https://img.shields.io/badge/discord-chat-5865F2?style=flat-square&logo=discord&logoColor=white)](https://discord.gg/4rRabwQTPE)

[English](../../README.md) · [Polski](README.pl.md) · [Deutsch](README.de.md) ·
[Español](README.es.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

<br>

<img src="../img/desktop-en.webp" alt="Mist のデスクトップ：トップバー、時計、天気・コンディション・ランニングのカード、Dock" width="100%">

<table>
  <tr>
    <td width="50%"><img src="../img/island.gif" alt="Dynamic Island：通知、音量、まもなく雨、次の予定" width="100%"></td>
    <td width="50%"><img src="../img/weather.gif" alt="雨の天気カードと、その背後のリアルタイムの空" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>Dynamic Island：通知、音量、1 時間以内の雨、次の予定</sub></td>
    <td align="center"><sub>リアルタイム GLSL の空に重なる天気カード</sub></td>
  </tr>
</table>

</div>

Mist は、バー、Dock、Dynamic Island、デスクトップウィジェット、検索、コントロールセンター、通知、ロック画面、
polkit エージェントをひとつにまとめたデスクトップです。すべてが約 50 MB の単一プロセスで動き、GPU コンテキスト
は一度も開きません。Hyprland の上でも、独自のコンポジター `mist-comp` の上でも動きます。

はじめはアイドル時に 428 MB と CPU 9.5 % を使う Quickshell の設定でした。そこから次の三つの原則で、画面ごとに
書き直されました。

- **CPU で描く。** `mist-ui` は [tiny-skia] と [swash] で `wl_shm` バッファに描き、変わった部分だけを再描画
  します。NVIDIA では OpenGL コンテキストひとつが最初のピクセルの前に 100〜160 MB を使いますが、Mist は
  ひとつも持ちません。
- **イベントを待つ。** データはすべてコンポジターのソケット、D-Bus、`pactl subscribe`、inotify から届きます。
  画面が止まっている間のポーリングはなく、隠れた画面は何もしません。
- **GPU プロセスはひとつだけ、表示するときだけ。** `mist-wall` が動く壁紙と天気の空を描きます。写真の壁紙の
  ときは終了し、写真は `mist-ui` が描きます。

> [!NOTE]
> Mist はひとりの日常のデスクトップです（Arch / CachyOS、デュアルモニター、NVIDIA）。完成したからではなく、
> 動くから公開しています。コードを読む覚悟をしてください。

## 機能

<table>
  <tr>
    <td width="50%"><img src="../img/panels/spotlight.webp" alt="Spotlight 風の検索" width="100%"></td>
    <td width="50%"><img src="../img/panels/settings.webp" alt="設定ウィンドウ" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>Spotlight：アプリ、ファイル、コマンド、絵文字、Web</sub></td>
    <td align="center"><sub>設定：すべての項目をひとつのウィンドウで、すぐに反映</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="../img/panels/control.webp" alt="コントロールセンター" width="100%"></td>
    <td width="50%"><img src="../img/panels/calendar.webp" alt="カレンダー" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>コントロールセンター：ネットワーク、Bluetooth、スマートフォン、サウンド、夜間モード</sub></td>
    <td align="center"><sub>カレンダー：月表示、予定一覧、新しい予定</sub></td>
  </tr>
</table>

| | |
|---|---|
| **トップバー** | 使用中のアプリ、システムトレイ（StatusNotifierItem + dbusmenu）、ネットワーク、Bluetooth、オーディオ、CPU とメモリ、時計 |
| **Dock** | ピン留めと起動中のアプリ、拡大表示、ライブサムネイル付きのウィンドウ選択 |
| **Dynamic Island** | イコライザー付きのメディア、タイマー、着信（KDE Connect または ADB）、通知、音量 |
| **デスクトップ** | 時計とガラスのカード：天気、Garmin のコンディション、ランニング、音楽、カレンダー、システム、世界時計 |
| **パネル** | Spotlight、スタート、アプリ切り替え、コントロールセンター、通知センター、クリップボード履歴、絵文字、セッションメニュー、スクリーンショット |
| **設定** | 言語、天気の地点、壁紙、デスクトップの配置、ウィジェット、夜間モード、ロックとスリープ、通知 |
| **システム** | 通知サーバー、polkit エージェント、ロック画面（PAM）、アイドルとスリープ |
| **アプリ** | Mist Files（あらゆるアプリのファイル選択ダイアログも兼ねる）、Mist Player（libmpv）、システム設定 |
| **スナップ** | Windows 11 風：端や角へドラッグ、緑のボタンか `Super + Z` で Snap Layouts、Snap Assist |
| **壁紙** | 写真、動く GLSL シーン、現在の天気に合わせた空 |

**天気**はアカウント不要の [Open-Meteo](https://open-meteo.com) から：1 時間ごとのグラフ、4 日間、1 時間以内の雨、
太陽と月。**Garmin のコンディション**と**ランニング**は自分の Garmin Connect のデータを表示し、`mistd` がローカルの
SQLite ファイルに同期します。一度ログインするまでカードは表示されません（[ガイド](../guides/garmin.md)）。

UI は**英語、ポーランド語、ドイツ語、スペイン語、フランス語**に対応し、`$LANG` か設定での選択に従います。
言語は [native/assets/i18n](../../native/assets/i18n) の JSON ファイルひとつです。

## ベンチマーク

<p align="center"><img src="../img/memory.svg" alt="アイドル時のメモリ：mist-ui のみ 36〜50 MB、mist-wall 込みの Mist 87〜106 MB、Noctalia v5 122 MB、空の Quickshell 124 MB、Quickshell 版 Mist 428 MB" width="100%"></p>

シェル全体のアイドル時 PSS、Ryzen 7 5700X + RTX 4060、Hyprland 0.56、1080p モニター 2 台、
[bench/measure.sh](../../bench/measure.sh)：

| シェル | 技術 | メモリ（PSS） | アイドル時 CPU | GL コンテキスト |
|---|---|---|---|---|
| **Mist** | Rust、CPU、`wl_shm` | **87〜106 MB** | 0.6 %（音楽再生中） | `mist-wall` のみ |
| Mist、`mist-ui` のみ | | 36〜50 MB | 0.6 % | なし |
| Noctalia v5 | C++、cairo + EGL | 122 MB | 0.07 % | あり |
| 空の Quickshell（`PanelWindow` ひとつ） | Qt Quick | 124 MB | ~0 % | あり |
| Quickshell 版 Mist（旧 QML 版） | Qt Quick | 428 MB | 9.5 % | あり |

`mist-comp` 上では、コンポジターとシェル全体を合わせてもアイドル時 **CPU 0.04 %、毎秒 6 回の起床**です
（ヘッドレス、ソフトウェアレンダラー）。計測方法、コンポジターの数値、実際の負荷の高いセッション：
[docs/BENCHMARKS.md](../BENCHMARKS.md)。

## mist-comp

Mist 独自の Wayland コンポジター。[Smithay] 上の約 12,500 行の Rust で、ログイン画面も備えます。同じシェルを
動かし、Hyprland の IPC を十分に実装しているので `hyprctl` や Mist のスクリプトはそのまま動きます。実験的ですが、
Hyprland と並ぶ作者の日常のセッションです。

- **ウィンドウ：** Mist のタイトルバーまたはアプリ自身のもの、角丸と影、半透明のウィンドウやパネルの下のガラス
  ぼかし、Windows 11 風スナップ、Overview（`Super + O`）、拡大鏡、指に追従するワークスペース切り替え、
  Win + D でデスクトップ表示。
- **省ける仕事はしない：** 内容が変わったモニターだけを再描画し、背後が動いたときだけガラスをぼかし直し、
  隠れたウィンドウには毎秒 60 フレームではなく 1 フレームだけ送ります。
- **セッション：** logind 経由の DRM / KMS、モニターのホットプラグ、VRR、explicit sync、XWayland、ポータル経由の
  画面共有、約 45 のプロトコル。クラッシュ後もウィンドウは元のワークスペースに戻り、`mist-session restart` で
  ログアウトせずに新しいビルドを読み込めます。
- **テスト：** ヘッドレスバックエンド上の 50 のネストテスト。CI ではソフトウェアレンダラーで実行します。

未対応：タイル配置、ティアリング制御、NVIDIA での直接スキャンアウトの検証。設定：
[docs/CONFIG.md](../CONFIG.md#compconf-outputs-mist-comp-only)。`mist-comp migrate` で Hyprland の設定を変換できます。

## インストール

Arch：[packaging/arch/PKGBUILD](../../packaging/arch/PKGBUILD) で `mist-shell-git` をビルドします。

```bash
git clone https://github.com/wiktor-jozwiak/mist && cd mist/packaging/arch && makepkg -si
```

クローンから（場所は任意）：

```bash
git clone https://github.com/wiktor-jozwiak/mist ~/src/mist
```

```bash
~/src/mist/scripts/mistd/install.sh && ~/src/mist/native/install.sh
```

ひとつ目が `mistd` をビルドし、ふたつ目が `mist-ui-bin`、`mist-wall-bin`、`mist-comp` を `~/.local/bin` に置きます。
ビルド済みバイナリは各[リリース](https://github.com/wiktor-jozwiak/mist/releases)に添付されています。

- **Hyprland で：** 設定から [examples/hyprland/mist.lua](../../examples/hyprland/mist.lua) を読み込み、他のバー、
  通知デーモン、polkit エージェントを外します。
- **mist-comp で：** `native/mist-comp/session/install-session.sh --greeter` がログイン画面に Mist セッションを
  追加します。Hyprland はそのまま残ります。

<details>
<summary>依存関係</summary>

<br>

**ビルド：** `rust`、`pkgconf`、`libxkbcommon`、`pam`、`wayland`、`mesa`。`mist-comp` には加えて `libinput`、
`libseat`、`libdrm`、`systemd-libs`。

**実行：** `pipewire-pulse`、`wireplumber`、`networkmanager`、`wl-clipboard`、`grim`、`slurp`、`polkit`、
`noto-fonts`、`noto-fonts-emoji`。任意：`ddcutil`（モニターの明るさ）、`kdeconnect` または `android-tools`
（スマートフォン）。それぞれで有効になる機能：[docs/CONFIG.md](../CONFIG.md#optional-integrations)。

**フォント：** UI は SF Pro を前提にデザインされていますが、Mist には同梱できません。ない場合は Inter、
次に Noto Sans を使います。

</details>

## 使い方

| キー | 動作 | | キー | 動作 |
|---|---|---|---|---|
| `Super` | スタート | | `Super + A` | 通知センター |
| `Super + Space` | Spotlight | | `Super + V` | クリップボード履歴 |
| `Super + Tab` | アプリ切り替え | | `Super + .` | 絵文字 |
| `Super + I` | コントロールセンター | | `Super + L` | ロック |
| `Super + Shift + S` | スクリーンショット | | デスクトップを右クリック | 設定 |

すべてスクリプトから操作できます：`mist-ui-bin msg mist toggle spotlight`、`mist-ui-bin msg ui stats`。設定は
`~/.config/mist/` にあり、各ファイルは [docs/CONFIG.md](../CONFIG.md) で説明しています。ログは
`~/.cache/mist/ui.log` です。

## 開発

```bash
cd native && cargo build --release && cargo test --release
```

どの画面もオフスクリーンで描画できるので、動作中のセッションに触れずに変更を確認できます：
`mist-ui-bin --pane-png spotlight 1920 1080 out.png`、`mist-ui-bin --desk-png dir`、
`mist-wall-bin --png out.ppm dunes 1920 1080`。`native/mist-comp/tests/run-all.sh` はネストしたウィンドウで
コンポジターのテストを実行します。詳しくは [native/README.md](../../native/README.md) と
[CONTRIBUTING.md](../../CONTRIBUTING.md)。

| パス | 内容 |
|---|---|
| `native/mist-ui` | シェルのすべての画面 |
| `native/mist-wall` | GPU 壁紙プロセス |
| `native/mist-comp` | コンポジター、セッション、ログイン画面 |
| `scripts/mistd` | バックエンド：システム情報、スマートフォン、着信、クリップボード、Garmin 同期 |
| `shaders/` | 壁紙と空のシェーダー |
| タグ `legacy-qml` | 最初の Quickshell 版 |

## クレジット

[Smithay]、[smithay-client-toolkit]、[tiny-skia]、[swash]、[resvg]、[zbus]、[Tabler Icons] の上に作られています。
最初の版は [Quickshell] 上で動き、[Noctalia] を参考にしました。

<sub>Mist は Apple とは無関係です。macOS、Dynamic Island、Spotlight、SF Pro は Apple Inc. の商標であり、
デザインの着想を説明するためにのみ記載しています。</sub>

[GPL-3.0](../../LICENSE)

[tiny-skia]: https://github.com/linebender/tiny-skia
[swash]: https://github.com/dfrg/swash
[resvg]: https://github.com/linebender/resvg
[zbus]: https://github.com/dbus2/zbus
[Smithay]: https://github.com/Smithay/smithay
[smithay-client-toolkit]: https://github.com/Smithay/client-toolkit
[Tabler Icons]: https://tabler.io/icons
[Quickshell]: https://quickshell.org
[Noctalia]: https://github.com/noctalia-dev/noctalia
