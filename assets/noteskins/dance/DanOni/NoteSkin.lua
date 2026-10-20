-- DanOni CW Edition's default 5/7-key appearance, adapted to StepMania actors.
-- Source references and the proof's limits: tools/noteskin-danoni/README.md.
local ret = ... or {}

-- DanOni's SVG arrow points LEFT. Its CSS rotations use screen coordinates.
ret.Rotate = { Left = 0, Down = -90, Up = 90, Right = 180,
    UpLeft = 45, UpRight = 135, DownLeft = -45, DownRight = -135,
    Onigiri = 0, Center = 0 }
ret.PartsToRotate = {}
for _, part in ipairs({ "Receptor", "Explosion", "Tap Note", "Tap Lift", "Tap Fake",
    "Hold Head Active", "Hold Head Inactive", "Hold Tail Active", "Hold Tail Inactive",
    "Roll Head Active", "Roll Head Inactive", "Roll Tail Active", "Roll Tail Inactive",
    "Hold Head Dropped", "Hold Tail Dropped", "Roll Head Dropped", "Roll Tail Dropped" }) do
    ret.PartsToRotate[part] = true
end
ret.Blank = {}
for _, part in ipairs({ "Tap Explosion Bright", "Tap Explosion Dim", "Hold Explosion", "Roll Explosion",
    "Hold Topcap Active", "Hold Topcap Inactive", "Hold Bottomcap Active", "Hold Bottomcap Inactive",
    "Roll Topcap Active", "Roll Topcap Inactive", "Roll Bottomcap Active", "Roll Bottomcap Inactive",
    "Hold Topcap Dropped", "Hold Bottomcap Dropped", "Roll Topcap Dropped", "Roll Bottomcap Dropped" }) do
    ret.Blank[part] = true
end
ret.Redir = function(button, element)
    if element == "Receptor" then return "_danoni", "receptor" end
    if element == "Explosion" then return "_danoni", "explosion" end
    if element == "Tap Mine" then return "_danoni", "mine" end
    if string.find(element, "Body") then return "_danoni", "body" end
    if string.find(element, "Head") or string.find(element, "Tail") then
        return "_danoni", "freeze"
    end
    return "_danoni", "tap"
end
ret.Load = ret.CommonLoad
return ret
