local function burst(self)
    self:finishtweening():diffusealpha(0.9):zoom(1):decelerate(0.22):zoom(1.5):diffusealpha(0)
end
return Def.Sprite {
    Texture = NOTESKIN:GetPath("_studio onigiri", "receptor"),
    InitCommand = function(self) self:diffusealpha(0) end,
    ColumnJudgmentMessageCommand = function(self, params)
        if params.TapNoteScore ~= "TapNoteScore_Miss" then burst(self) end
    end,
    HitMineCommand = burst,
}
