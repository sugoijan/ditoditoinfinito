-- Extension for StepMania charts: DanOni itself has no mine. A red outline.
return Def.ActorFrame { Def.Sprite {
    Texture = NOTESKIN:GetPath("_onigiri", "shape"),
    InitCommand = function(self) self:diffuse(color("#ff4444")) end,
} }
