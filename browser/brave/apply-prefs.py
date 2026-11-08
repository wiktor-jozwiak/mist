#!/usr/bin/env python3
"""Mist look + Zen-like vertical tabs + perf/privacy prefs for Brave's Default profile.
Brave must be closed (it rewrites Preferences on exit). Backup: Preferences.before-mist.
Also drops External Extensions json (Return YouTube Dislike, 2FAS) so Brave offers to install them."""
import json, os, shutil, subprocess, sys

UD = os.path.expanduser("~/.config/BraveSoftware/Brave-Browser")
PREF = os.path.join(UD, "Default", "Preferences")
MIST_ACC = 0xFF0A84FF  # native/common/tokens.rs ACCENT
HERE = os.path.dirname(os.path.abspath(__file__))
THEME_ID = "ndaefmlnomfobnhdkckniikicjebipib"  # mist-theme.crx (klucz: ~/.local/share/mist/brave-mist-theme.pem)

if subprocess.run(["pgrep", "-x", "brave"], capture_output=True).returncode == 0:
    sys.exit("Brave działa: zamknij go i uruchom ponownie")

def put(d, path, v):
    *ks, last = path.split(".")
    for k in ks:
        d = d.setdefault(k, {})
    d[last] = v

p = json.load(open(PREF))
if not os.path.exists(PREF + ".before-mist"):
    shutil.copy2(PREF, PREF + ".before-mist")

sk = MIST_ACC - (1 << 32)  # SkColor stored as signed int32
for k, v in {
    # Mist: ciemny, neutralny wariant z akcentem teal; treść bez ramki (zaokrąglona dawała ~12 px pustej ramki
    # także w zmaksymalizowanym oknie, Brave nie umie jej wyłączyć tylko przy maksymalizacji)
    "browser.theme.color_scheme2": 2,
    "browser.theme.user_color2": sk,
    "browser.theme.color_variant2": 2,
    "browser.theme.is_grayscale2": False,
    "brave.dark_mode": 1,
    "brave.web_view_rounded_corners": False,
    "brave.location_bar_is_wide": True,
    # Minimalny pasek jak w Zenie: bez paska zakładek, domu, Leo, Rewards, Wallet, News
    "bookmark_bar.show_on_all_tabs": False,
    "browser.show_home_button": False,
    "brave.show_bookmarks_button": False,
    "brave.rewards.show_brave_rewards_button_in_location_bar": False,
    "brave.wallet.show_wallet_icon_on_toolbar": False,
    "brave.ai_chat.show_toolbar_button": False,
    "brave.today.should_show_toolbar_button": False,
    "brave.tabs_search_show": False,
    "brave.mru_cycling_enabled": True,             # Ctrl+Tab = ostatnio używane (jak Zen)
    # Pionowe karty jak Zen: zwinięte całkiem schowane, rozwijają się po najechaniu nad treścią
    "brave.tabs.vertical_tabs_enabled": True,
    "brave.tabs.vertical_tabs_collapsed": True,
    "brave.tabs.vertical_tabs_floating_enabled": True,
    "brave.tabs.vertical_tabs_hide_completely_when_collapsed": True,  # zwinięte = 0 px; wysuwa je najechanie na lewą krawędź
    "brave.tabs.vertical_tabs_expanded_width": 248,
    "brave.tabs.vertical_tabs_on_right": False,
    "brave.tabs.vertical_tabs_show_title_on_window": False,
    "brave.tabs.vertical_tabs_show_toggle_button": True,
    "brave.tabs.vertical_tabs_show_scrollbar": False,
    "brave.tabs.vertical_tabs_expanded_state_per_window": False,
    "brave.tabs.tree_tabs_enabled": False,
    "brave.tabs.shared_pinned_tab": True,          # przypięte = Essentials, wspólne dla okien
    "brave.tabs.always_hide_tab_close_button": False,
    "brave.tabs.hover_mode": 1,                    # podgląd karty jako tooltip, bez obrazka
    # Boczny panel Brave tylko na żądanie (nie dubluje paska kart)
    "brave.sidebar.sidebar_show_option": 3,
    "brave.show_side_panel_button": False,
    # Jak Zen: przywracaj poprzednią sesję
    "session.restore_on_startup": 1,
    # Wydajność: Memory Saver (zrzut nieaktywnych kart)
    "performance_tuning.high_efficiency_mode.state": 1,
    "performance_tuning.battery_saver_mode.state": 0,
    # Prywatność
    "webrtc.ip_handling_policy": "default_public_interface_only",
    "enable_do_not_track": False,                  # DNT = dodatkowy bit fingerprintu; Brave wysyła GPC
    "brave.de_amp.enabled": True,
    "brave.debounce.enabled": True,
    "brave.reduce_language": True,
    "profile.password_manager_enabled": False,
    "credentials_enable_service": False,
    "autofill.credit_card_enabled": False,
    "autofill.profile_enabled": False,
    "brave.new_tab_page.show_sponsored_images": False,
    "brave.new_tab_page.show_rewards": False,
    "brave.new_tab_page.show_brave_news": False,
    "brave.new_tab_page.show_together": False,
    "brave.new_tab_page.show_brave_vpn": False,
}.items():
    put(p, k, v)

tmp = PREF + ".tmp"
json.dump(p, open(tmp, "w"), separators=(",", ":"))
os.replace(tmp, PREF)

ext = os.path.join(UD, "External Extensions")
os.makedirs(ext, exist_ok=True)
for eid in ("gebbhagfogifgggkldgodflihgfeippi",   # Return YouTube Dislike
            "dbfoemgnkgieejfkaddieamagdfepnff"):  # 2FAS Auth
    json.dump({"external_update_url": "https://clients2.google.com/service/update2/crx"},
              open(os.path.join(ext, eid + ".json"), "w"))
# Motyw Mist (lokalny CRX)
json.dump({"external_crx": os.path.join(HERE, "mist-theme.crx"), "external_version": "1.1.0"},
          open(os.path.join(ext, THEME_ID + ".json"), "w"))
print("OK")
