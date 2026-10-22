#!/bin/sh
# overview (Mission Control): three windows one over another; the overview shows each in its own grid place
# (its colour at the place's centre), draws nothing while open and idle, a click on one focuses it and
# closes; three fingers up opens it, Escape closes
D=$(dirname "$0"); export KITTY_CONFIG_DIRECTORY=$(cd $D && pwd)/kittycfg; cd $D; ./unnest.sh >/dev/null 2>&1
./nest.sh >/dev/null; S=$(cat sock)
for c in ff0000:ov-red 00ff00:ov-green 0000ff:ov-blue; do
  WAYLAND_DISPLAY=$S setsid kitty --class ${c#*:} -o cursor_blink_interval=0 -o background=#${c%:*} >/dev/null 2>&1 &
  timeout 10 sh -c "until ./cc.sh windows | grep -q '${c#*:} ws1 '; do sleep 0.05; done"
done
timeout 0.5 tail -f /dev/null
px() { WAYLAND_DISPLAY=$S grim -g "$1,$2 1x1" - | magick - -format '%[pixel:p{0,0}]' info:; }
./cc.sh action overview >/dev/null; timeout 0.5 tail -f /dev/null
./cc.sh overview | while read o app at size rest; do
  x=$(( ${at%,*} + ${size%x*} / 2 )); y=$(( ${at#*,} + ${size#*x} / 2 + 10 ))
  echo "slot $app $at $size: centre $(px $x $y)"
done
a=$(./cc.sh stats | cut -d' ' -f2); timeout 1 tail -f /dev/null; b=$(./cc.sh stats | cut -d' ' -f2); echo "frames open + idle 1 s: $((b-a))"
set -- $(./cc.sh overview | grep ov-red); x=$(( ${3%,*} + ${4%x*} / 2 )); y=$(( ${3#*,} + ${4#*x} / 2 ))
./cc.sh motion $x $y >/dev/null; ./cc.sh button left press >/dev/null; ./cc.sh button left release >/dev/null; timeout 0.5 tail -f /dev/null
echo "clicked red: overview $(./cc.sh overview) | focus $(./cc.sh focus) | top $(./cc.sh windows | tail -1)"
./cc.sh swipe begin >/dev/null; ./cc.sh swipe 0 -40 >/dev/null; ./cc.sh swipe 0 -40 >/dev/null; ./cc.sh swipe end >/dev/null
timeout 0.4 tail -f /dev/null; echo "three fingers up: $(./cc.sh overview | wc -l) slots"
./cc.sh key 1 press >/dev/null; ./cc.sh key 1 release >/dev/null; timeout 0.4 tail -f /dev/null; echo "escape: $(./cc.sh overview)"
./unnest.sh >/dev/null 2>&1
