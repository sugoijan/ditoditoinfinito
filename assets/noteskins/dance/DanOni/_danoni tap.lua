-- g_dfColorObj.setColorInit and color5_0_0 / color7_0_0. No beat animation.
local button = Var "Button"
local onigiri = button == "Onigiri" or button == "Center"
local diagonal = button == "DownLeft" or button == "DownRight" or button == "UpLeft" or button == "UpRight"
local tint = onigiri and "#ffffff" or (diagonal and "#99ffff" or "#6666ff")
-- DrawActor owns the root's diffuse. Keep the source color on a child.
return Def.ActorFrame { Def.Sprite {
    Texture = NOTESKIN:GetPath(onigiri and "_onigiri" or "_arrow", "shape"),
    InitCommand = function(self) self:diffuse(color(tint)):animate(false) end,
} }
