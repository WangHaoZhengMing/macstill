-- Add to your existing Hyprland Lua config (0.56+).
-- Keep your own wallpaper, portals, input method and lock-screen setup.
local macstill = os.getenv("HOME") .. "/.local/bin/macstill"
local config_home = os.getenv("XDG_CONFIG_HOME") or (os.getenv("HOME") .. "/.config")
dofile(config_home .. "/macstill/hyprland/window-management.lua")(macstill)
hl.on("hyprland.start", function()
    hl.exec_cmd(macstill .. " shell")
end)
hl.bind("SUPER + SPACE", hl.dsp.exec_cmd(macstill .. " launchpad"))
hl.bind("SUPER + comma", hl.dsp.exec_cmd(macstill .. " settings"))

-- Frosted material, limited to painted pixels so transparent headroom stays clear.
-- https://wiki.hypr.land/configuring/core/rules/layer-rules/
hl.layer_rule({
    match = { namespace = "macstill-dock" },
    blur = true,
    ignore_alpha = 0.05,
    no_anim = true, -- GPUI owns hover/bounce/hide animation.
})
