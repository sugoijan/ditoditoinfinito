-- The receptor: the arrow's outline, smaller while its button is down,
-- flashing a judgement's colour for a quarter of a second (not a miss's).
local base = color("0.75,0.75,0.8,0.9")
local flashes =
{
	W1 = color("0.55,0.95,1,1"),
	W2 = color("1,0.9,0.35,1"),
	W3 = color("0.45,0.95,0.45,1"),
	W4 = color("0.75,0.5,1,1"),
	W5 = color("1,0.55,0.3,1"),
}
local outline = NOTESKIN:GetPath("_down", "receptor")

local t = Def.ActorFrame {
	PressCommand = function(self)
		self:zoom(0.88)
	end,
	LiftCommand = function(self)
		self:zoom(1)
	end,
	Def.Sprite {
		Texture = outline,
		InitCommand = function(self)
			self:diffuse(base)
		end,
	},
	Def.Sprite {
		Name = "Flash",
		Texture = outline,
		InitCommand = function(self)
			self:diffusealpha(0)
		end,
	},
}
for judgement, c in pairs(flashes) do
	t[judgement .. "Command"] = function(self)
		self:GetChild("Flash"):finishtweening():diffuse(c):linear(0.25):diffusealpha(0)
	end
end
return t
