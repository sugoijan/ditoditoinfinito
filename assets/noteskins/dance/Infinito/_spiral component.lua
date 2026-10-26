-- SVG tap/receptor factory. Loaded only at skin setup.
-- Paint phase is independent of the charge sequence.
local colors = NOTESKIN:GetParameter("Colors")
local rainbow = colors == "Rainbow"
local function color_phase()
    if rainbow then
        -- 64 steps distinguish the downbeat from every offbeat in the
        -- 48-tick chart grid, with exact power-of-two phase arithmetic.
        -- The material removes the song beat to recover the note's color family.
        return Bind.Linear {clock="note",length=64,from=0,to=64}
    end
    return Bind.Linear {clock=colors == "Flat" and "beat" or "note",
        length=4,from=0.626,to=-0.374}
end

-- Frame zero is full for taps/long heads, empty for reversed lifts.
local charge_clock = NOTESKIN:GetParameter("NoteTimedCharge") and "note-relative" or "note"
local turns = {Up=0,Right=90,Down=180,Left=270,UpRight=45,DownRight=135,DownLeft=225,UpLeft=315}
local button = Var "Button"
local element = string.lower(Var "Element")
    if button == "Onigiri" and NOTESKIN:GetParameter("Onigiri") == false then button = "Center" end
    local shape = button == "Center" and "center" or button == "Onigiri" and "onigiri"
        or ((button == "Up" or button == "Down") and "vertical" or "arrow")
    local receptor = element == "receptor"
    local id = (receptor and "receptor-" or "tap-") .. shape
    local offset = shape == "vertical" and 1 or shape == "onigiri" and -7 or 0
    local parts = {}
    if receptor then
        -- Keep the 20% bright / 80% dim beat blink and the old press's
        -- one-frame dwell followed by a four-frame linear recovery.
        parts[id .. "-band"] = {Frame = Bind.Curve {
            clock="beat", length=1, keys={0,0, 0.2,1, 1,1}}}
        parts[id .. "-shadow"] = {Opacity=0.5}
        parts[id .. "-fill"] = {Visible=NOTESKIN:GetParameter("ReceptorFill") ~= false}
    else
        parts[id .. "-band"] = {
            Material = Def.Material {
                File="_spiral material.wgsl", Space="note", Origin={0,offset},
                Params={{120.6,1,97,0.43}, {0.5,1.089,0.0,
                    shape == "onigiri" and 20.691 or 8.712},
                    {0,rainbow and 1 or 0},{0.021,1.35,0.3,2}},
                Images={"_spiral components.svg#palette"},
            },
            Phase=color_phase(),
        }
        parts[id .. "-charge"] = {Frame=Bind.Steps {clock=charge_clock, length=1, count=4}}
    end
    return Def.Component {
        File="_spiral components.svg", Id=id, Rotation=turns[button] or 0, Parts=parts,
        -- Zoom the complete receptor about its aligned center. The lane's
        -- last press supplies a hit clock for judgments and empty presses.
        Pivot={0,offset},
        Zoom=receptor and Bind.Curve {clock="hit", keys={0,0.9, 1/60,0.9, 5/60,1}} or 1,
    }
