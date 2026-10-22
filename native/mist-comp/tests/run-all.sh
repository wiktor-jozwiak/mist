#!/bin/sh
# every nested test, one summary line each; full output in <test>.out; exit 1 if any failed
# env: CI=1 (throwaway XDG_RUNTIME_DIR, Mesa's software renderer, hyprctl = the built mist-comp: no GPU, no
# session needed), SKIP="t-a t-b" (tests left out), ONLY="t-a t-b"
D=$(dirname "$0"); cd $D
T=$(cd ../../target/release && pwd)
if [ -n "${CI:-}" ]; then
  export XDG_RUNTIME_DIR=$(mktemp -d /tmp/mrun.XXXX) MIST_COMP_SOFTWARE=1 LIBGL_ALWAYS_SOFTWARE=1
  # the Mist session may hide Mesa from EGL (start-mist on NVIDIA): llvmpipe needs it
  unset __EGL_VENDOR_LIBRARY_FILENAMES
  chmod 700 $XDG_RUNTIME_DIR; mkdir -p $XDG_RUNTIME_DIR/mist
  mkdir -p $XDG_RUNTIME_DIR/bin; ln -sf $T/mist-comp $XDG_RUNTIME_DIR/bin/hyprctl; export PATH=$XDG_RUNTIME_DIR/bin:$PATH
  unset WAYLAND_DISPLAY DISPLAY HYPRLAND_INSTANCE_SIGNATURE
fi
failed=0
for t in ${ONLY:-t-*.sh}; do
  t=${t%.sh}.sh
  case " ${SKIP:-} " in *" ${t%.sh} "*) echo "$t skipped"; continue ;; esac
  timeout 120 ./$t > ${t%.sh}.out 2>&1; ./unnest.sh >/dev/null 2>&1
  bad=$(grep -ciE 'panic|fail|error|timeout' ${t%.sh}.out)
  [ "$bad" -gt 0 ] && failed=$((failed + 1))
  echo "$t bad=$bad | $(grep -v '^\s*$' ${t%.sh}.out | tail -1 | cut -c1-110)"
done
[ -n "${CI:-}" ] && rm -rf "$XDG_RUNTIME_DIR"
echo "failed: $failed"
[ $failed -eq 0 ]
