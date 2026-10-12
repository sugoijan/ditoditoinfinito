-- A mine, its spikes turning with the song as the built-in one's do
-- (six radians a second over eight spikes: 43 degrees a second,
-- anticlockwise).
return Def.ActorFrame {
	InitCommand = function(self)
		self:SetUpdateFunction(function(frame)
			local seconds = GAMESTATE:GetSongPosition():GetMusicSeconds()
			frame:GetChild("Mine"):rotationz((-seconds * 43) % 360)
		end)
	end,
	Def.Sprite {
		Name = "Mine",
		Texture = NOTESKIN:GetPath("_down", "mine"),
		Frame0000 = 0,
		Delay0000 = 1,
	},
}
