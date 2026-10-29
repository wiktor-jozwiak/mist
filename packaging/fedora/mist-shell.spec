# Mist for Fedora (COPR: enable "network access during builds", cargo fetches the locked crates).
#   rpmbuild -ba packaging/fedora/mist-shell.spec --define "commit $(git rev-parse HEAD)"
# Same layout as packaging/arch/PKGBUILD: binaries in /usr/bin, data in /usr/share/mist (common/paths.rs),
# the Mist session in /usr/share/wayland-sessions. Hyprland is optional here: the Mist session runs on mist-comp.
%global commit %{?commit}%{!?commit:HEAD}
%global shortcommit %(c=%{commit}; echo ${c:0:7})
# cargo builds with its own (stripped) profile
%global debug_package %{nil}

Name:           mist-shell
Version:        0^%(date +%Y%m%d)git%{shortcommit}
Release:        1%{?dist}
Summary:        macOS-inspired desktop shell and Wayland compositor, written in Rust and drawn on the CPU
License:        GPL-3.0-only
URL:            https://github.com/wiktor-jozwiak/mist
Source0:        %{url}/archive/%{commit}/mist-%{commit}.tar.gz

ExclusiveArch:  x86_64
BuildRequires:  cargo rust pkgconf-pkg-config gcc
BuildRequires:  pkgconfig(wayland-server) pkgconfig(wayland-client) pkgconfig(xkbcommon) pkgconfig(egl)
BuildRequires:  pkgconfig(gbm) pkgconfig(libdrm) pkgconfig(libinput) pkgconfig(libseat) pkgconfig(libudev)
BuildRequires:  pam-devel
Requires:       wayland mesa-libEGL mesa-libgbm libinput libseat pam
Requires:       pipewire-pulseaudio wireplumber wl-clipboard grim slurp xdg-utils xdg-user-dirs
Recommends:     xdg-desktop-portal-hyprland xorg-x11-server-Xwayland google-noto-emoji-fonts
Suggests:       hyprland NetworkManager bluez kde-connect android-tools mpv-libs

%description
Mist: bar, dock, dynamic island, desktop widgets, panels and apps (mist-ui), live wallpapers (mist-wall),
backends (mistd) and its own Wayland compositor (mist-comp), all drawn on the CPU except the compositor.
Pick "Mist" in the login manager, or run the shell on Hyprland.

%prep
%autosetup -n mist-%{commit}

%build
# native/.cargo/config.toml builds for the author's CPU: a package builds for any x86-64
export RUSTFLAGS="" CARGO_BUILD_RUSTFLAGS=""
# GCC LTO bytecode from the crates' C code (bundled SQLite) does not link into a Rust binary
export CFLAGS="${CFLAGS:-} -fno-lto" CXXFLAGS="${CXXFLAGS:-} -fno-lto"
(cd native && cargo build --release --locked)
(cd scripts/mistd && cargo build --release --locked)

%check
export RUSTFLAGS="" CARGO_BUILD_RUSTFLAGS=""
(cd native && cargo test --release --locked -p mist-ui)

%install
install -Dm755 native/target/release/mist-ui %{buildroot}%{_bindir}/mist-ui-bin
install -Dm755 native/target/release/mist-wall %{buildroot}%{_bindir}/mist-wall-bin
install -Dm755 native/target/release/mist-comp %{buildroot}%{_bindir}/mist-comp
install -Dm755 scripts/mistd/target/release/mistd %{buildroot}%{_bindir}/mistd
install -Dm755 native/dist/mist-ui %{buildroot}%{_bindir}/mist-ui
share=%{buildroot}%{_datadir}/mist
install -d $share/native $share/scripts $share/shaders $share/examples $share/native/mist-comp/session
cp -r native/assets $share/native/
install -m755 scripts/*.sh $share/scripts/
install -m644 shaders/*.glsl $share/shaders/
cp -r shaders/glsl $share/shaders/
cp -r examples/hyprland $share/examples/
cp -r docs $share/
install -m755 native/mist-comp/session/start-mist native/mist-comp/session/mist-session $share/native/mist-comp/session/
install -m644 native/mist-comp/session/mist-portals.conf $share/native/mist-comp/session/
install -Dm644 packaging/mist.desktop %{buildroot}%{_datadir}/wayland-sessions/mist.desktop

%files
%license LICENSE NOTICE.md native/assets/LICENSE.noctalia
%{_bindir}/mist-ui
%{_bindir}/mist-ui-bin
%{_bindir}/mist-wall-bin
%{_bindir}/mist-comp
%{_bindir}/mistd
%{_datadir}/mist/
%{_datadir}/wayland-sessions/mist.desktop

%changelog
* Thu Oct 22 2026 Wiktor Józwiak <wjozwiak92@gmail.com> - 0^20261022git-1
- First spec: mist-ui, mist-wall, mistd, mist-comp and the Mist session
