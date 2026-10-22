-- The explosion (Dito's, on this skin's art): the arrow's glow, turned to
-- the button and colored by the judgement, swelling as it fades (more
-- past 100 combo); for the best three judgements, rays flung out upright
-- around it; a held hold's or roll's glow breathing with the beat; a
-- mine's flash in red.
local turns =
{
	Up = 180,
	Down = 0,
	Left = 90,
	Right = -90,
	UpLeft = 135,
	UpRight = 225,
	Center = 0,
}
local turn = turns[Var "Button"] or 0
local colors =
{
	TapNoteScore_W1 = color("0.55,0.95,1,1"),
	TapNoteScore_W2 = color("1,0.9,0.35,1"),
	TapNoteScore_W3 = color("0.45,0.95,0.45,1"),
	TapNoteScore_W4 = color("0.75,0.5,1,1"),
	TapNoteScore_W5 = color("1,0.55,0.3,1"),
	HoldNoteScore_Held = color("1,1,1,1"),
}
local rays =
{
	TapNoteScore_W1 = true,
	TapNoteScore_W2 = true,
	TapNoteScore_W3 = true,
	HoldNoteScore_Held = true,
}
-- A pump pad's center glows in its octagon.
local glow = NOTESKIN:GetPath(Var "Button" == "Center" and "Center" or "Down", "Tap Explosion Dim")

local function burst(self, c, bright, ray)
	self:GetChild("Glow"):finishtweening()
		:diffuse(c):diffusealpha(bright and 1 or 0.75):zoom(0.85)
		:decelerate(0.3):zoom(bright and 1.35 or 1.15):diffusealpha(0)
	if ray then
		self:GetChild("Rays"):finishtweening()
			:diffuse(c):diffusealpha(0.9):zoom(0.6):rotationz(0)
			:decelerate(0.25):zoom(bright and 1.25 or 1):rotationz(15):diffusealpha(0)
	end
end

local function sustain(name, period)
	return Def.Sprite {
		Name = name,
		Texture = glow,
		InitCommand = function(self)
			self:rotationz(turn):blend("BlendMode_Add"):diffusealpha(0)
				:effectclock("beat"):pulse():effectperiod(period)
				:effectmagnitude(0.95, 1.05, 1)
		end,
	}
end

local function on(name)
	return function(self)
		self:GetChild(name):stoptweening():diffusealpha(0.7)
	end
end

local function off(name)
	return function(self)
		self:GetChild(name):stoptweening():decelerate(0.15):diffusealpha(0)
	end
end

return Def.ActorFrame {
	ColumnJudgmentMessageCommand = function(self, params)
		local score = params.TapNoteScore or params.HoldNoteScore or ""
		if colors[score] then
			burst(self, colors[score], params.Bright, rays[score])
		end
	end,
	HitMineCommand = function(self)
		burst(self, color("1,0.3,0.25,1"), true, true)
	end,
	HoldingOnCommand = on("Hold"),
	HoldingOffCommand = off("Hold"),
	RollOnCommand = on("Roll"),
	RollOffCommand = off("Roll"),
	sustain("Hold", 1),
	sustain("Roll", 0.5),
	Def.Sprite {
		Name = "Glow",
		Texture = glow,
		InitCommand = function(self)
			self:rotationz(turn):blend("BlendMode_Add"):diffusealpha(0)
		end,
	},
	Def.Sprite {
		Name = "Rays",
		Texture = NOTESKIN:GetPath("Down", "Tap Explosion Rays"),
		InitCommand = function(self)
			self:blend("BlendMode_Add"):diffusealpha(0)
		end,
	},
}
