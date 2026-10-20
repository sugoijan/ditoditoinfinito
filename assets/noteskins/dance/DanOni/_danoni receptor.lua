-- makeStepZone / mainKeyUpActFunc: a fixed gray outline plus cyan key-down
-- overlay. No pulsing, shrinking, tweened release, or judgment recoloring.
local symbol = (Var "Button" == "Onigiri" or Var "Button" == "Center") and "_onigiri" or "_arrow"
local texture = NOTESKIN:GetPath(symbol, "shape")
return Def.ActorFrame {
    PressCommand = function(self, params) self:GetChild("Pressed"):visible(not params.Autoplay) end,
    LiftCommand = function(self) self:GetChild("Pressed"):visible(false) end,
    Def.Sprite {
        Texture = texture,
        InitCommand = function(self) self:diffuse(color("#999999")) end,
    },
    Def.Sprite {
        Name = "Pressed", Texture = texture,
        InitCommand = function(self) self:diffuse(color("#66ffff")):visible(false) end,
    },
}
