-- The body owns the whole long note; the loader never asks for legacy pieces.
-- Frame zero is full for taps/long heads, empty for reversed lifts.
local charge_clock = NOTESKIN:GetParameter("NoteTimedCharge") and "note-relative" or "note"
local button = Var "Button"
if button == "Onigiri" and NOTESKIN:GetParameter("Onigiri") == false then button = "Center" end
local element = string.lower(Var "Element")
local roll = string.find(element, "^roll ") ~= nil
local kind = roll and "roll" or "hold"
local parts = { ["head-charge"] = {Frame=Bind.Steps {clock=charge_clock, length=1, count=4}} }
if roll then
    -- The accepted roll's exact 0.65-power pulse.
    -- Its cycle is 75% of the active scaled ruleset window, independent of BPM.
    for _, name in ipairs({"body-pulse", "cap-pulse"}) do
        parts[name] = {Opacity=Bind.Curve {
            clock="roll", length=0.75, ease="out", power=0.65,
            keys={0,0.85, 0.75*0.25,0.08, 0.75,0.08}}}
    end
end
return Def.LongNote {
    File="_spiral " .. kind .. " " .. string.lower(button) .. " components.svg",
    Head="head", Body="body", Cap="cap", Pitch=roll and 25.0 or 12.5,
    Join=({UpLeft=-12.625,Left=-0.625,DownLeft=-11.75,Down=-1.625,Center=-11.75,
        Onigiri=8.75,Up=0.375,UpRight=-12.75,Right=-0.625,DownRight=-11.625})[button] or 0,
    Anchor="head",
    Material=Def.Material {
        File="_spiral wave.wgsl", Space="lane",
        Params=roll and {{0.1908089953765448,0.981627183447664,171,31.5},{209,206},"#3b18ee","#7132ff","#bf35fb",{0},{0.08,0.22}} or {{0.8660254037844386,0.5000000000000001,121,31},{209,206},"#2bab4b","#55d232","#cee774",{0},{0.08,0.22}},
    },
    Phase=Bind.Linear {clock="seconds",length=1.0,from=0.7,to=1.7}, Parts=parts,
}
