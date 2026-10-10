pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// NetworkManager events for the whole shell: one `nmcli monitor` (event stream, no polling). Island reads the
// lines (connectivity, primary connection, VPN); `type` (ethernet / wifi / none) for the bar is re-read
// with one `nmcli device` only when NetworkManager reports a change.
Singleton {
    id: root
    property string type: "ethernet"
    signal line(string l)

    Process {
        id: mon; running: true
        environment: ({ LC_ALL: "C", LANG: "C" })
        // setpriv --pdeathsig: the kernel stops the helper if Quickshell dies (a crash skips onDestruction)
        command: ["setpriv", "--pdeathsig", "TERM", "nmcli", "monitor"]
        stdout: SplitParser { onRead: l => { root.line(l); devT.restart() } }
        onExited: restartT.restart()
    }
    Timer { id: restartT; interval: 15000; onTriggered: mon.running = true }
    Timer { id: devT; interval: 400; onTriggered: dev.running = true }
    Process {
        id: dev
        command: ["nmcli", "-t", "-f", "TYPE,STATE", "device"]
        stdout: StdioCollector {
            onStreamFinished: {
                const l = this.text.split("\n").filter(x => x.endsWith(":connected"))
                root.type = l.some(x => x.startsWith("ethernet")) ? "ethernet" : l.some(x => x.startsWith("wifi")) ? "wifi" : "none"
            }
        }
    }
    Component.onCompleted: dev.running = true
}
