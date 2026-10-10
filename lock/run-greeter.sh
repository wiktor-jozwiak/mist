#!/bin/sh
# greetd -> noctalia-greeter-session (wlroots compositor) -> this script -> Quickshell greeter.qml.
# If Quickshell fails to start, fall back to the stock noctalia-greeter so login keeps working.
D=/usr/share/mist-greeter
P=/opt/mist-qs/usr
export LD_LIBRARY_PATH="$P/lib"
export QT_PLUGIN_PATH="$P/lib/qt6/plugins:/usr/lib/qt6/plugins"
export QML_IMPORT_PATH="$P/lib/qt6/qml:/usr/lib/qt6/qml"
export QML2_IMPORT_PATH="$QML_IMPORT_PATH"
export QT_QPA_PLATFORM=wayland
# the greeter compositor has no xdg-decoration: without this Qt draws its own title bar/frame around the window
export QT_WAYLAND_DISABLE_WINDOWDECORATION=1
export QT_QUICK_CONTROLS_STYLE=Basic
export QSG_RENDER_LOOP=basic
export XDG_CACHE_HOME=/tmp/mist-greeter/cache XDG_STATE_HOME=/tmp/mist-greeter/state XDG_DATA_HOME=/tmp/mist-greeter/data
mkdir -p /tmp/mist-greeter/cache /tmp/mist-greeter/state /tmp/mist-greeter/data
"$P/bin/quickshell" -p "$D/greeter.qml" >/tmp/mist-greeter/greeter.log 2>&1 && exit 0
exec /usr/bin/noctalia-greeter
