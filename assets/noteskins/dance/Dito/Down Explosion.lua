-- Dito's explosions (the built-in arrows have none). A hit bursts from the
-- receptor in its judgement's colour, as a glow and the arrow's outline
-- (the onigiri's star's) growing and fading, larger and brighter past 100
-- combo; a completed hold bursts green; a held hold or roll glows green,
-- pulsing on the beat (a roll faster); a mine flashes red.
local colors =
{
	TapNoteScore_W1 = color("0.55,0.95,1,1"),
	TapNoteScore_W2 = color("1,0.9,0.35,1"),
	TapNoteScore_W3 = color("0.45,0.95,0.45,1"),
	TapNoteScore_W4 = color("0.75,0.5,1,1"),
	TapNoteScore_W5 = color("1,0.55,0.3,1"),
	HoldNoteScore_Held = color("0.55,0.95,0.65,1"),
}
local held = color("0.55,0.95,0.65,1")
local mine = color("1,0.35,0.3,1")
local glow = NOTESKIN:GetPath("_down", "glow")
local outline = NOTESKIN:GetPath(Var "Button" == "Onigiri" and "_onigiri" or "_down", "receptor")

local function burst(self, c, bright)
	self:GetChild("Glow"):finishtweening()
		:diffuse(c):diffusealpha(bright and 1 or 0.7):zoom(0.8)
		:decelerate(0.25):zoom(bright and 1.6 or 1.3):diffusealpha(0)
	self:GetChild("Ring"):finishtweening()
		:diffuse(c):diffusealpha(bright and 1 or 0.8):zoom(1)
		:decelerate(0.2):zoom(bright and 1.35 or 1.2):diffusealpha(0)
end

local function holding(name, period)
	return Def.Sprite {
		Name = name,
		Texture = glow,
		InitCommand = function(self)
			self:blend("BlendMode_Add"):diffuse(held):diffusealpha(0)
				:effectclock("beat"):pulse():effectperiod(period)
				:effectmagnitude(1.15, 0.95, 1)
		end,
	}
end

local function on(name)
	return function(self)
		self:GetChild(name):stoptweening():linear(0.05):diffusealpha(0.6)
	end
end

local function off(name)
	return function(self)
		self:GetChild(name):stoptweening():linear(0.1):diffusealpha(0)
	end
end

return Def.ActorFrame {
	ColumnJudgmentMessageCommand = function(self, params)
		local c = colors[params.TapNoteScore or params.HoldNoteScore or ""]
		if c then
			burst(self, c, params.Bright)
		end
	end,
	HitMineCommand = function(self)
		burst(self, mine, true)
	end,
	HoldingOnCommand = on("Hold"),
	HoldingOffCommand = off("Hold"),
	RollOnCommand = on("Roll"),
	RollOffCommand = off("Roll"),
	holding("Hold", 1),
	holding("Roll", 0.5),
	Def.Sprite {
		Name = "Glow",
		Texture = glow,
		InitCommand = function(self)
			self:blend("BlendMode_Add"):diffusealpha(0)
		end,
	},
	Def.Sprite {
		Name = "Ring",
		Texture = outline,
		InitCommand = function(self)
			self:blend("BlendMode_Add"):diffusealpha(0)
		end,
	},
}
