-- A tap, lift or fake (and a hold's or roll's head): the frames of the
-- colour scheme's sheet for it (one for Quantized and Single, coloured by
-- the row the engine picks; the four-beat cycle for Vivid and Flat; the
-- beat's flow for Rainbow). Lifts and fakes have sheets of their own (a
-- dot, 60 %): the engine sets a note's colour and frame on its sprite.
local sheets = { ["Tap Lift"] = "tap lift", ["Tap Fake"] = "tap fake" }
local count = NOTESKIN:GetMetricI("NoteDisplay", "DitoTapFrames")
local frames = {}
for i = 0, count - 1 do
	frames[#frames + 1] = { Frame = i, Delay = 1 / count }
end
return Def.Sprite {
	Texture = NOTESKIN:GetPath("_down", sheets[Var "Element"] or "tap note"),
	Frames = frames,
}
