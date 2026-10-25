#!/bin/sh
# overview with several workspaces (niri): red + green on 1, blue on 2, yellow on 3; the overview shows one card
# per workspace, the hidden ones' windows drawn too (their colour at their place); the keyboard moves the
# selection (→ in the row, ↓ to the next workspace, Tab through all) and Enter goes there (workspace + focus)
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
SIZE=1280x900 ./nest.sh >/dev/null; S=$(cat sock)
win() { WAYLAND_DISPLAY=$S setsid kitty --class $2 -o cursor_blink_interval=0 -o background=#$1 >/dev/null 2>&1 &
  timeout 10 sh -c "until ./cc.sh windows | grep -q '$2 ws$3 '; do sleep 0.05; done"; }
win ff0000 ow-red 1; win 00ff00 ow-green 1
./cc.sh action workspace 2 >/dev/null; win 0000ff ow-blue 2
./cc.sh action workspace 3 >/dev/null; win ffff00 ow-yellow 3
./cc.sh action workspace 1 >/dev/null; timeout 0.6 tail -f /dev/null
px() { WAYLAND_DISPLAY=$S grim -g "$1,$2 1x1" - | magick - -format '%[pixel:p{0,0}]' info:; }
key() { ./cc.sh key $1 press >/dev/null; ./cc.sh key $1 release >/dev/null; timeout 0.35 tail -f /dev/null; }
./cc.sh action overview >/dev/null; timeout 0.5 tail -f /dev/null
show() { ./cc.sh overview | while read o app at size ws rest; do
  x=$(( ${at%,*} + ${size%x*} / 2 )); y=$(( ${at#*,} + ${size#*x} / 2 + 10 ))
  c=$( [ $y -gt 0 ] && [ $y -lt 900 ] && px $x $y || echo offscreen ); echo "  $app $ws ${rest:-} centre $c"; done; }
echo "open:"; show
key 106; echo "→: $(./cc.sh overview | grep sel | cut -d' ' -f2)"
key 108; timeout 0.3 tail -f /dev/null; echo "↓ (scrolled):"; show
key 15; echo "Tab: $(./cc.sh overview | grep sel | cut -d' ' -f2)"
key 28; timeout 0.5 tail -f /dev/null
echo "Enter: overview $(./cc.sh overview) | $(./cc.sh windows | grep -v '^$' | awk '{print $1, $2}' | tr '\n' ' ')| focus $(./cc.sh focus)"
./unnest.sh >/dev/null 2>&1
