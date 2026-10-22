-- The receptor: the outline, a tenth smaller while its button is down;
-- a judgement lights its flash art in the judgement's color for a
-- quarter of a second (not a miss).
local flashes =
{
	W1 = color("0.55,0.95,1,1"),
	W2 = color("1,0.9,0.35,1"),
	W3 = color("0.45,0.95,0.45,1"),
	W4 = color("0.75,0.5,1,1"),
	W5 = color("1,0.55,0.3,1"),
}

-- A pump pad's center draws its octagon instead.
local art = Var "Button" == "Center" and "_center" or "_down"

local t = Def.ActorFrame {
	PressCommand = function(self)
		self:zoom(0.9)
	end,
	LiftCommand = function(self)
		self:zoom(1)
	end,
	Def.Sprite {
		Texture = NOTESKIN:GetPath(art, "receptor"),
		Frame0000 = 0,
		Delay0000 = 1,
	},
	Def.Sprite {
		Name = "Flash",
		Texture = NOTESKIN:GetPath(art, "rflash"),
		Frame0000 = 0,
		Delay0000 = 1,
		InitCommand = function(self)
			self:blend("BlendMode_Add"):diffusealpha(0)
		end,
	},
}
for judgement, c in pairs(flashes) do
	t[judgement .. "Command"] = function(self)
		self:GetChild("Flash"):finishtweening():diffuse(c):linear(0.25):diffusealpha(0)
	end
end
return t
