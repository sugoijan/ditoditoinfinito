-- A lift: a diamond that swells a little on each beat.
return Def.Sprite {
	Texture = NOTESKIN:GetPath("_down", "tap lift"),
	Frame0000 = 0,
	Delay0000 = 1,
	InitCommand = function(self)
		self:effectclock("beat"):thump():effectmagnitude(1, 1.1, 1)
	end,
}
