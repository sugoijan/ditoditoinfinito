-- Spiral: original taps, receptors, hold and roll materials.
local ret = ... or {}
local oldRedir, oldLoad = ret.Redir, ret.Load
local buttons = {Down=true,Up=true,Left=true,Right=true,UpLeft=true,UpRight=true,
    DownLeft=true,DownRight=true,Center=true,Onigiri=true}
-- The `_spiral` file drawing an element of ours: a lift on any button,
-- the rest on the ten buttons above.
local function file(button, element)
    local e = string.lower(element)
    if e == "tap lift" then return "lift.lua" end
    if not buttons[button] then return nil end
    if e == "explosion" then return "explosion.lua" end
    if e == "tap note" or e == "tap fake" or e == "receptor" then return "component.lua" end
    if e == "tap mine" then return "mine.lua" end
    if string.find(e, "^hold ") or string.find(e, "^roll ") then return "hold.lua" end
    return nil
end
ret.Redir = function(button, element)
    if file(button, element) then return button, element end
    return oldRedir(button, element)
end
ret.Load = function()
    -- An explosion redirected here is ours whatever its button.
    local f = string.lower(Var "Element") == "explosion" and "explosion.lua"
        or file(Var "Button", Var "Element")
    if f then return LoadActor(NOTESKIN:GetPath("_spiral", f)) end
    return oldLoad()
end
return ret
