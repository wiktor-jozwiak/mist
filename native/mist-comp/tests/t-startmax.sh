#!/bin/sh
# a window maximized / fullscreen before its first frame opens that way (mpv --fs, a browser closed maximized)
D=$(dirname "$0"); cd $D
for m in max full; do
  ./unnest.sh >/dev/null 2>&1
  ./nest.sh env GDK_BACKEND=wayland python3 $PWD/startmax.py $m >/dev/null
  timeout 10 sh -c "until ./cc.sh windows | grep -q startmax; do sleep 0.1; done"; sleep 0.6
  w=$(./cc.sh windows | grep startmax); echo "$m: $w"
  s=$(echo $w | grep -oE "[0-9]+x[0-9]+" | head -1)
  case $m in
    max) [ ${s%x*} -gt 1200 ] || echo "FAIL: $m not maximized" ;;
    full) [ "$s" = "1280x800" ] || echo "FAIL: $m not fullscreen" ;;
  esac
  grep -q panicked comp.log && echo PANIC
done
./unnest.sh >/dev/null 2>&1
echo "result: done"
