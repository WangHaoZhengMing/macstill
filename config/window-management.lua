-- macOS-inspired floating desktop for Hyprland 0.55/0.56.
-- Return an installer so the real session and nested preview share the same profile.
return function(macstill)
    local quoted = "'" .. macstill:gsub("'", "'\\''") .. "'"
    hl.config({
        general = {
            gaps_in = 0, gaps_out = 0, border_size = 1,
            resize_on_border = true,
            col = { active_border = "rgba(85858b80)", inactive_border = "rgba(55555b40)" },
            snap = { enabled = true, window_gap = 12, monitor_gap = 12 },
        },
        input = { follow_mouse = 0, float_switch_override_focus = 0 },
        -- Swipes only navigate existing desktops; create them explicitly in Mission Control.
        gestures = { workspace_swipe_create_new = false },
        decoration = {
            rounding = 12, active_opacity = 1.0, inactive_opacity = 1.0,
            -- Hyprland's downsampled blur is not Apple's blurRadius scale.
            -- Rims/tint are rendered by GPUI; this filters the real backdrop.
            blur = {
                enabled = true, size = 8, passes = 3,
                new_optimizations = true, xray = false,
                contrast = 0.95, brightness = 1.0,
                vibrancy = 0.18, vibrancy_darkness = 0.0, noise = 0.012,
            },
            shadow = { enabled = true, range = 24, render_power = 3, color = "rgba(00000038)", color_inactive = "rgba(00000020)", offset = "0 6" },
        },
        misc = { focus_on_activate = true, animate_manual_resizes = false, disable_hyprland_logo = true, disable_splash_rendering = true },
        cursor = { no_warps = true },
        animations = { enabled = true },
    })
    hl.window_rule({ name = "macstill-floating-desktop", match = { class = ".*" }, float = true })
    -- Settings paints regional alpha: translucent sidebar, opaque content.
    -- Keep text opaque and avoid a second compositor border around its rim.
    hl.window_rule({
        name = "macstill-settings-material",
        match = { class = "^org[.]macstill[.]Settings$" },
        rounding = 12, border_size = 0,
        no_shadow = false, no_blur = false, xray = false,
        opacity = "1.0 override 1.0 override",
    })
    hl.window_rule({
        name = "macstill-finder-material",
        match = { class = "^org[.]macstill[.]Finder$" },
        rounding = 12, border_size = 0,
        no_shadow = false, no_blur = false, xray = false,
        opacity = "1.0 override 1.0 override",
    })
    hl.curve("macstillEase", { type = "bezier", points = { {0.2, 0.8}, {0.2, 1.0} } })
    for _, animation in ipairs({
        { "windowsIn", 2.2, "popin 96%" }, { "windowsOut", 1.6, "popin 96%" },
        { "windowsMove", 2.5 }, { "fade", 1.8 }, { "workspaces", 3.0, "slide" },
    }) do
        hl.animation({ leaf = animation[1], enabled = true, speed = animation[2], bezier = "macstillEase", style = animation[3] })
    end
    -- Shell surfaces own their transitions; compositor layer animations would
    -- add a second slide/pop animation to menus and Launchpad.
    for _, namespace in ipairs({"macstill-bar", "macstill-desktop", "macstill-launchpad", "macstill-control-center", "macstill-notification-center", "macstill-notification-banner", "macstill-apple-menu", "macstill-tray"}) do
        hl.layer_rule({ match = { namespace = namespace }, no_anim = true })
    end
    -- Alpha mask limits backdrop filtering to painted material, leaving sparse
    -- layer-shell input/headroom and the Stage preview overlay transparent.
    for _, namespace in ipairs({"macstill-bar", "macstill-dock", "macstill-launchpad", "macstill-control-center", "macstill-notification-center", "macstill-notification-banner", "macstill-apple-menu", "macstill-tray", "macstill-stage-manager"}) do
        hl.layer_rule({ match = { namespace = namespace }, blur = true, ignore_alpha = 0.05, no_anim = true })
    end
    for _, binding in ipairs({
        {"SUPER + SHIFT + 3", "--fullscreen"}, {"SUPER + SHIFT + 4", "--region"},
        {"SUPER + SHIFT + 5", ""}, {"SUPER + CTRL + SHIFT + 3", "--fullscreen --clipboard"},
        {"SUPER + CTRL + SHIFT + 4", "--region --clipboard"},
    }) do
        hl.bind(binding[1], hl.dsp.exec_cmd(quoted .. " screenshot " .. binding[2]))
    end
    hl.layer_rule({ match = { namespace = "macstill-screenshot" }, no_anim = true })
    hl.layer_rule({ match = { namespace = "macstill-screenshot-preview" }, no_anim = true })
    -- Mission Control: native horizontal workspace swipes, interactive vertical overview.
    hl.bind("CTRL + UP", hl.dsp.exec_cmd(quoted .. " mission-control toggle"))
    hl.bind("SUPER + F3", hl.dsp.exec_cmd(quoted .. " mission-control toggle"))
    hl.bind("XF86LaunchA", hl.dsp.exec_cmd(quoted .. " mission-control toggle"))
    hl.bind("CTRL + LEFT", hl.dsp.exec_cmd(quoted .. " mission-control previous"))
    hl.bind("CTRL + RIGHT", hl.dsp.exec_cmd(quoted .. " mission-control next"))
    hl.gesture({fingers=3, direction="horizontal", action="workspace"})
    hl.layer_rule({match={namespace="macstill-mission-control"},no_anim=true})
    local runtime=os.getenv("XDG_RUNTIME_DIR")
    local instance=os.getenv("HYPRLAND_INSTANCE_SIGNATURE")
    if runtime and instance then
        local path=runtime .. "/hypr/" .. instance .. "/macstill-mission-gesture.json"
        local sequence=0
        local function overview_gesture(direction)
            local distance=0
            local function publish(active,cancelled)
                local file=io.open(path .. ".tmp","w")
                if file then
                    file:write(string.format('{"seq":%d,"direction":"%s","progress":%.4f,"active":%s,"cancelled":%s}', sequence,direction,math.max(0,math.min(1,distance/260)),tostring(active),tostring(cancelled)))
                    file:close(); os.rename(path .. ".tmp",path)
                end
            end
            return {
                start=function(event)
                    sequence=math.max(sequence+1,event.time_ms or 1); distance=0
                    publish(true,false)
                    hl.exec_cmd(quoted .. " mission-control gesture")
                end,
                update=function(event)
                    distance=distance+(event.delta and event.delta.y or 0)*(direction=="up" and -1 or 1)
                    publish(true,false)
                end,
                finish=function(event) publish(false,event.cancelled==true) end,
            }
        end
        hl.gesture({fingers=3,direction="up",action=overview_gesture("up")})
        hl.gesture({fingers=3,direction="down",action=overview_gesture("down")})
    end
    local spaces_home=os.getenv("XDG_CONFIG_HOME") or (os.getenv("HOME") .. "/.config")
    local spaces_path=spaces_home .. "/macstill/mission-spaces.lua"
    local spaces_file=io.open(spaces_path,"r")
    if spaces_file then spaces_file:close();dofile(spaces_path) end
    local function action(keys, command)
        hl.bind(keys, hl.dsp.exec_cmd(quoted .. " window " .. command))
    end
    action("SUPER + TAB", "next")
    action("SUPER + SHIFT + TAB", "previous")
    action("SUPER + grave", "same-app")
    action("SUPER + ALT + LEFT", "left")
    action("SUPER + ALT + RIGHT", "right")
    action("SUPER + ALT + C", "center")
    action("SUPER + ALT + UP", "maximize")
    action("SUPER + CTRL + F", "fullscreen")
    action("SUPER + W", "close")
    action("SUPER + ALT + T", "float")
    for _, key in ipairs({{"XF86AudioPlay", "play-pause"}, {"XF86AudioPause", "pause"}, {"XF86AudioStop", "stop"}, {"XF86AudioNext", "next"}, {"XF86AudioPrev", "previous"}}) do
        hl.bind(key[1], hl.dsp.exec_cmd(quoted .. " media " .. key[2]))
    end
    hl.bind("SUPER + mouse:272", hl.dsp.window.drag())
    hl.bind("SUPER + mouse:273", hl.dsp.window.resize())
    local config_home = os.getenv("XDG_CONFIG_HOME") or (os.getenv("HOME") .. "/.config")
    local scales = config_home .. "/macstill/display-scales.lua"
    local file = io.open(scales, "r")
    if file then file:close(); dofile(scales) end
    local pointers = config_home .. "/macstill/pointer-settings.lua"
    local pointer_file = io.open(pointers, "r")
    if pointer_file then pointer_file:close(); dofile(pointers) end

end
