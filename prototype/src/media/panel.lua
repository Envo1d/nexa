local mp = require 'mp'
local msg = require 'mp.msg'
local assdraw = require 'mp.assdraw'
local overlay = mp.create_osd_overlay('ass-events')
local visible, dragging = true, nil
local pending_seek = nil
local last_seek = -1
local function flush_seek(final)
    if pending_seek == nil then return end
    if not final and mp.get_time() - last_seek < 0.1 then return end
    local target = pending_seek
    pending_seek = nil
    last_seek = mp.get_time()
    mp.commandv('seek', target, final and 'absolute+exact' or 'absolute+keyframes')
end
local function number(name, fallback)
    return mp.get_property_number(name, fallback or 0)
end
local function geometry()
    local w, h = mp.get_osd_size()
    if w <= 0 or h <= 0 then return nil end
    return w, h, h * 1000 / w
end
local function clock(value)
    value = math.max(0, math.floor(value))
    return string.format('%02d:%02d', math.floor(value / 60), value % 60)
end
local function draw()
    if not visible then overlay:remove(); return end
    local w, h, height = geometry()
    if not w then return end
    local a = assdraw.ass_new()
    local function box(x, y, width, high, color, alpha)
        a:new_event()
        a:append(string.format([[{\an7\pos(0,0)\bord0\shad0\1c&H%s&\1a&H%s&}]], color, alpha or '00'))
        a:draw_start(); a:rect_cw(x, y, x + width, y + high); a:draw_stop()
    end
    local function text(x, y, label, size, color)
        a:new_event()
        a:append(string.format([[{\an7\pos(%g,%g)\fnSegoe UI\fs%d\bord0\shad0\1c&H%s&}%s]], x, y, size or 18, color or 'F6EDEE', label))
    end
    local y = height - 100
    box(12, y, 976, 88, '201414', '18')
    box(12, y, 976, 1, '443330')
    local duration, time = number('duration'), number('time-pos')
    local fraction = duration > 0 and math.min(1, time / duration) or 0
    box(28, y + 12, 944, 4, '544B60')
    box(28, y + 12, 944 * fraction, 4, 'FF86A7')
    box(25 + 944 * fraction, y + 9, 7, 10, 'FF86A7')
    text(30, y + 40, mp.get_property_native('pause') and 'PLAY' or 'PAUSE', 17, 'FFB9CF')
    text(112, y + 40, '-10s', 17)
    text(177, y + 40, '+10s', 17)
    text(250, y + 40, clock(time) .. ' / ' .. clock(duration), 18)
    local muted = mp.get_property_native('mute')
    text(650, y + 40, muted and 'MUTED' or 'VOL', 16, 'C3B6CD')
    local volume = math.max(0, math.min(100, number('volume')))
    box(720, y + 49, 135, 4, '544B60')
    box(720, y + 49, muted and 0 or 135 * volume / 100, 4, 'FF86A7')
    text(871, y + 40, string.format('%d', math.floor(volume)), 16)
    text(926, y + 40, 'FULL', 16, 'FFB9CF')
    overlay.res_x, overlay.res_y = 1000, math.floor(height + 0.5)
    overlay.data = a.text
    overlay:update()
end
local function point()
    local w, h, height = geometry()
    if not w then return nil end
    local x, y = mp.get_mouse_pos()
    return x * 1000 / w, y * 1000 / w, height
end
local function drag(final)
    if not dragging or not visible then return end
    local x = point()
    if not x then return end
    if dragging == 'seek' then
        local duration = number('duration')
        if duration > 0 then
            pending_seek = math.max(0, math.min(1, (x - 28) / 944)) * duration
            if not final then flush_seek(false) end
        end
    else
        mp.set_property_number('volume', math.max(0, math.min(100, (x - 720) * 100 / 135)))
    end
    draw()
end
local function down()
    if not visible then return end
    local x, y, height = point()
    if not x then return end
    local top = height - 100
    if y >= top and y <= top + 28 then dragging = 'seek'; drag(); return end
    if y < top + 29 or y > top + 83 then return end
    if x >= 28 and x < 103 then mp.commandv('cycle', 'pause')
    elseif x >= 103 and x < 169 then mp.commandv('seek', -10, 'relative')
    elseif x >= 169 and x < 235 then mp.commandv('seek', 10, 'relative')
    elseif x >= 640 and x < 715 then mp.commandv('cycle', 'mute')
    elseif x >= 715 and x <= 860 then dragging = 'volume'; drag() end
    draw()
end
mp.add_forced_key_binding('MBTN_LEFT', 'panel-click', function(event)
    if event.event == 'down' then down()
    elseif event.event == 'up' then
        if dragging == 'seek' then drag(true); flush_seek(true) end
        dragging = nil
    end
end, {complex = true})
mp.add_forced_key_binding('MOUSE_MOVE', 'panel-move', drag)
mp.register_script_message('nexa-panel-visibility', function(mode)
    flush_seek(true)
    visible = mode ~= 'never'; dragging = nil; draw()
end)
mp.register_event('shutdown', function() overlay:remove() end)
mp.add_periodic_timer(0.25, draw)
msg.info('Nexa custom playback panel initialized')
draw()
