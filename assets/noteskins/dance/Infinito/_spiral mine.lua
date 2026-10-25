-- Only outside half-arcs stay visible on connected mine rows.
local button = Var "Button"
if button == "Onigiri" and NOTESKIN:GetParameter("Onigiri") == false then button = "Center" end
local shape = string.lower(button)
local arrows = NOTESKIN:GetParameter("ShockArrows") == true
local shadow = arrows and "_spiral roll " .. shape or "_spiral mine shared"
local base = arrows and "_spiral mine " .. shape or "_spiral mine shared"
local electricity = arrows and "_spiral mine" or "_spiral mine shared"
local function spin(self)
    self:effectclock("music"):spin():effectmagnitude(0,0,-22.5)
end
local function stationary(self) self:effectclock("music") end
local still = not arrows and stationary or nil
local core = arrows and Def.Sprite {Name="MineCore",Texture=NOTESKIN:GetPath(base,"base")}
    or Def.ActorFrame {Name="MineCore",InitCommand=spin,
        Def.Sprite {Name="MineBand",Texture=NOTESKIN:GetPath(base,"base")},
        Def.Sprite {Name="MineCharge",Texture=NOTESKIN:GetPath(base,"charge"),
            Frames={{Frame=0,Delay=0.75},{Frame=1,Delay=0.25}},
            InitCommand=function(self) self:effectclock("beat") end},
    }
local rimFrames = arrows and 16 or 64
local rimPeriod = arrows and 0.5 or 2
return Def.ActorFrame {
    Def.Sprite {Name="MineShadow", Texture=NOTESKIN:GetPath(shadow,arrows and "head shadow" or "shadow"),InitCommand=not arrows and spin or nil},
    core,
    Def.Sprite {Name="MineRimLeft", Texture=NOTESKIN:GetPath(electricity,"rim left"), Frames=Sprite.LinearFrames(rimFrames,rimPeriod),InitCommand=still},
    Def.Sprite {Name="MineRimRight", Texture=NOTESKIN:GetPath(electricity,"rim right"), Frames=Sprite.LinearFrames(rimFrames,rimPeriod),InitCommand=still},
    Def.Sprite {Name="MineLink", Texture=NOTESKIN:GetPath(electricity,"link"), Frames=Sprite.LinearFrames(16,0.5),InitCommand=still},
}
