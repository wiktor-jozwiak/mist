-- Mist on Hyprland (Lua config, Hyprland >= 0.55). Load it from hyprland.lua:
--   dofile(os.getenv("HOME") .. "/.config/quickshell/mist/examples/hyprland/mist.lua")
-- or copy the parts you want. Everything goes through `mist-ui-bin msg <target> <function> [args]`.

local home = os.getenv("HOME")
local mist = home .. "/.local/bin/mist-ui-bin msg "
local scripts = home .. "/.config/quickshell/mist/scripts/"
local mainMod = "SUPER"

-- autostart: the guard restarts mist-ui after a crash (gives up after 5 crashes in 2 min); mist-ui starts
-- mist-wall (wallpaper) and the mistd backends itself
hl.on("hyprland.start", function()
    hl.exec_cmd("dbus-update-activation-environment --systemd --all")
    hl.exec_cmd(home .. "/.local/bin/mist-ui")
end)

-- panels
hl.bind(mainMod .. " + Space",       hl.dsp.exec_cmd(mist .. "mist toggle spotlight"))
hl.bind(mainMod .. " + SUPER_L",     hl.dsp.exec_cmd(mist .. "mist toggle start"), { release = true })
hl.bind(mainMod .. " + Tab",         hl.dsp.exec_cmd(mist .. "switcher next"))
hl.bind(mainMod .. " + SHIFT + Tab", hl.dsp.exec_cmd(mist .. "switcher prev"))
hl.bind(mainMod .. " + I",           hl.dsp.exec_cmd(mist .. "mist toggle control"))
hl.bind(mainMod .. " + A",           hl.dsp.exec_cmd(mist .. "mist toggle notifications"))
hl.bind(mainMod .. " + V",           hl.dsp.exec_cmd(mist .. "mist toggle clipboard"))
hl.bind(mainMod .. " + period",      hl.dsp.exec_cmd(mist .. "mist toggle emoji"))
hl.bind(mainMod .. " + Y",           hl.dsp.exec_cmd(mist .. "mist toggle phone"))
hl.bind(mainMod .. " + SHIFT + W",   hl.dsp.exec_cmd(mist .. "mist toggle wallpaper"))
hl.bind(mainMod .. " + ALT + C",     hl.dsp.exec_cmd(mist .. "mist toggle session"))
hl.bind(mainMod .. " + L",           hl.dsp.exec_cmd(home .. "/.local/bin/mist-ui-bin lock"))

-- media and brightness (brightness over DDC/CI: ddcutil)
hl.bind("XF86AudioPlay",  hl.dsp.exec_cmd(mist .. "mist media toggle"), { locked = true })
hl.bind("XF86AudioPause", hl.dsp.exec_cmd(mist .. "mist media toggle"), { locked = true })
hl.bind("XF86AudioNext",  hl.dsp.exec_cmd(mist .. "mist media next"),   { locked = true })
hl.bind("XF86AudioPrev",  hl.dsp.exec_cmd(mist .. "mist media prev"),   { locked = true })
hl.bind("XF86MonBrightnessUp",   hl.dsp.exec_cmd(scripts .. "bright.sh up"),   { locked = true, repeating = true })
hl.bind("XF86MonBrightnessDown", hl.dsp.exec_cmd(scripts .. "bright.sh down"), { locked = true, repeating = true })

-- screenshots (grim + slurp, toolbar and thumbnail drawn by mist-ui)
hl.bind("Print",                     hl.dsp.exec_cmd(scripts .. "screenshot.sh region"))
hl.bind(mainMod .. " + SHIFT + S",   hl.dsp.exec_cmd(scripts .. "screenshot.sh region"))
hl.bind(mainMod .. " + Print",       hl.dsp.exec_cmd(scripts .. "screenshot.sh full"))

-- glass: Hyprland blurs under the translucent surfaces; the shell animates its own layers
hl.layer_rule({
    name = "mist",
    match = { namespace = "^mist-(desktop|bar|dock|start|popup|spotlight|switcher|session)$" },
    ignore_alpha = 0.25,
    blur = true,
    blur_popups = true,
})
hl.layer_rule({ name = "mist-noanim", match = { namespace = "^mist-(desktop|bar|island|spotlight|switcher)$" }, no_anim = true })
hl.layer_rule({ name = "mist-snap", match = { namespace = "^mist-snap(-assist)?$" }, ignore_alpha = 0.1, blur = true, no_anim = true })
hl.layer_rule({ name = "mist-snap-catcher", match = { namespace = "^mist-snap-catcher$" }, no_anim = true })
hl.layer_rule({ name = "mist-gallery", match = { namespace = "^mist-gallery$" }, ignore_alpha = 0.45, blur = true, no_anim = true })
hl.layer_rule({ name = "mist-wallpreview", match = { namespace = "^mist-wallpreview$" }, no_anim = true })
