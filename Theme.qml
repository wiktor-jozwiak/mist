pragma Singleton
import QtQuick
import Quickshell

// Mist tokens: the same values as seelen-widgets/shared/mist/tokens.css on Windows.
Singleton {
    // Apple dark-mode label colours: primary / secondary (60 %) / tertiary (30 %) / quaternary (18 %)
    readonly property color t1: Qt.rgba(1, 1, 1, 0.96)
    readonly property color t2: Qt.rgba(235/255, 235/255, 245/255, 0.6)
    readonly property color t3: Qt.rgba(235/255, 235/255, 245/255, 0.3)
    readonly property color t4: Qt.rgba(235/255, 235/255, 245/255, 0.18)

    readonly property color base: Qt.rgba(14/255, 19/255, 29/255, 1)
    readonly property color veil: Qt.rgba(11/255, 15/255, 24/255, 0.62)
    readonly property color sheet: Qt.rgba(14/255, 19/255, 29/255, 0.86)
    readonly property color inset: Qt.rgba(4/255, 7/255, 14/255, 0.26)
    readonly property color hair: Qt.rgba(200/255, 214/255, 236/255, 0.1)
    readonly property color rim: Qt.rgba(214/255, 224/255, 242/255, 0.16)
    readonly property color fill: Qt.rgba(1, 1, 1, 0.055)
    readonly property color fill2: Qt.rgba(1, 1, 1, 0.1)

    // Apple system colours (dark mode)
    readonly property color accent: "#0A84FF"
    readonly property color accentStrong: "#409CFF"
    readonly property color accentSoft: Qt.rgba(10/255, 132/255, 255/255, 0.18)
    readonly property color violet: "#BF5AF2"
    readonly property color teal: "#64D2FF"
    readonly property color orange: "#FF9F0A"
    readonly property color green: "#30D158"
    readonly property color yellow: "#FFD60A"
    readonly property color danger: "#FF453A"
    readonly property color pink: "#FF375F"
    readonly property color indigo: "#5E5CE6"
    readonly property color ink: "#0e1420"

    // Mist palette (desktop cards, Start): shades of white, one teal accent from the wallpaper, amber warning, red only
    // when something is really bad. Secondary text never below 0.6 opacity.
    readonly property color mFg: Qt.rgba(244/255, 247/255, 251/255, 0.97)
    readonly property color mFg2: Qt.rgba(222/255, 230/255, 242/255, 0.86)
    readonly property color mMuted: Qt.rgba(206/255, 216/255, 232/255, 0.74)
    readonly property color mFaint: Qt.rgba(196/255, 208/255, 226/255, 0.6)
    readonly property color mAcc: "#8ad6d0"
    readonly property color mAccStrong: "#a8e4df"
    readonly property color mAccSoft: Qt.rgba(138/255, 214/255, 208/255, 0.16)
    readonly property color mBlue: "#9dbdf0"
    readonly property color mWarn: "#ecc48e"
    readonly property color mBad: "#ee95a3"
    readonly property color mTrack: Qt.rgba(200/255, 214/255, 236/255, 0.12)

    readonly property string font: "SF Pro"                  // variable: wght + opsz (Text <-> Display)
    readonly property string fontRounded: "SF Pro Rounded"   // numbers, like Fitness / Health
    readonly property real rSheet: 26
    readonly property real rPanel: 18
    readonly property real rRow: 14
    readonly property real rCard: 26

    // type scale (desktop cards, bar): caption / small / body / lead / title / hero
    readonly property real fsXs: 11
    readonly property real fsSm: 12
    readonly property real fsMd: 13
    readonly property real fsLg: 15
    readonly property real fsXl: 22
    readonly property real fsHero: 34

    readonly property string iconFont: tabler.name
    FontLoader { id: tabler; source: "file:///usr/share/noctalia/assets/fonts/noctalia-tabler.ttf" }

    // tabler codepoints (from /usr/share/noctalia/assets/fonts/tabler.json)
    readonly property var ic: ({
        search: "", bluetooth: "", wifi: "", volume: "", volumeOff: "",
        cpu: "", updown: "", bell: "", grid: "", power: "",
        sun: "", moon: "", cloud: "", rain: "", snow: "", fog: "",
        storm: "", cloudSun: "", play: "", pause: "", next: "", prev: "",
        calendar: "", plus: "", left: "", right: "", refresh: "",
        desktop: "", trash: "", run: "", heart: "", moonStars: "",
        drop: "", wind: "", battery: "", sunrise: "", sunset: "",
        apps: "", logout: "", reload: "", settings: "", lock: "",
        folder: "", file: "", x: "", terminal: "", sparkles: "",
        network: "", ethernet: "", heart: "", activity: "", bellOff: "",
        phone: "", bright: "", mic: "", headphones: "", calc: "", world: "",
        clock: "", pin: "", bulb: "", camera: "", screenshot: "", music: "",
        micOff: "\ued16", keyboard: "\uebd6", volLow: "\ueb4f", btOn: "\uecea", btOff: "\ueceb",
        worldOff: "\uf1ca", worldOk: "\uf9db", calEvent: "\uea52", bellRing: "\ued07", shuffle: "\uf000",
        repeat: "\ueb72", repeatOne: "\ueb71", playF: "\uf691", pauseF: "\uf690", nextF: "\uf694",
        prevF: "\uf693", charging: "\uea33", plug: "\uf00a",
        ram: "\ueef7", down: "\uea16", up: "\uea25", tool: "\ueb40", layoutGrid: "\uedba", bolt: "\uea38",
        chevron: "\uea61", temp: "\ueb38", flame: "\uec2c", zzz: "\uf228", hrm: "\uef61", walk: "\uec87",
        target: "\uf51a", trophy: "\ueb45", flag: "\ueaa6", umbrella: "\uebf1", category: "\uf1f6", dots: "\uea95",
        minus: "\ueaf2", gridDots: "\ueaba", photo: "", clipboard: "", call: "",
        usb: "\uf00c", mouse: "\ueaf9", gamepad: "\uf1d2", printer: "\ueb0e", video: "\ued22", videoOff: "\ued20",
        clipOk: "\uea6c", copy: "\uea7a", palette: "\ueb01", files: "\uedef", fileText: "\ueaa2", leaf: "\ued4f",
        gauge: "\ueab1", rocket: "\uec45", capsLock: "\uefef", numbers: "\uf015", alert: "\uea06", hourglass: "\uef93",
        alarm: "\uea04", shield: "\ued58", battLow: "\uff1d", plugX: "\uf0a0", eject: "\uefbc", command: "\uea78",
        window: "\uefe6", layers: "\ueaca", pinned: "\ued60", screenOn: "\uf86a", screenOff: "\uee78", link: "\ueade",
        stopwatch: "\uff9b", sdCard: "\uf384", drive: "\uf07c", check: "\uea67", folderOpen: "\ufaf7", mobile: "\uea8a",
        watch: "\uebf9", headset: "\ueb90", bellZ: "\ueff1", stop: "\ued4a", lockOpen: "\ueae1", refreshAlert: "\ued57",
        message: "\ueaef", send: "\ueb1e", fileUp: "\uec91", mobileOff: "\uf062", mobileVib: "\ueb86", mobileShare: "\uf8bc",
        unlink: "\ueb46", fileZip: "\ued4e", bars1: "\uecc7", bars2: "\uecc8", bars3: "\uecc9", bars4: "\uecca", bars5: "\ueccb", barsOff: "\uf0aa",
        locationF: "\uf67f", appleLogo: "\ufd74", callEnd: "\uff9d", callIn: "\ueb06", callOut: "\ueb07", callRing: "\uff9c", callOff: "\uecf5", dialpad: "\uf067", speaker: "\uea8b", btLink: "\uecea",
        adjust: "\uec38", chevLeft: "\uea60", tv: "\uea8d", router: "\ueb18", sunMoon: "\uf4a3", tick: "\uea5e"
    })

    function wxIcon(code, day) {
        if (code === 0 || code === 1) return day === 0 ? "moon" : "sun"
        if (code === 2) return day === 0 ? "moonStars" : "cloudSun"
        if (code === 3) return "cloud"
        if (code === 45 || code === 48) return "fog"
        if (code >= 71 && code <= 77 || code === 85 || code === 86) return "snow"
        if (code >= 95) return "storm"
        return "rain"
    }
}
