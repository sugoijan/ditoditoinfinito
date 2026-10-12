-- Alpha-Arrows by Aozora Tori (Apache-2.0): its images as upstream. This
-- Lua and metrics.ini are Dito Dito Infinito's own (MIT), written for
-- those images; the original's code derives from skins without a licence
-- (its readme), so it is not bundled.
local ret = ... or {}

-- How far each button's arrow turns from Down's art.
local turns =
{
	Up = 180,
	Down = 0,
	Left = 90,
	Right = -90,
	UpLeft = 135,
	UpRight = 225,
}

-- Hold and roll top caps exist for the diagonal arrows only (their art is
-- UpLeft's); tails are never drawn.
local function blank(button, element)
	if string.find(element, "Tail") then
		return true
	end
	return string.find(element, "Topcap") ~= nil and button ~= "UpLeft" and button ~= "UpRight"
end

-- Parts drawn as Down's art turned to the button; the rest is drawn
-- upright (bodies and caps run along the lane, the lift's diamond is the
-- same every way, the mine spins, the explosion turns its own glow).
local turned =
{
	["Receptor"] = true,
	["Tap Note"] = true,
	["Tap Fake"] = true,
	["Hold Head Active"] = true,
	["Hold Head Inactive"] = true,
	["Roll Head Active"] = true,
	["Roll Head Inactive"] = true,
}

ret.Load = function()
	local button = Var "Button"
	local element = Var "Element"
	if blank(button, element) then
		if Var "SpriteOnly" then
			return LoadActor(NOTESKIN:GetPath("", "_blank")) .. {
				InitCommand = function(self)
					self:visible(false)
				end,
			}
		end
		return Def.Actor {}
	end
	-- Heads and fakes are taps.
	local file = element
	if string.find(element, "Head") or element == "Tap Fake" then
		file = "Tap Note"
	end
	local art = string.find(element, "Topcap") and "UpLeft" or "Down"
	local t = LoadActor(NOTESKIN:GetPath(art, file))
	if turned[element] then
		t.BaseRotationZ = turns[button]
	end
	return t
end

return ret
