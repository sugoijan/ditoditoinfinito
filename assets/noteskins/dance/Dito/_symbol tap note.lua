-- A symbol's tap, lift or fake (and a hold's or roll's head): the
-- onigiri's star or the centre panel's square (`Var "Button"`), the
-- frames of the color scheme's sheet for it (one for Quantized and
-- Single, colored by the row the engine picks; the four-beat cycle for
-- Vivid and Flat; the beat's flow for Rainbow). Lifts and fakes have
-- sheets of their own (a dot, 60 %): the engine sets a note's color and
-- frame on its sprite.
local prefix = Var "Button" == "Center" and "_center" or "_onigiri"
local sheets = { ["Tap Lift"] = "tap lift", ["Tap Fake"] = "tap fake" }
local count = NOTESKIN:GetMetricI("NoteDisplay", "DitoTapFrames")
local frames = {}
for i = 0, count - 1 do
	frames[#frames + 1] = { Frame = i, Delay = 1 / count }
end
return Def.Sprite {
	Texture = NOTESKIN:GetPath(prefix, sheets[Var "Element"] or "tap note"),
	Frames = frames,
}
