-- Standalone desktop: Hyprland is the only compositor in this user session.
local shell = "/opt/macstill/bin/macstill"
hl.monitor({ output = "", mode = "preferred", position = "auto", scale = 1 })
dofile(os.getenv("HOME") .. "/.config/macstill/hyprland/window-management.lua")(shell)
hl.on("hyprland.start", function() hl.exec_cmd("/opt/macstill/bin/macstill-session-ready") end)
hl.bind("SUPER + SPACE", hl.dsp.exec_cmd(shell .. " launchpad"))
hl.bind("SUPER + comma", hl.dsp.exec_cmd(shell .. " settings"))
hl.bind("SUPER + SHIFT + Escape", hl.dsp.exit())
hl.layer_rule({ match = { namespace = "macstill-dock" }, blur = true, ignore_alpha = 0.05, no_anim = true })
