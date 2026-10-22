<div align="center">

<img src="../img/banner.svg" alt="Mist" width="100%">

**Un entorno de escritorio para Wayland inspirado en macOS, escrito en Rust y dibujado en la CPU.**

[![CI](https://img.shields.io/github/actions/workflow/status/wiktor-jozwiak/mist/ci.yml?branch=main&style=flat-square&label=CI)](../../.github/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/wiktor-jozwiak/mist?style=flat-square&color=3b82f6)](https://github.com/wiktor-jozwiak/mist/releases)
[![Licencia: GPL-3.0](https://img.shields.io/badge/licencia-GPL--3.0-8d8590?style=flat-square)](../../LICENSE)
[![Discord](https://img.shields.io/badge/discord-chat-5865F2?style=flat-square&logo=discord&logoColor=white)](https://discord.gg/4rRabwQTPE)

[English](../../README.md) · [Polski](README.pl.md) · [Deutsch](README.de.md) ·
[Español](README.es.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

<br>

<img src="../img/desktop-en.webp" alt="El escritorio de Mist: barra superior, reloj, tarjetas de tiempo, disposición y carrera, dock" width="100%">

<table>
  <tr>
    <td width="50%"><img src="../img/island.gif" alt="Dynamic Island: una notificación, el volumen, lluvia en breve, el próximo evento" width="100%"></td>
    <td width="50%"><img src="../img/weather.gif" alt="La tarjeta del tiempo bajo la lluvia, sobre su cielo en vivo" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>Dynamic Island: notificaciones, volumen, lluvia en la próxima hora, próximo evento</sub></td>
    <td align="center"><sub>Tarjeta del tiempo sobre un cielo GLSL en tiempo real</sub></td>
  </tr>
</table>

</div>

Mist es la barra, el dock, la Dynamic Island, los widgets del escritorio, el buscador, el centro de control,
las notificaciones, la pantalla de bloqueo y el agente polkit de un escritorio, en un único proceso de ~50 MB
que nunca abre un contexto de GPU. Funciona sobre Hyprland o sobre su propio compositor, `mist-comp`.

Empezó como una configuración de Quickshell que usaba 428 MB y un 9,5 % de CPU en reposo, y se reescribió
superficie a superficie siguiendo tres reglas:

- **Dibujar en la CPU.** `mist-ui` pinta búferes `wl_shm` con [tiny-skia] y [swash] y solo redibuja lo que
  cambió. En NVIDIA cada contexto OpenGL cuesta entre 100 y 160 MB antes del primer píxel; Mist no tiene
  ninguno.
- **Esperar eventos.** Todo llega del socket del compositor, D-Bus, `pactl subscribe` e inotify. Nada consulta
  en bucle mientras la pantalla está quieta, y las superficies ocultas no trabajan.
- **Un solo proceso de GPU, y solo cuando muestra algo.** `mist-wall` dibuja fondos animados y el cielo del
  tiempo; con una foto de fondo se cierra y la foto la dibuja `mist-ui`.

> [!NOTE]
> Mist es el escritorio diario de una persona (Arch / CachyOS, dos monitores, NVIDIA). Es público porque
> funciona, no porque esté terminado: cuenta con leer algo de código.

## Funciones

<table>
  <tr>
    <td width="50%"><img src="../img/panels/spotlight.webp" alt="Búsqueda al estilo Spotlight" width="100%"></td>
    <td width="50%"><img src="../img/panels/settings.webp" alt="Ventana de ajustes" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>Spotlight: aplicaciones, archivos, comandos, emoji, web</sub></td>
    <td align="center"><sub>Ajustes: todo en una ventana, aplicado al instante</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="../img/panels/control.webp" alt="Centro de control" width="100%"></td>
    <td width="50%"><img src="../img/panels/calendar.webp" alt="Calendario" width="100%"></td>
  </tr>
  <tr>
    <td align="center"><sub>Centro de control: red, Bluetooth, teléfono, sonido, luz nocturna</sub></td>
    <td align="center"><sub>Calendario: mes, agenda, eventos nuevos</sub></td>
  </tr>
</table>

| | |
|---|---|
| **Barra superior** | aplicación activa, bandeja del sistema (StatusNotifierItem + dbusmenu), red, Bluetooth, audio, CPU y memoria, reloj |
| **Dock** | aplicaciones fijadas y abiertas, ampliación, selector de ventanas con miniaturas en vivo |
| **Dynamic Island** | multimedia con ecualizador, temporizadores, llamadas (KDE Connect o ADB), notificaciones, volumen |
| **Escritorio** | reloj y tarjetas de cristal: tiempo, disposición de Garmin, carrera, música, calendario, sistema, relojes del mundo |
| **Paneles** | Spotlight, Inicio, cambiador de aplicaciones, centro de control, centro de notificaciones, portapapeles, emoji, menú de sesión, capturas |
| **Ajustes** | idioma, lugar del tiempo, fondo, disposición del escritorio, widgets, luz nocturna, bloqueo y suspensión, notificaciones |
| **Sistema** | servidor de notificaciones, agente polkit, pantalla de bloqueo (PAM), inactividad y suspensión |
| **Aplicaciones** | Mist Files (también el selector de archivos de cualquier aplicación), Mist Player (libmpv), ajustes del sistema |
| **Ajuste de ventanas** | como en Windows 11: arrastra a un borde o esquina, Snap Layouts en el botón verde o con `Super + Z`, Snap Assist |
| **Fondos** | fotos, escenas GLSL animadas o un cielo que sigue el tiempo actual |

El **tiempo** viene de [Open-Meteo](https://open-meteo.com) sin cuenta: curva por horas, cuatro días, lluvia en
la próxima hora, sol y luna. **Disposición de Garmin** y **carrera** muestran tus propios datos de Garmin
Connect, que `mistd` sincroniza en un archivo SQLite local; las tarjetas siguen ocultas hasta que inicias
sesión una vez ([guía](../guides/garmin.md)).

La interfaz está en **español, inglés, polaco, alemán y francés**, según `$LANG` o lo elegido en Ajustes. Un
idioma es un archivo JSON en [native/assets/i18n](../../native/assets/i18n).

## Mediciones

<p align="center"><img src="../img/memory.svg" alt="Memoria en reposo: mist-ui solo 36 a 50 MB, Mist con mist-wall 87 a 106 MB, Noctalia v5 122 MB, Quickshell vacío 124 MB, Mist sobre Quickshell 428 MB" width="100%"></p>

Todo el entorno en reposo, PSS, Ryzen 7 5700X + RTX 4060, Hyprland 0.56, dos monitores 1080p,
[bench/measure.sh](../../bench/measure.sh):

| Entorno | Tecnología | Memoria (PSS) | CPU en reposo | Contexto GL |
|---|---|---|---|---|
| **Mist** | Rust, CPU, `wl_shm` | **87 a 106 MB** | 0,6 % (con música) | solo `mist-wall` |
| Mist, solo `mist-ui` | | 36 a 50 MB | 0,6 % | no |
| Noctalia v5 | C++, cairo + EGL | 122 MB | 0,07 % | sí |
| Quickshell vacío (un `PanelWindow`) | Qt Quick | 124 MB | ~0 % | sí |
| Mist sobre Quickshell (versión QML antigua) | Qt Quick | 428 MB | 9,5 % | sí |

Sobre `mist-comp`, el compositor y todo el entorno juntos se quedan en **0,04 % de CPU y 6 despertares por
segundo** (sin monitor, renderizador por software). Método, las cifras del compositor y una sesión real con
carga: [docs/BENCHMARKS.md](../BENCHMARKS.md).

## mist-comp

El compositor Wayland propio de Mist, unas 12 500 líneas de Rust sobre [Smithay], con pantalla de inicio de
sesión. Ejecuta el mismo entorno y responde a suficiente IPC de Hyprland como para que `hyprctl` y los scripts
de Mist funcionen sin cambios. Es experimental y es la sesión diaria del autor junto a Hyprland.

- **Ventanas:** barras de título de Mist o las de la aplicación, esquinas redondeadas y sombras, desenfoque de
  cristal bajo ventanas y paneles translúcidos, ajuste como en Windows 11, Overview (`Super + O`), una lupa,
  cambio de escritorio que sigue a los dedos, Win + D muestra el escritorio.
- **No hace el trabajo que puede evitar:** un monitor solo se redibuja si algo cambió en él; el cristal solo
  se vuelve a desenfocar si algo detrás se movió; una ventana tapada recibe un fotograma por segundo en lugar
  de sesenta.
- **Sesión:** DRM / KMS mediante logind, conexión de monitores en caliente, VRR, explicit sync, XWayland,
  compartir pantalla mediante el portal, unos 45 protocolos. Tras un fallo las ventanas vuelven a sus
  escritorios; `mist-session restart` carga una versión nueva sin cerrar sesión.
- **Probado:** 50 pruebas anidadas sobre un backend sin monitor, ejecutadas en la CI con el renderizador por software.

Aún no: mosaico, control del tearing, scan-out directo verificado en NVIDIA. Configuración:
[docs/CONFIG.md](../CONFIG.md#compconf-outputs-mist-comp-only); `mist-comp migrate` convierte una configuración de Hyprland.

## Instalación

Arch: [packaging/arch/PKGBUILD](../../packaging/arch/PKGBUILD) construye `mist-shell-git`.

```bash
git clone https://github.com/wiktor-jozwiak/mist && cd mist/packaging/arch && makepkg -si
```

Desde un clon, en cualquier lugar:

```bash
git clone https://github.com/wiktor-jozwiak/mist ~/src/mist
```

```bash
~/src/mist/scripts/mistd/install.sh && ~/src/mist/native/install.sh
```

El primero compila `mistd`, el segundo coloca `mist-ui-bin`, `mist-wall-bin` y `mist-comp` en `~/.local/bin`.
Cada [versión](https://github.com/wiktor-jozwiak/mist/releases) incluye binarios ya compilados.

- **En Hyprland:** carga [examples/hyprland/mist.lua](../../examples/hyprland/mist.lua) desde tu configuración y
  quita otras barras, demonios de notificaciones y agentes polkit.
- **En mist-comp:** `native/mist-comp/session/install-session.sh --greeter` añade una sesión Mist a la pantalla
  de inicio; Hyprland se queda.

<details>
<summary>Dependencias</summary>

<br>

**Compilación:** `rust`, `pkgconf`, `libxkbcommon`, `pam`, `wayland`, `mesa`; para `mist-comp` también
`libinput`, `libseat`, `libdrm`, `systemd-libs`.

**Ejecución:** `pipewire-pulse`, `wireplumber`, `networkmanager`, `wl-clipboard`, `grim`, `slurp`, `polkit`,
`noto-fonts`, `noto-fonts-emoji`. Opcional: `ddcutil` (brillo del monitor), `kdeconnect` o `android-tools`
(teléfono). Qué activa cada una: [docs/CONFIG.md](../CONFIG.md#optional-integrations).

**Fuente:** la interfaz está pensada para SF Pro, que Mist no puede incluir. Sin ella usa Inter y después
Noto Sans.

</details>

## Uso

| Tecla | Acción | | Tecla | Acción |
|---|---|---|---|---|
| `Super` | Inicio | | `Super + A` | centro de notificaciones |
| `Super + Space` | Spotlight | | `Super + V` | portapapeles |
| `Super + Tab` | cambiador de aplicaciones | | `Super + .` | emoji |
| `Super + I` | centro de control | | `Super + L` | bloquear |
| `Super + Shift + S` | captura | | clic derecho en el escritorio | ajustes |

Todo se puede automatizar: `mist-ui-bin msg mist toggle spotlight`, `mist-ui-bin msg ui stats`. Los ajustes
están en `~/.config/mist/`, cada archivo se describe en [docs/CONFIG.md](../CONFIG.md); el registro es
`~/.cache/mist/ui.log`.

## Desarrollo

```bash
cd native && cargo build --release && cargo test --release
```

Cada superficie se renderiza fuera de pantalla, así que los cambios se comprueban sin tocar la sesión en marcha:
`mist-ui-bin --pane-png spotlight 1920 1080 out.png`, `mist-ui-bin --desk-png dir`,
`mist-wall-bin --png out.ppm dunes 1920 1080`. `native/mist-comp/tests/run-all.sh` ejecuta las pruebas del
compositor en una ventana anidada. Más en [native/README.md](../../native/README.md) y
[CONTRIBUTING.md](../../CONTRIBUTING.md).

| Ruta | Qué |
|---|---|
| `native/mist-ui` | todas las superficies del entorno |
| `native/mist-wall` | el proceso de fondos en GPU |
| `native/mist-comp` | compositor, sesión y pantalla de inicio |
| `scripts/mistd` | backends: estadísticas del sistema, teléfono, llamadas, portapapeles, sincronización de Garmin |
| `shaders/` | shaders de fondos y cielo |
| etiqueta `legacy-qml` | la versión original en Quickshell |

## Créditos

Construido sobre [Smithay], [smithay-client-toolkit], [tiny-skia], [swash], [resvg], [zbus] y [Tabler Icons].
La primera versión funcionaba sobre [Quickshell], con [Noctalia] como referencia.

<sub>Mist no está afiliado a Apple. macOS, Dynamic Island, Spotlight y SF Pro son marcas de Apple Inc., citadas
aquí solo para describir la inspiración del diseño.</sub>

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
