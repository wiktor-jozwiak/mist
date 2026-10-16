<div align="center">

# Mist

**Un entorno de escritorio para Hyprland inspirado en macOS, escrito en Rust y dibujado en la CPU.**

Barra, dock, Dynamic Island, widgets de escritorio, buscador al estilo Spotlight, centro de control,
notificaciones, pantalla de bloqueo y agente polkit en un solo proceso de ~50 MB, sin contexto de GPU.

[English](../../README.md) · [Polski](README.pl.md) · [Deutsch](README.de.md) ·
[Español](README.es.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

![Licencia: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue)
![Rust](https://img.shields.io/badge/rust-2024-orange)
![Wayland](https://img.shields.io/badge/wayland-Hyprland-58e1ff)

![Escritorio de Mist](../img/desktop.webp)

<sub>Render fuera de pantalla (`mist-ui --desk-png`, `--dock-png`, `--png`, `mist-wall-bin --png`) con datos
de demostración. En una sesión real Hyprland desenfoca el fondo bajo el cristal.</sub>

</div>

## Por qué

La mayoría de los entornos para Hyprland usan Qt Quick (Quickshell), GTK + GJS (AGS) o un WebView. Todos
abren un contexto OpenGL, y en NVIDIA cada contexto GL carga el controlador: unos **100 a 160 MB por proceso**
antes del primer píxel. Mist empezó como una configuración de Quickshell (428 MB, 9,5 % de CPU) y se
reescribió superficie a superficie:

- **Superficies dibujadas en la CPU.** `mist-ui` pinta en búferes `wl_shm` con [tiny-skia] y [swash];
  Hyprland las compone y desenfoca. Solo se redibujan las zonas que cambian.
- **Basado en eventos.** Los datos llegan por el socket de Hyprland, D-Bus, `pactl subscribe`, inotify y
  backends `mistd` de larga duración. Nada consulta con temporizadores mientras la pantalla está quieta;
  las superficies ocultas no trabajan.
- **Un único proceso de GPU, solo cuando hace falta.** `mist-wall` (EGL, GLSL 150) dibuja fondos animados y
  el cielo del tiempo. Con una foto de fondo se cierra y `mist-ui` dibuja la foto.

Resultado en la máquina del autor: **~87 a 106 MB de PSS para todo el entorno**, 0,6 % de un núcleo con
música sonando. Ver [docs/BENCHMARKS.md](../BENCHMARKS.md) (en inglés).

## Funciones

| | |
|---|---|
| **Barra superior** | aplicación y título activos, bandeja (host StatusNotifierItem + dbusmenu), red, Bluetooth, audio, CPU / memoria, reloj, calendario |
| **Dock** | aplicaciones fijadas y abiertas, ampliación, selector de ventanas con miniaturas en vivo (`hyprland_toplevel_export_v1`), clic en la app activa la minimiza |
| **Dynamic Island** | multimedia (MPRIS) con ecualizador, temporizadores, llamadas (KDE Connect / ADB), notificaciones, volumen |
| **Escritorio** | reloj, salida / puesta de sol, tarjetas de cristal (tiempo, disposición Garmin, plan de carrera), hoja de ajustes por widget |
| **Paneles** | buscador al estilo Spotlight, Inicio, selector Cmd+Tab, centro de control, centro de notificaciones, historial del portapapeles, emoji, menú de sesión, galería de fondos, barra de capturas |
| **Sistema** | servidor de notificaciones, agente polkit, bloqueo (`ext-session-lock-v1` + PAM), inactividad (`ext-idle-notify-v1`, logind), acople de ventanas como en Windows 11 |
| **Fondos** | fotos (CPU) o escenas GLSL en vivo y cielo meteorológico en tiempo real (`mist-wall`) |
| **mist-comp** *(experimental)* | compositor propio sobre Smithay + greeter, para usar Mist sin Hyprland |

## Arquitectura

```
 Hyprland ── layer-shell ──┬── mist-ui      Rust, CPU (tiny-skia + swash), wl_shm, un proceso para todas las superficies
                           │      ├── mistd stats | phone | calls | island | clipstore   (backends de eventos)
                           │      └── IPC: mist-ui-bin msg <destino> <función> [args]
                           └── mist-wall    Rust, EGL / GLSL 150, solo fondos en vivo y el cielo
```

| Ruta | Qué |
|---|---|
| `native/mist-ui` | todas las superficies ([native/README.md](../../native/README.md)) |
| `native/mist-wall` | proceso de GPU del fondo |
| `native/mist-comp` | compositor experimental, sesión y greeter |
| `native/common` | código compartido por mist-ui y mist-wall (astronomía, presets) |
| `scripts/mistd` | backends: estadísticas, teléfono (KDE Connect, ADB), llamadas, portapapeles, D-Bus del gestor de archivos |
| `shaders/` | shaders de fondos y cielo (fuentes `.frag`, paquetes `.qsb` que lee mist-wall) |
| `*.qml`, `components/` | la versión original en Quickshell, conservada como referencia; ya no se inicia |

## Instalación

Mist es el escritorio diario de una persona: Arch / CachyOS, Hyprland 0.55+ con configuración en Lua, GPU
NVIDIA o Mesa. Cuenta con leer código.

**Compilación:** `rust` (stable), `pkgconf`, `libxkbcommon`, `pam`, `wayland`, `mesa` (cabeceras EGL);
para `mist-comp` también `libinput`, `libseat`, `libdrm`, `mesa` (gbm), `systemd-libs` (udev).

**Ejecución:** `hyprland`, `pipewire` + `pipewire-pulse` (`pactl`, `pw-play`, `parec`), `wireplumber`
(`wpctl`), `networkmanager` (`nmcli`), `wl-clipboard`, `grim`, `slurp`, `polkit`, `qt6-shadertools` (`qsb`,
solo fondos en vivo), `noto-fonts`, `noto-fonts-emoji`. Opcional: `ddcutil` (brillo por DDC/CI),
`kdeconnect` / `android-tools` (teléfono), `wf-recorder` (grabación de pantalla).

**Fuente:** la interfaz está diseñada para SF Pro (`SF-Pro.ttf` variable en `~/.local/share/fonts/SF-Pro/`),
que Apple licencia solo para sus plataformas y Mist no incluye. Sin ella Mist usa Inter Variable y luego
Noto Sans.

```bash
git clone <este repositorio> ~/.config/quickshell/mist
```

```bash
~/.config/quickshell/mist/scripts/mistd/install.sh
```

```bash
~/.config/quickshell/mist/native/install.sh
```

El clon debe estar en `~/.config/quickshell/mist` (las rutas aún son fijas, ver
[docs/PUBLISHING.md](../PUBLISHING.md)). `native/install.sh` instala en `~/.local/bin` `mist-ui-bin`,
`mist-wall-bin`, `mist-comp` y, la primera vez, el guardián `~/.local/bin/mist-ui`.

Después añade [examples/hyprland/mist.lua](../../examples/hyprland/mist.lua) a la configuración de Hyprland
(arranque, atajos, reglas de desenfoque) y quita otras barras, demonios de notificaciones y agentes polkit.

## Uso

| Tecla | Acción |
|---|---|
| `Super` | Inicio |
| `Super + Espacio` | Spotlight |
| `Super + Tab` | selector de aplicaciones |
| `Super + I` | centro de control |
| `Super + A` | centro de notificaciones |
| `Super + V` | historial del portapapeles |
| `Super + .` | emoji |
| `Super + L` | bloquear |
| `Impr`, `Super + Shift + S` | captura (región) |

Todo es programable: `mist-ui-bin msg mist toggle spotlight`, `mist-ui-bin msg ui stats`,
`mist-ui-bin msg dock click firefox`. Registro: `~/.cache/mist/ui.log`. Ajustes: `~/.config/mist/`.

## Desarrollo

```bash
cd native && cargo build --release && cargo test --release
```

Comprobaciones sin tocar la sesión en marcha:

| Comando | Resultado |
|---|---|
| `mist-ui --png out.png '{"app":"Firefox"}'` | la barra superior en PNG |
| `mist-ui-bin --desk-png dir` / `--dock-png out.png` | escenario del escritorio + tarjetas / dock |
| `mist-ui --test` | una segunda barra sobre la real en HDMI-A-1 |
| `mist-wall-bin --png out.ppm dunes 1920 1080` | un fotograma del fondo en vivo |
| `mist-ui-bin msg ui stats` | despertares, repintados y tiempo de pintado por superficie |
| `bench/measure.sh 30 mist-ui-bin mist-wall-bin mistd` | PSS / CPU en reposo del entorno en marcha |

Mantén `lto = false` (el rustc de Arch con el LLVM del sistema falla con LTO). Ver
[CONTRIBUTING.md](../../CONTRIBUTING.md).

## Estado

Escritorio diario de una persona, dos monitores, NVIDIA. Todo lo de la tabla de funciones es nativo y se usa
a diario; `mist-comp` es experimental. Los textos de la interfaz están en polaco por ahora (lo que falta antes
de una publicación: [docs/PUBLISHING.md](../PUBLISHING.md)).

## Créditos

[Hyprland], [Smithay] y [smithay-client-toolkit], [tiny-skia], [swash], [resvg], [zbus], [Tabler Icons]
(MIT). La versión QML original se hizo con [Quickshell]; [Noctalia] sirvió de respaldo y referencia.

*Mist no está afiliado a Apple. macOS, Dynamic Island, Spotlight y SF Pro son marcas de Apple Inc.; se
mencionan aquí solo para describir la inspiración.*

## Licencia

[GPL-3.0](../../LICENSE).

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
