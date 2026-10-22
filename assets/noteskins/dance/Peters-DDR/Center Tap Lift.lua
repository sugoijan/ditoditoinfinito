-- Selected raster draft: full -> reset -> inner -> inner+middle on each beat.
return Def.Sprite {
    Texture = NOTESKIN:GetPath("_center", "raster tap"),
    InitCommand = function(self)
        self:SetStateProperties({
            {Frame=0, Delay=0.25}, {Frame=16, Delay=0.25},
            {Frame=32, Delay=0.25}, {Frame=48, Delay=0.25},
        })
        self:SetTextureFiltering(false)
    end,
}
