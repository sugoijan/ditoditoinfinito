-- Original Vivid texture columns; white charge on gray, inner to outer.
return Def.Sprite {
 Texture = NOTESKIN:GetPath("_center", "vivid tap"),
 InitCommand = function(self)
  local states = {}
  for frame = 0, 31 do states[#states+1] = {Frame=frame, Delay=0.125} end
  self:SetStateProperties(states)
  self:SetTextureFiltering(false)
 end,
}
