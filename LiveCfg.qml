pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Wallpaper settings (~/.config/mist/wallpaper.json: {"live": "<preset id>" | "", "image": "<photo path>",
// "tempo": "calm|normal|fast", "music": bool}), shared by LiveWallpaper.qml (every monitor, draws the photo
// while no live preset is chosen), Wallpapers.qml and the lock screen (always the photo).
// music: while a player is playing, one capture of the default output's loudness (parec on its monitor
// source, RMS with automatic gain in awk, ~30 values/s, slow envelope: follows loudness, not single beats) -> `level` 0..1.
Singleton {
    id: root
    property string live: ""
    readonly property string defImage: Quickshell.env("HOME") + "/Obrazy/Tapety/glow-ember.png"
    property string image: defImage
    property string tempo: "normal"
    property bool music: false

    function save(o) {
        if (o.live !== undefined) live = o.live
        if (o.image !== undefined) image = o.image
        if (o.tempo !== undefined) tempo = o.tempo
        if (o.music !== undefined) music = o.music
        cfgF.setText(JSON.stringify({ live: live, image: image, tempo: tempo, music: music }, null, 2))
    }

    FileView {
        id: cfgF
        path: Quickshell.env("HOME") + "/.config/mist/wallpaper.json"
        watchChanges: true; printErrors: false
        onFileChanged: reload()
        onLoaded: {
            try {
                const c = JSON.parse(text())
                root.live = c.live || ""; root.image = c.image || root.defImage; root.tempo = c.tempo || "normal"; root.music = !!c.music
            } catch (e) { root.live = "" }
        }
        onLoadFailed: root.live = ""
    }

    // the music level for the scene is measured by mist-wall itself (native/mist-wall)
}
