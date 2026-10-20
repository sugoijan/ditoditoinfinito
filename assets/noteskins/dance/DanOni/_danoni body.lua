-- 40/50-wide freeze bar, .75 alpha; #6600ff before, #999933 while held.
local active = string.find(Var "Element", "Active") ~= nil
local dropped = string.find(Var "Element", "Dropped") ~= nil
return Def.Sprite {
    -- The host draws body sprites as strips, with field-owned vertex colors.
    -- Select colored/alpha SVGs rather than a diffuse the host replaces.
    Texture = NOTESKIN:GetPath("_bar", dropped and "dropped" or (active and "active" or "inactive")),
}
