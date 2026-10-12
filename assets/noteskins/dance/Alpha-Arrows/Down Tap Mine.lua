-- A mine, turning a quarter turn every two beats.
return Def.Sprite {
	Texture = NOTESKIN:GetPath("_down", "tap mine"),
	Frame0000 = 0,
	Delay0000 = 1,
	InitCommand = function(self)
		self:effectclock("beat"):spin():effectmagnitude(0, 0, 45)
	end,
}
