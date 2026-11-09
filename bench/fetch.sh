#!/bin/sh
# The shells and compositors bench/shells.sh and bench/compositors.sh compare, without root: the distribution's
# packages (and the dependencies missing on this system) are downloaded, checked against the SHA-256 sums in
# pacman's own sync databases, and unpacked into ~/.local/share/mist-bench/root. Nothing is installed;
# env.sh there points PATH and the library paths at it. Arch-based systems only; elsewhere install the
# packages and the scripts use the system ones.
#   bench/fetch.sh [package...]
set -eu
export LC_ALL=C
D=$HOME/.local/share/mist-bench
mkdir -p "$D/pkg" "$D/root"
[ $# -gt 0 ] || set -- waybar quickshell ironbar nwg-panel sway labwc weston
cd "$D/pkg"
pacman -Sp "$@" >urls.txt
while read -r u; do
    f=${u##*/}
    [ -s "$f" ] && file "$f" | grep -q Zstandard && continue
    curl -sfL -o "$f" "$u" && file "$f" | grep -q Zstandard || { echo "download failed: $u (try another mirror)" >&2; rm -f "$f"; }
done <urls.txt
python3 - <<'EOF'
import glob, hashlib, re, sys, tarfile
want = {}
for db in glob.glob("/var/lib/pacman/sync/*.db"):
    t = tarfile.open(db)
    for m in t.getmembers():
        if m.name.endswith("/desc"):
            d = t.extractfile(m).read().decode()
            f, s = re.search(r"%FILENAME%\n(.+)", d), re.search(r"%SHA256SUM%\n(.+)", d)
            if f and s:
                want.setdefault(f.group(1), s.group(1))
bad = [f for f in glob.glob("*.pkg.tar.zst") if want.get(f) != hashlib.sha256(open(f, "rb").read()).hexdigest()]
if bad:
    sys.exit("checksum mismatch, not unpacked: " + " ".join(bad))
EOF
cd "$D/root"
for f in "$D"/pkg/*.pkg.tar.zst; do
    tar --zstd -xf "$f" --exclude .PKGINFO --exclude .MTREE --exclude .BUILDINFO --exclude .INSTALL
done
cat >"$D/env.sh" <<EOF
# sourced by bench/shells.sh and bench/compositors.sh: the packages unpacked by bench/fetch.sh
R=$D/root
export PATH=\$R/usr/bin:\$PATH
export LD_LIBRARY_PATH=\$R/usr/lib\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}
export QT_PLUGIN_PATH=\$R/usr/lib/qt6/plugins:/usr/lib/qt6/plugins
export QML_IMPORT_PATH=\$R/usr/lib/qt6/qml:/usr/lib/qt6/qml QML2_IMPORT_PATH=\$R/usr/lib/qt6/qml:/usr/lib/qt6/qml
export XDG_DATA_DIRS=\$R/usr/share:/usr/share
export PYTHONPATH=\$(ls -d \$R/usr/lib/python3*/site-packages 2>/dev/null | tr '\n' ':')
export GI_TYPELIB_PATH=\$R/usr/lib/girepository-1.0
EOF
echo "unpacked $(ls "$D"/pkg/*.pkg.tar.zst | wc -l) packages into $D/root"
