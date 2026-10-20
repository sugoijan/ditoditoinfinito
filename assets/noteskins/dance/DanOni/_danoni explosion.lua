-- mainWindow.js stepHitTargetArrow: a fixed 80/50 outline, alpha .75,
-- hidden abruptly after C_FRM_HITMOTION=4 frames at the reference 60 Hz.
-- DanOni's four native tiers are W1/W2/W3/W4; ITG's five slots adapt as below.
-- A miss has no flash. Freeze completion hides its overlay without a burst.
local onigiri = Var "Button" == "Onigiri" or Var "Button" == "Center"
local symbol = onigiri and "_onigiri" or "_arrow"
local texture = NOTESKIN:GetPath(symbol, "shape")
local shadow = NOTESKIN:GetPath(symbol, "shadow")
local colors = { TapNoteScore_W1 = "#66ffff", TapNoteScore_W2 = "#66ffff",
    TapNoteScore_W3 = "#99ff99", TapNoteScore_W4 = "#ff9966", TapNoteScore_W5 = "#ccccff" }
local danoniColors = { TapNoteScore_W1 = "#66ffff", TapNoteScore_W2 = "#99ff99",
    TapNoteScore_W3 = "#ff9966", TapNoteScore_W4 = "#ccccff" }
local function holding(self, visible) self:GetChild("Freeze"):visible(visible) end
return Def.ActorFrame {
    ColumnJudgmentMessageCommand = function(self, params)
        local tint = (params.DanoniJudgments and danoniColors or colors)[params.TapNoteScore or ""]
        if tint then
            local auto = params.Autoplay
            self:GetChild("Hit"):stoptweening()
                :xy(auto and 0 or (params.HitX or 0), auto and 0 or (params.HitY or 0))
                :diffuse(color(auto and "#999999" or tint)):diffusealpha(auto and 1 or 0.75)
                :sleep(4 / 60):diffusealpha(0)
        end
    end,
    HoldingOnCommand = function(self) holding(self, true) end,
    HoldingOffCommand = function(self) holding(self, false) end,
    RollOnCommand = function(self) holding(self, true) end,
    RollOffCommand = function(self) holding(self, false) end,
    Def.ActorFrame {
        Name = "Freeze",
        InitCommand = function(self) self:visible(false):diffusealpha(0.9) end,
        Def.Sprite {
            Texture = shadow,
            InitCommand = function(self)
                self:diffuse(color(onigiri and "#000000" or "#ffffff")):zoom(onigiri and 1 or 66 / 50)
            end,
        },
        Def.Sprite {
            Texture = texture,
            InitCommand = function(self) self:diffuse(color("#cccc33")):visible(onigiri) end,
        },
    },
    Def.Sprite {
        Name = "Hit", Texture = texture,
        InitCommand = function(self) self:zoom(80 / 50):diffusealpha(0) end,
    },
}
