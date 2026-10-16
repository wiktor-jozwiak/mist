# Zen Browser in the Mist style

[Polski](zen.pl.md)

`integrations/zen/` turns [Zen Browser](https://zen-browser.app) into a Mist window: the sidebar is liquid
glass over the blurred wallpaper (like the bar and dock), the page floats on it as a card with the Mist rim
and shadow, and every menu, popup, panel, tooltip, dialog and the find bar is drawn as a Mist pane (tinted
glass, 1 px lens rim, soft shadow, blue `#0A84FF` rows). The floating address bar looks like Spotlight; the
window controls are macOS traffic lights; toasts look like the Dynamic Island. Fonts follow mist-ui:
SF Pro, else Inter, else Noto Sans, for the browser UI and as the default sans-serif for pages; SF Mono or
JetBrains Mono for code. The settings, add-ons, about:config and new-tab pages use the same palette, with
green toggles and System Settings–style sidebar rows.

| File | What |
|---|---|
| `chrome/mist/tokens.css` | the Mist tokens (`theme.rs`, `glass.rs`) as CSS variables |
| `chrome/mist/chrome.css` | browser UI: window, sidebar, tabs, urlbar, menus, panels, dialogs, Zen parts |
| `chrome/mist/content.css` | about: pages |
| `user.js` | prefs: load the stylesheets, transparency, dark mode, accent, fonts, overlay scrollbars |

## Install

```bash
~/.config/quickshell/mist/integrations/zen/install.sh            # every profile in profiles.ini
~/.config/quickshell/mist/integrations/zen/install.sh <profile>  # just one
```

It looks for profiles in `~/.zen`, `~/.config/zen` and the Flatpak directory, copies `chrome/` into each,
and writes the prefs into `<profile>/user.js` between `// mist-begin` and `// mist-end`. An existing
`userChrome.css` / `userContent.css` is kept as `*.before-mist`; your own rules stay and the Mist `@import`s
go on top. An older hand-made Mist theme (a file with `MIST SHELL THEME` in it) is replaced instead, since
kept below the `@import`s it would override this one. Run it again after updating Mist. Restart Zen.

Undo: `install.sh --uninstall`. Prefs already written to `prefs.js` keep their value until reset in
`about:config`.

## Hyprland

The sidebar is translucent; Hyprland blurs what is behind it as long as `decoration.blur` is on (the
default). For the menus and popups too, set `decoration.blur.popups = true`. If the window
looks flat, check that no window rule forces `opaque` on Zen.

What floats over the page (the compact-mode sidebar, the floating address bar, the find bar, dialogs) is
solid glass tone: Zen cannot blur the page under it, and a see-through pane shows the page text behind
the tabs.

The yellow light minimizes through the click position (Hyprland ignores the app's minimize request): its
centre is 41, 20 px from the window's top-left corner; `ZEN_MIN` in the Hyprland config has to match.

## Tips

- Without SF Pro installed, Inter Variable is used (`pacman -S inter-font`).
- Workspace colours are switched off (`zen.theme.gradient`), so every workspace is the same glass. Delete that
  line from `user.js` to get them back.
- Zen renames elements between releases; whatever no longer matches simply keeps the stock look. Use the
  Browser Toolbox (`Ctrl+Alt+Shift+I`) to find the new selector and adjust `chrome/mist/chrome.css`.
