-- makeFrzSprites / changeHitFrz: black silhouette behind the outlined ends.
-- Held heads lose their outline; the Explosion actor supplies frzHitTop.
local onigiri = Var "Button" == "Onigiri" or Var "Button" == "Center"
local symbol = onigiri and "_onigiri" or "_arrow"
local element = Var "Element"
local active = string.find(element, "Active") ~= nil
local head = string.find(element, "Head") ~= nil
local dropped = string.find(element, "Dropped") ~= nil
return Def.ActorFrame {
    Def.Sprite {
        Texture = NOTESKIN:GetPath(symbol, "shadow"),
        InitCommand = function(self) self:diffuse(color(dropped and "#333333" or "#000000")) end,
    },
    Def.Sprite {
        Texture = NOTESKIN:GetPath(symbol, "shape"),
        InitCommand = function(self)
            self:diffuse(color(dropped and "#cccccc" or (active and "#cccc33" or "#66ffff")))
                :visible(not (active and head))
        end,
    },
}
