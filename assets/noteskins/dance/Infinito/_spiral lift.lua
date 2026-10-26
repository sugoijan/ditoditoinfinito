-- Oriented masks; rotate only the spiral paint, leaving the trail upright.
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
local button = Var "Button"
if button == "Onigiri" and NOTESKIN:GetParameter("Onigiri") == false then button = "Center" end
local offset = ({Up=1,Down=-1,Onigiri=-7})[button] or 0
local function wave(shift)
    return Def.Material {File="_spiral wave.wgsl",Space="lane",
        Params={{0.8660254037844386,0.5000000000000001,121,31},{209,206},"#2bab4b","#55d232","#cee774",{shift},{0.08,0.22}}}
end
local function seconds() return Bind.Linear {clock="seconds",length=1.0,from=0.7,to=1.7} end
return Def.Lift {
    File="_spiral lift " .. string.lower(button) .. " components.svg", Id="head", Trail="trail",
    Seconds=0.28, Fade=2, Opacity=1,
    Material=wave(0),
    Phase=seconds(),
    Parts={
        ["head-band"]={Material=wave(0.7), Phase=seconds()},
        ["head-charge"]={Frame=Bind.Steps {clock=charge_clock,length=1,count=4},
            Material=Def.Material {File="_spiral material.wgsl",Space="note",Origin={0,offset},
                Params={{120.6,1,97,0.43},{0.5,1.089,0.0,34.848},
                    {({Up=0,Right=90,Down=180,Left=270,UpRight=45,DownRight=135,DownLeft=225,UpLeft=315})[button] or 0, rainbow and 1 or 0},{0.021,1.35,0.3,2}},
                Images={"_spiral components.svg#palette"}},
            Phase=color_phase()},
    },
}
