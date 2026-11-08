#!/bin/sh
# Polityki przeglądarek (wymaga root): Brave -> /etc/brave/policies/managed, Zen -> /opt/zen-browser-bin/distribution
# Użycie: sudo ~/.config/quickshell/mist/browser/install-policies.sh
set -e
here=$(dirname "$(readlink -f "$0")")
# Lokalne CA Vaultwarden (nameConstraints: tylko localhost/127.0.0.1) -> systemowy p11-kit (Brave); Zen bierze plik przez policy Certificates
ca="/home/${SUDO_USER:-$USER}/.local/share/vaultwarden/tls/ca.pem"
if [ -f "$ca" ]; then install -Dm644 "$ca" /etc/ca-certificates/trust-source/anchors/mist-local-ca.pem; update-ca-trust; fi
install -Dm644 "$here/brave/policies/mist.json" /etc/brave/policies/managed/mist.json
if [ -d /opt/zen-browser-bin/distribution ]; then
  [ -f /opt/zen-browser-bin/distribution/policies.json.before-mist ] || cp /opt/zen-browser-bin/distribution/policies.json /opt/zen-browser-bin/distribution/policies.json.before-mist
  install -m644 "$here/zen/policies.json" /opt/zen-browser-bin/distribution/policies.json
fi
echo "OK: sprawdź brave://policy i about:policies (Zen)"
