<div align="center">

<img src="../img/logo.svg" alt="" width="104">

# Mist

**{{tagline}}**

{{BADGES}}

[English](../../README.md) · [Polski](README.pl.md) · [Deutsch](README.de.md) ·
[Español](README.es.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

<br>

<img src="../img/{{desk_img}}" alt="{{desk_alt}}" width="100%">

<table>
  <tr>
    <td align="center" width="33%"><h3>{p_pss} MB</h3><sub>{{st_mem}}</sub></td>
    <td align="center" width="33%"><h3>0</h3><sub>{{st_gl}}</sub></td>
    <td align="center" width="33%"><h3>{wake} / s</h3><sub>{{st_wake}}</sub></td>
  </tr>
</table>

**[{{h_tour}}](#{{a_tour}}) · [{{h_what}}](#{{a_what}}) · [{{h_why}}](#{{a_why}}) · [mist-comp](#mist-comp) ·
[{{h_install}}](#{{a_install}}) · [{{nav_keys}}](#{{a_keys}}) · [{{h_dev}}](#{{a_dev}})**

</div>

{{intro}}

> [!NOTE]
{{note}}

## {{h_tour}}

<div align="center">

<img src="../img/island.webp" alt="{{alt_island}}" width="100%">

<sub>{{sub_island}}</sub>

<br><br>

<table>
  <tr>
    <td width="50%" align="center"><img src="../img/weather.webp" alt="{{alt_weather}}" width="100%"><br><sub>{{sub_weather}}</sub></td>
    <td width="50%" align="center"><img src="../img/wallpaper.webp" alt="{{alt_wall}}" width="100%"><br><sub>{{sub_wall}}</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="../img/panels/start.webp" alt="{{alt_start}}" width="100%"><br><sub>{{sub_start}}</sub></td>
    <td align="center"><img src="../img/panels/spotlight.webp" alt="{{alt_spot}}" width="100%"><br><sub>{{sub_spot}}</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="../img/panels/files.webp" alt="{{alt_files}}" width="100%"><br><sub>{{sub_files}}</sub></td>
    <td align="center"><img src="../img/panels/player.webp" alt="{{alt_player}}" width="100%"><br><sub>{{sub_player}}</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="../img/panels/settings.webp" alt="{{alt_set}}" width="100%"><br><sub>{{sub_set}}</sub></td>
    <td align="center"><img src="../img/panels/control.webp" alt="{{alt_cc}}" width="100%"><br><sub>{{sub_cc}}</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="../img/panels/calendar.webp" alt="{{alt_cal}}" width="100%"><br><sub>{{sub_cal}}</sub></td>
    <td align="center"><img src="../img/panels/emoji.webp" alt="{{alt_emoji}}" width="100%"><br><sub>{{sub_emoji}}</sub></td>
  </tr>
</table>

<img src="../img/themes.webp" alt="{{alt_themes}}" width="100%">

<sub>{{sub_themes}}</sub>

<br><br>

<table>
  <tr>
    <td align="center" width="33%"><img src="../../native/assets/icons/hicolor/scalable/apps/mist-files.svg" alt="" width="88"><br><b>Mist Files</b><br><sub>{{app_files}}</sub></td>
    <td align="center" width="33%"><img src="../../native/assets/icons/hicolor/scalable/apps/mist-player.svg" alt="" width="88"><br><b>Mist Player</b><br><sub>{{app_player}}</sub></td>
    <td align="center" width="33%"><img src="../../native/assets/icons/hicolor/scalable/apps/mist-settings.svg" alt="" width="88"><br><b>{{app_set_name}}</b><br><sub>{{app_set}}</sub></td>
  </tr>
</table>

<br>

<a href="https://youtu.be/mwisDtz0Yd0"><img src="../img/demo-poster.webp" alt="Mist demo video (40 s)" width="720"></a>

<sub>{{demo}} · <a href="https://github.com/user-attachments/assets/ed8137a5-daf5-488e-8515-686aa54e6e07">MP4</a></sub>

</div>

## {{h_what}}

**{{g_shell}}**

{{b_shell}}

**{{g_desk}}**

{{b_desk}}

**{{g_repl}}**

{{b_repl}}

{{langs}}

## {{h_why}}

{{why_intro}}

{{rules}}

<p align="center"><img src="../img/memory.svg" alt="{{chart_alt}}" width="100%"></p>

<details>
<summary>{{nums}}</summary>

<br>

{{shells_p}}

{{SHELLS}}

{{comps_p}}

{{COMPS}}

{{bench_more}}

</details>

## mist-comp

{{comp_p}}

{{comp_b}}

{{comp_not}}

## {{h_install}}

### {{h_get}}

{{get_arch}}

```bash
git clone https://github.com/wiktor-jozwiak/mist && cd mist/packaging/arch && makepkg -si
```

{{get_any}}

```bash
git clone https://github.com/wiktor-jozwiak/mist ~/src/mist
```

```bash
~/src/mist/scripts/mistd/install.sh && ~/src/mist/native/install.sh
```

{{get_rel}}

<details>
<summary>{{deps}}</summary>

<br>

{{deps_p}}

</details>

### {{h_try}}

```bash
mist-try
```

{{try_p}}

### {{h_make}}

{{make_b}}

## {{h_keys}}

{{keys_t}}

{{keys_p}}

## {{h_dev}}

```bash
cd native && cargo build --release && cargo test --release
```

{{dev_p}}

{{dev_pics}}

{{paths_t}}

## {{h_stars}}

<a href="https://star-history.com/#wiktor-jozwiak/mist&Date">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=wiktor-jozwiak/mist&type=Date&theme=dark">
    <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=wiktor-jozwiak/mist&type=Date">
    <img alt="{{stars_alt}}" src="https://api.star-history.com/svg?repos=wiktor-jozwiak/mist&type=Date" width="100%">
  </picture>
</a>

## {{h_credits}}

{{credits}}

<sub>{{apple}}</sub>

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
