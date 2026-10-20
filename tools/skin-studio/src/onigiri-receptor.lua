return Def.Sprite {
    Texture = NOTESKIN:GetPath("_studio onigiri", "receptor"),
    PressCommand = function(self) self:zoom(0.9) end,
    LiftCommand = function(self) self:zoom(1) end,
}
