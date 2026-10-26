-- Load after your general Hyprland 0.56+ window rules.
-- Blur must be enabled in decoration.blur in your compositor profile.
hl.window_rule({
    name = "finder-material",
    match = { class = "^org[.]macstill[.]Finder$" },
    rounding = 12, border_size = 0,
    no_shadow = false, no_blur = false, xray = false,
    opacity = "1.0 override 1.0 override",
})
