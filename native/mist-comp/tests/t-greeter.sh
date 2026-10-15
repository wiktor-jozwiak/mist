#!/bin/sh
# native greeter (dry run, no GREETD_SOCK: password "test"): mist-comp --session runs `mist-ui greeter`,
# GREETD=1: against fake-greetd.py (password pw123). A wrong password shakes, the right one ends the greeter and with it the compositor
D=$(cd $(dirname "$0") && pwd); cd $D; ./unnest.sh >/dev/null 2>&1
UI=$D/../../target/release/mist-ui
G=${GREETD:-}
[ -n "$G" ] && { python3 $D/fake-greetd.py $XDG_RUNTIME_DIR/mist-fake-greetd.sock & echo $! > fake.pid; timeout 5 sh -c "until [ -S $XDG_RUNTIME_DIR/mist-fake-greetd.sock ]; do sleep 0.1; done"; }
env -u GREETD_SOCK ${G:+GREETD_SOCK=$XDG_RUNTIME_DIR/mist-fake-greetd.sock} MIST_COMP_CONFIG=$D/greeter-comp.conf MIST_COMP_OUTPUT=DP-3 MIST_COMP_SIZE=1280x800 RUST_LOG=warn,mist_comp=info \
  setsid ../../target/release/mist-comp --headless --session "$UI greeter $D/greeter.json" >comp.log 2>&1 & echo $! > comp.pid
timeout 10 sh -c "until grep -q '^WAYLAND_DISPLAY=' comp.log; do sleep 0.1; done"; grep -m1 '^WAYLAND_DISPLAY=' comp.log | cut -d= -f2 > sock
timeout 10 sh -c "until ./cc.sh locked | grep -q 'true surfaces=1'; do sleep 0.1; done"; echo "greeter: $(./cc.sh locked)"
timeout 1 tail -f /dev/null; ./cc.sh shot $D/greeter.ppm >/dev/null; magick greeter.ppm -resize 40% greeter.png
k() { for c in "$@"; do ./cc.sh key $c press >/dev/null 2>&1; ./cc.sh key $c release >/dev/null 2>&1; done; }
# x y z Return (wrong), then t e s t Return
k 45 21 44 28; timeout 1.5 tail -f /dev/null
echo "after wrong: $(./cc.sh locked) comp alive: $(kill -0 $(cat comp.pid) 2>/dev/null && echo yes)"
if [ -n "$G" ]; then k 25 17 2 3 4 28; else k 20 18 31 20 28; fi
timeout 5 sh -c "while kill -0 $(cat comp.pid) 2>/dev/null; do sleep 0.1; done"
echo "after test: comp alive: $(kill -0 $(cat comp.pid) 2>/dev/null && echo yes || echo no)"
[ -n "$G" ] && { kill $(cat fake.pid); echo "greetd saw:"; cat fake-greetd.log; }
grep -q panicked comp.log && echo PANIC
./unnest.sh >/dev/null 2>&1
