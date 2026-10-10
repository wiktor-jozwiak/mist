import QtQuick
import "Astro.js" as Astro

// Weather scene drawn by one fragment shader (shaders/sky.frag), true to what is outside right now:
// - the Sun and the Moon stand where they really are (altitude / azimuth for lat, lon; view faces south,
//   east on the left), the Moon with its real phase, terminator tilt, earthshine, also by day when it is up
// - sky colour follows the sun's altitude: day, golden hour, civil / nautical twilight, night
// - stars fade with twilight, moonlight and haze; meteor rate follows the shower calendar; aurora from Kp
// - volumetric clouds, amount = cover %, lit from the real sun direction, drifting with the wind
// - rain / drizzle / hail / snow amount from the 15-min measurement, slanted by the wind; fog from visibility
// - life: autumn leaves tumbling in the wind, a flock of birds now and then by day, sun shafts through broken cloud
// Two passes: shaders/clouds.frag (sky + ray-marched volumetric clouds, half resolution) and
// shaders/sky.frag (stars, discs, precipitation, glass drops, text scrim, corners, full resolution).
// A Timer drives time at `fps` (30) instead of every vsync (165 Hz on DP-3),
// and nothing runs while `animate` is false.
Item {
    id: sky
    property int code: 3
    property bool animate: true
    property real wind: 0                       // km/h
    property real cover: -1                     // cloud cover 0..100 % (API); -1 = derive from the code
    property real precip: 0                     // mm in the last 15 min
    property real snowNow: 0                    // cm in the last 15 min
    property real windDir: 270                  // where the wind comes FROM, degrees
    property real gust: 0                       // km/h
    property real vis: 50000                    // visibility, m
    property real kp: 0                         // planetary K index
    property real temp: 10                      // °C
    property real hum: 70                       // relative humidity %
    property real snowDepth: 0                  // m of snow on the ground
    property real hor: 0.86                     // horizon line, 0..1 of the height
    // card shake while lightning flashes (read by the card)
    readonly property real flash: bolt.x
    property real lat: 52.2297
    property real lon: 21.0122
    property date now: new Date()               // astronomy refresh (Data.now ticks once a minute)
    property real radius: 0                     // rounded corners, drawn by the shader (no clipping layer)
    property real boxH: height                  // visible height (the card can be shorter than the scene)
    property int fps: 30
    // behind windows (only their blur shows the card): 5 frames a second
    property bool slow: false

    // ------------------------------------------------------------ weather -> intensities
    readonly property bool codeSnow: code >= 71 && code <= 77 || code === 85 || code === 86
    readonly property bool codeWet: code >= 51 && code <= 67 || code >= 80 && code <= 82 || code >= 95
    readonly property bool codeFog: code === 45 || code === 48
    readonly property bool drizzleCode: code >= 51 && code <= 57
    readonly property bool heavyCode: code === 65 || code === 67 || code === 82 || code >= 95
    readonly property bool freezingCode: code === 56 || code === 57 || code === 66 || code === 67
    readonly property bool codeRime: code === 48
    // sleet: wet precipitation around 0 °C falls as a rain and snow mix
    readonly property bool sleet: (precip > 0 || codeWet || codeSnow) && temp > -1 && temp < 2.5 && !freezingCode && code < 95
    readonly property bool snowing: snowNow > 0 || (codeSnow && precip <= 0) || sleet
    readonly property bool raining: (!snowing || sleet) && (precip > 0 || codeWet || sleet)
    // rain rate in mm/h (coded rain without a measurement = typical for the code)
    readonly property real rate: precip > 0 ? precip * 4 : raining ? (drizzleCode ? 0.3 : heavyCode ? 6 : 1.2) : 0
    readonly property real coverPct: cover >= 0 ? cover
        : code <= 1 ? 5 : code === 2 ? 40 : 90
    readonly property real rainK: raining ? Math.min(1, 0.3 + 0.28 * Math.log2(1 + rate)) * (sleet ? 0.6 : 1) : 0
    readonly property real snowK: snowing ? Math.max(sleet ? 0.45 : 0, Math.min(1, (code === 77 ? 0.25 : 0.35) + 1.2 * snowNow)) : 0
    readonly property real fogK: Math.max(codeFog ? 0.75 : 0, Math.min(1, Math.max(0, (1200 - vis) / 900)))
    readonly property real hazeK: fogK > 0 ? 0 : Math.min(1, Math.max(0, (5000 - vis) / 4000))
    readonly property real stormK: code >= 95 ? 1 : 0
    readonly property real hailK: code === 96 || code === 99 ? 1 : 0
    readonly property real windK: gust >= 35 || wind >= 25 ? Math.min(1, Math.max(0.25, (Math.max(gust, wind) - 25) / 35)) : 0

    // wind: compass direction it blows TO; facing south east is left, north is behind the viewer
    readonly property real toRad: (windDir + 180) * Math.PI / 180
    readonly property real vx: -Math.sin(toRad)
    readonly property real hwind: wind * Math.max(0.2, Math.abs(vx))
    readonly property real slant: Math.tan(Math.max(-32, Math.min(32, (vx >= 0 ? 1 : -1) * hwind * 0.9)) * Math.PI / 180)
    readonly property real cloudSpeed: (vx >= 0 ? 1 : -1) * (0.035 + wind * 0.0045)  // world units / s (+ = west, to the right)

    // ------------------------------------------------------------ astronomy
    readonly property var sunP: Astro.sun(now, lat, lon)
    readonly property var moonP: Astro.moon(now, lat, lon)
    // the camera of shaders/clouds.frag: looks south, ray = ((x - .5) * aspect * FOV, (HOR - y) * FOV, 1)
    readonly property real fov: 1.8
    // the camera turns to what is worth seeing: the sun by day, the moon at night (centred, in the gap between
    // the two text columns), the rainbow (sun behind) when there is one, else due south
    readonly property real yaw: bowK > 0 ? sunP.az + 180
        : sunP.alt > -4 ? sunP.az
        : moonP.alt > 0 ? moonP.az : 180
    function dir(az, alt) {                      // camera frame: x right, y up, z forward
        const a = (az - yaw) * Math.PI / 180, e = alt * Math.PI / 180
        return { x: Math.sin(a) * Math.cos(e), y: Math.sin(e), z: Math.cos(a) * Math.cos(e) }
    }
    // tilt up when the body we face stands high (summer noon sun, a winter full moon): it stays near y 0.3
    readonly property real focusAlt: bowK > 0 ? 0 : sunP.alt > -4 ? sunP.alt : moonP.alt > 0 ? moonP.alt : 0
    readonly property real pitch: Math.max(0, focusAlt - 36) * Math.PI / 180
    function proj(az, alt) {                     // card position 0..1 (far outside when behind the camera)
        const w = dir(az, alt), asp = Math.max(1, width) / Math.max(1, height)
        const cp = Math.cos(pitch), sp = Math.sin(pitch)
        const d = { x: w.x, y: w.y * cp - w.z * sp, z: w.y * sp + w.z * cp }
        if (d.z < 0.05) return Qt.point(d.x > 0 ? 3 : -2, hor)
        return Qt.point(0.5 + d.x / d.z / (asp * fov), hor - d.y / d.z / fov)
    }
    readonly property point sunS: proj(sunP.az, sunP.alt)
    readonly property point moonS: proj(moonP.az, moonP.alt)
    readonly property var sunD: dir(sunP.az, sunP.alt)
    readonly property real moonLight: moonP.fraction * Math.max(0, Math.min(1, (moonP.alt + 1) / 9)) * (sunP.alt < -5 ? 1 : 0)
    readonly property real auroraK: kp >= 5.5 && sunP.alt < -10 ? Math.min(1, (kp - 5.5) / 2.5) * (1 - coverPct / 100) : 0
    readonly property real meteorK: sunP.alt < -10 && coverPct < 60 ? Math.min(0.55, 0.015 + Astro.meteorZhr(now) * 0.0035) : 0
    // rainbow: rain falling, sun shining through (not too much cloud), sun below 42°
    readonly property real bowK: rainK > 0 && sunP.alt > 0 && sunP.alt < 42 && coverPct < 85
        ? Math.min(1, rainK * 2) * (1 - coverPct / 100) * Math.min(1, (42 - sunP.alt) / 8) : 0

    // life: autumn leaves in the wind (Sep-Nov), birds by day in fair weather, sun shafts through broken cloud
    readonly property bool autumn: now.getMonth() >= 8 && now.getMonth() <= 10
    readonly property real leafK: autumn && !snowing && Math.max(wind, gust * 0.7) >= 12
        ? Math.min(1, (Math.max(wind, gust * 0.7) - 8) / 30) * (raining ? 0.5 : 1) : 0
    readonly property real birdK: sunP.alt > 2 && rainK < 0.5 && snowK <= 0 && stormK <= 0 && fogK < 0.5
        ? (wind > 35 ? 0.25 : 0.6) : 0
    readonly property real rayK: sunP.alt > -1 && coverPct > 15 && coverPct < 97 && fogK < 0.5
        ? Math.min(1, (coverPct - 15) / 25) * Math.min(1, (97 - coverPct) / 12) : 0

    // more weather: frost on the glass, misted glass, heat shimmer, snow on the ground, rain curtains,
    // snowflakes on the glass, 22° halo, fireflies on summer nights, pollen / dust in the sun
    readonly property int month: now.getMonth()
    readonly property real frostK: freezingCode || codeRime ? 0.85 : temp <= 0.5 ? Math.min(1, (1 - temp) / 7) : 0
    readonly property real mistK: hum >= 92 && temp <= 14 && !raining ? Math.min(1, (hum - 91) / 6) * 0.8 : 0
    readonly property real heatK: temp >= 25 && sunP.alt > 12 && coverPct < 60 && !raining ? Math.min(1, (temp - 24) / 8) : 0
    readonly property real gsnowK: snowDepth > 0.003 ? Math.min(1, 0.45 + snowDepth * 12) : snowing && temp <= 1 ? 0.35 : 0
    readonly property real curtainK: rainK > 0.55 || stormK > 0 ? Math.min(1, Math.max(stormK * 0.8, (rainK - 0.5) * 2.2)) : 0
    readonly property real glassSnowK: snowK > 0 && temp <= 1.5 ? Math.min(1, snowK * 1.4) : 0
    readonly property real haloK: !raining && !snowing && fogK < 0.3 && coverPct >= 30 && coverPct <= 85
        && ((sunP.alt > 4 && sunP.alt < 50) || (sunP.alt < -6 && moonP.alt > 5 && moonP.fraction > 0.5)) ? 0.8 : 0
    readonly property real fireflyK: month >= 5 && month <= 7 && sunP.alt < -4 && temp >= 14 && !raining && wind < 18 ? 1 : 0
    readonly property real moteK: month >= 2 && month <= 8 && sunP.alt > 8 && temp >= 12 && !raining && !snowing && coverPct < 70 && wind < 25 ? 0.8 : 0
    property real _frost: frostK
    property real _mist: mistK
    property real _heat: heatK
    property real _gsnow: gsnowK
    Behavior on _frost { NumberAnimation { duration: 5000; easing.type: Easing.InOutSine } }
    Behavior on _mist { NumberAnimation { duration: 4000; easing.type: Easing.InOutSine } }
    Behavior on _heat { NumberAnimation { duration: 3000 } }
    Behavior on _gsnow { NumberAnimation { duration: 4000 } }

    // smooth hand-over when a new reading arrives (every 10 min)
    property real _cov: coverPct / 100
    property real _rain: rainK
    property real _snow: snowK
    property real _fog: fogK
    property real _haze: hazeK
    property real _slant: slant
    Behavior on _cov { NumberAnimation { duration: 4000; easing.type: Easing.InOutSine } }
    Behavior on _rain { NumberAnimation { duration: 3000 } }
    Behavior on _snow { NumberAnimation { duration: 3000 } }
    Behavior on _fog { NumberAnimation { duration: 4000 } }
    Behavior on _haze { NumberAnimation { duration: 4000 } }
    Behavior on _slant { NumberAnimation { duration: 3000 } }

    // ------------------------------------------------------------ clock: time + wind drift, integrated
    // (integrating keeps clouds in place when the wind speed changes)
    property real t: 0
    property real offX: 0
    property real evo: 0                        // cloud shapes morph, faster in wind
    // cloud pass clock (kept separate so it can be throttled again if needed)
    property real ct: 0
    property real cOff: 0
    property real cEvo: 0
    property int _tick: 0
    property double _last: 0
    Timer {
        interval: sky.slow ? 200 : Math.round(1000 / sky.fps)
        repeat: true
        running: sky.animate && sky.visible && sky.width > 0
        onRunningChanged: sky._last = Date.now()
        onTriggered: {
            const n = Date.now(), dt = Math.min(0.25, (n - sky._last) / 1000)
            sky._last = n
            sky.t = (sky.t + dt) % 3600
            sky.offX = (sky.offX + sky.cloudSpeed * dt) % 1e6
            sky.evo += (0.03 + sky.wind * 0.001) * dt
            sky.strike()
            sky.ct = sky.t; sky.cOff = sky.offX; sky.cEvo = sky.evo
        }
    }

    // lightning schedule, shared by both passes: a strike may fall in each 3.2 s slot
    property vector4d bolt: Qt.vector4d(0, 0, 9, 0)
    function h1(a, b) { const x = Math.sin(a * 127.1 + b * 311.7) * 43758.5453; return x - Math.floor(x) }
    function strike() {
        if (stormK <= 0) { if (bolt.x !== 0) bolt = Qt.vector4d(0, 0, 9, 0); return }
        const slot = Math.floor(t / 3.2)
        if (h1(slot, 9.1) >= 0.55 * stormK) { if (bolt.x !== 0) bolt = Qt.vector4d(0, 0, 9, slot); return }
        const lt = t - slot * 3.2 - h1(slot, 8.3) * 2.2
        const f = lt <= 0 ? 0 : Math.exp(-lt * 7) + (lt >= 0.18 ? 0.7 * Math.exp(-(lt - 0.18) * 9) : 0)
        bolt = Qt.vector4d(f < 0.002 ? 0 : f, 0.15 + 0.7 * h1(slot, 7.7), lt, slot)
    }

    // pass A: sky + volumetric clouds at half resolution (soft by nature), into a texture
    ShaderEffect {
        id: cloudPass
        width: sky.width; height: sky.height
        blending: false
        fragmentShader: Qt.resolvedUrl("../shaders/clouds.frag.qsb")
        property real time: sky.ct
        property real fov: sky.fov
        property real pitch: sky.pitch
        property size size: Qt.size(width, height)
        property vector4d sunDir: Qt.vector4d(sky.sunD.x, sky.sunD.y, sky.sunD.z, sky.sunP.alt)
        property vector4d scene: Qt.vector4d(0, 0, sky.cOff, sky.cEvo)
        property vector4d wx: Qt.vector4d(sky._cov, sky._rain, sky._snow, sky._fog)
        property vector4d wx2: Qt.vector4d(sky.stormK, sky.moonLight, sky.auroraK, sky._haze)
        property vector4d bolt: sky.bolt
        property vector4d fx: Qt.vector4d(sky.bowK, sky.hor, sky._gsnow, 0)
    }
    ShaderEffectSource {
        id: cloudTex
        sourceItem: cloudPass
        hideSource: true
        smooth: true
        textureSize: Qt.size(Math.ceil(sky.width / 2), Math.ceil(sky.height / 2))
    }

    // pass B: sharp details on top, rounded corners
    ShaderEffect {
        anchors.fill: parent
        fragmentShader: Qt.resolvedUrl("../shaders/sky.frag.qsb")
        blending: true
        property var src: cloudTex
        property real time: sky.t
        property real radius: sky.radius
        property size size: Qt.size(width, height)
        property vector4d sun: Qt.vector4d(sky.sunS.x, sky.sunS.y, sky.sunP.alt, 0)
        property vector4d moon: Qt.vector4d(sky.moonS.x, sky.moonS.y, sky.moonP.alt, sky.moonP.fraction)
        // sun direction seen from the Moon: phase angle i, bright limb at angle z from the zenith (toward east = left)
        property vector4d moonLit: {
            const i = sky.moonP.inc, z = sky.moonP.limbZenith
            return Qt.vector4d(-Math.sin(z) * Math.sin(i), -Math.cos(z) * Math.sin(i), Math.cos(i), sky.moonP.pa)
        }
        property vector4d wx: Qt.vector4d(sky._cov, sky._rain, sky._snow, sky._fog)
        property vector4d wx2: Qt.vector4d(sky.stormK, sky.hailK, sky.rate < 0.5 ? 1 : 0, sky.windK)
        property vector4d fx: Qt.vector4d(sky._slant, 0, sky.meteorK, sky._haze)
        property vector4d bolt: sky.bolt
        property vector4d life: Qt.vector4d(Math.min(1, sky.boxH / Math.max(1, sky.height)), sky.leafK, sky.birdK, sky.rayK)
        property vector4d env: Qt.vector4d(sky.temp, sky.hum / 100, sky._frost, sky._heat)
        property vector4d env2: Qt.vector4d(sky._gsnow, sky.fireflyK, sky.moteK, sky.hor)
        property vector4d env3: Qt.vector4d(sky.haloK, sky._mist, sky.curtainK, sky.glassSnowK)
    }
}
