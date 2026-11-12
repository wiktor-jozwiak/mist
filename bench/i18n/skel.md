<div align="center">

<img src="../img/banner.svg" alt="Mist" width="100%">

**{{tagline}}**

{{BADGES}}

[English](../../README.md) · [Polski](README.pl.md) · [Deutsch](README.de.md) ·
[Español](README.es.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

<br>

<img src="../img/{{desk_img}}" alt="{{desk_alt}}" width="100%">

**[{{h_what}}](#{{a_what}}) · [{{h_why}}](#{{a_why}}) · [mist-comp](#mist-comp) ·
[{{h_install}}](#{{a_install}}) · [{{nav_keys}}](#{{a_keys}}) · [{{h_dev}}](#{{a_dev}})**

</div>

{{intro}}

> [!NOTE]
{{note}}

## {{h_what}}

<div align="center">

<table>
  <tr>
    <td width="50%"><img src="../img/island.gif" alt="{{alt_island}}" width="100%"></td>
    <td width="50%"><img src="../img/weather.gif" alt="{{alt_weather}}" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>{{sub_island}}</sub></td>
    <td align="center"><sub>{{sub_weather}}</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="../img/panels/spotlight.webp" alt="{{alt_spot}}" width="100%"></td>
    <td width="50%"><img src="../img/panels/settings.webp" alt="{{alt_set}}" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>{{sub_spot}}</sub></td>
    <td align="center"><sub>{{sub_set}}</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="../img/panels/control.webp" alt="{{alt_cc}}" width="100%"></td>
    <td width="50%"><img src="../img/panels/calendar.webp" alt="{{alt_cal}}" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>{{sub_cc}}</sub></td>
    <td align="center"><sub>{{sub_cal}}</sub></td>
  </tr>
</table>

<a href="https://youtu.be/mwisDtz0Yd0"><img src="../img/demo-poster.webp" alt="Mist demo video (40 s)" width="720"></a>

<sub>{{demo}} · <a href="https://github.com/user-attachments/assets/ed8137a5-daf5-488e-8515-686aa54e6e07">MP4</a></sub>

</div>

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

{{shells_p}}

{{SHELLS}}

{{comps_p}}

{{COMPS}}

{{bench_more}}

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

{{paths_t}}

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
