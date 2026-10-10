import QtQuick
import "LivePresets.js" as Presets
import "Astro.js" as Astro
import ".."

// One live wallpaper scene (shaders/live.frag, toon.frag, fx.frag), drawn at full resolution with a
// sub-LSB dither in the shader. A Timer drives time at `fps` instead of every
// vsync (165 Hz on DP-3); nothing runs while `animate` is false. Used by LiveWallpaper.qml (desktop)
// and as moving thumbnails in Wallpapers.qml.
// auto presets: colours follow the real sun altitude (Astro.js, the weather's lat / lon), dulled by cloud
// cover and rain, sped up by the wind; Horyzont / Góry put the sun or the moon where it really stands.
// weather presets (Za oknem) also pick the scene: rain on the window, snowfall, smoke for fog, stars on a
// clear night, else mountains.
Item {
    id: lw
    property string preset: "aurora"
    property bool animate: true
    property int fps: 30
    // behind windows (only their blur shows it): a few frames a second are enough
    property bool slow: false
    property int slowFps: 5
    readonly property int curFps: slow ? slowFps : fps
    property real speedMul: 1
    property real energy: 0                     // music level 0..1 (LiveWallpaper.qml)
    // a wall-clock start keeps all monitors and the picker previews in step
    property real t: (Date.now() / 1000) % 10000
    readonly property var p: Presets.get(preset) || Presets.list[0]

    // ------------------------------------------------------------ auto: sun, moon, weather
    readonly property var wx: Data.weather || ({})
    readonly property real lat: wx.lat ?? 52.2297
    readonly property real lon: wx.lon ?? 21.0122
    readonly property var sunP: p.auto ? Astro.sun(Data.now, lat, lon) : null
    readonly property var moonP: p.auto && (mode === 5 || mode === 14) ? Astro.moon(Data.now, lat, lon) : null
    readonly property real cloud: Math.max(0, Math.min(1, (wx.cloud ?? (wx.code >= 3 ? 90 : 20)) / 100))
    readonly property bool wet: (wx.precip ?? 0) > 0 || (wx.code ?? 0) >= 51
    readonly property int code: wx.code ?? 3
    readonly property int mode: {
        if (!p.weather) return p.mode
        if (code >= 71 && code <= 77 || code === 85 || code === 86) return 16
        if (wet) return 8
        if (code === 45 || code === 48) return 20
        if (sunP && sunP.alt < -6 && cloud < 0.5) return 11
        return 14
    }
    readonly property var dayP: p.auto && sunP ? Presets.day(sunP.alt, cloud, wet) : null
    readonly property var cols: dayP ? dayP.c : p.c
    readonly property real stars: dayP ? dayP.stars * (mode === 5 || mode === 14 ? 0.8 : 0.5) : p.stars
    readonly property real windK: p.auto ? Math.min(2.2, 0.8 + (wx.wind ?? 0) / 25) : 1
    // horizon: view faces south, the frame spans ~140° of azimuth; height = altitude over 60°
    function place(b, strength) {
        return Qt.vector4d(Math.max(0.08, Math.min(0.92, 0.5 + (b.az - 180) / 140)), 0.64 - Math.max(-5, b.alt) / 60 * 0.6, 0.026, strength)
    }
    readonly property vector4d sunV: {
        if (mode !== 5 && mode !== 14) return Qt.vector4d(0, 0, 0, 0)
        if (!p.auto) return p.sun ? Qt.vector4d(p.sun[0], p.sun[1], p.sun[2], p.sun[3]) : Qt.vector4d(0.5, 2, 0.02, 0)
        if (!sunP) return Qt.vector4d(0.5, 2, 0.02, 0)
        if (sunP.alt > -3) return place(sunP, 1 - 0.75 * cloud)
        if (moonP && moonP.alt > 0) return place(moonP, -(0.25 + 0.6 * moonP.fraction) * (1 - 0.8 * cloud))
        return Qt.vector4d(0.5, 2, 0.02, 0)
    }

    Timer {
        interval: Math.round(1000 / lw.curFps)
        repeat: true
        running: lw.animate && lw.visible && lw.width > 0
        onTriggered: lw.t += lw.p.speed * lw.speedMul * lw.windK * (1 + 0.4 * lw.energy) / lw.curFps
    }

    // drawn straight at full resolution: an offscreen half-res pass (ShaderEffectSource) made Qt render
    // every tick twice and measured costlier on CPU and GPU than the full-res shader itself
    ShaderEffect {
        id: scene
        anchors.fill: parent
        blending: false
        fragmentShader: Qt.resolvedUrl(lw.p.shader === "toon" ? "../shaders/toon.frag.qsb" : lw.p.shader === "fx" ? "../shaders/fx.frag.qsb" : "../shaders/live.frag.qsb")
        property real time: lw.t
        property real mode: lw.mode
        property size size: Qt.size(width, height)
        property color c0: lw.cols[0]
        property color c1: lw.cols[1]
        property color c2: lw.cols[2]
        property color c3: lw.cols[3]
        property vector4d opt: Qt.vector4d(lw.stars, lw.p.vig, lw.p.bright, lw.energy)
        property vector4d sun: lw.sunV
    }
}
