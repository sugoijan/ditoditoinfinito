-- Same regions as the note, with the original receptor's beat and press timing.
local function press(self)
    self:finishtweening():zoom(0.9):sleep(1/60):linear(4/60):zoom(1)
end
return Def.Sprite {
    Texture = NOTESKIN:GetPath("_center", "raster receptor"),
    InitCommand = function(self)
        self:effectclock("beat")
        self:SetStateProperties({{Frame=0, Delay=0.2}, {Frame=1, Delay=0.8}})
        self:SetTextureFiltering(false)
    end,
    NoneCommand=press, PressCommand=press,
    W1Command=press, W2Command=press, W3Command=press,
    W4Command=press, W5Command=press,
}
