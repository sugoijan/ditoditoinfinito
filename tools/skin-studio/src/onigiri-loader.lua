-- MIT: wrap the inherited loader without modifying its button tables.
local ret = ... or {}
local inherited = ret.Load
ret.Load = function(...)
    if Var "Button" ~= "Onigiri" then return inherited(...) end
    local part = Var "Element"
    if part == "Receptor" or part == "Explosion" then
        return LoadActor(NOTESKIN:GetPath("Onigiri", part))
    end
    if part == "Tap Note" or part == "Tap Lift" or part == "Tap Fake" or string.find(part, "Head") then
        local actor = LoadActor(NOTESKIN:GetPath("Onigiri", "Tap Note"))
        if part == "Tap Fake" then actor.InitCommand = function(self) self:diffusealpha(0.6) end end
        return actor
    end
    if string.find(part, "Explosion") then
        if Var "SpriteOnly" then
            return Def.Sprite { Texture = NOTESKIN:GetPath("_studio onigiri", "tap"), InitCommand = function(self) self:visible(false) end }
        end
        return Def.Actor {}
    end
    -- Bodies, caps, tails and mines retain the source skin's Down behaviour.
    -- Restore Var even if that loader rejects a part.
    local previous = Var
    _G.Var = function(name)
        if name == "Button" then return "Down" end
        return previous(name)
    end
    local ok, actor = pcall(inherited, ...)
    _G.Var = previous
    if not ok then error(actor) end
    return actor
end
return ret
