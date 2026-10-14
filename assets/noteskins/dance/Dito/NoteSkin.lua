-- Dito: the game's built-in arrows as a noteskin, so that every play runs
-- a skin's Lua (docs/plans/bundled-skins.md). Its art is generated from
-- the built-in arrows' shapes (`cargo run -p xtask -- gen-dito`); each
-- colour scheme is a skin falling back on this one.
local ret = ... or {}

-- Every button is drawn from Down's art, turned; hold and roll heads,
-- lifts and fakes are taps (Down Tap Note.lua picks each one's sheet).
-- Dancing☆Onigiri's onigiri (the Onigiri button) has its own taps and
-- receptor, a star, and Down's holds and explosion.
ret.Redir = function(sButton, sElement)
	if string.find(sElement, "Head") or sElement == "Tap Lift" or sElement == "Tap Fake" then
		sElement = "Tap Note"
	end
	if sButton == "Onigiri" and (sElement == "Tap Note" or sElement == "Receptor") then
		return "Onigiri", sElement
	end
	return "Down", sElement
end

ret.PartsToRotate =
{
	["Receptor"] = true,
	["Tap Note"] = true,
	["Tap Lift"] = true,
	["Tap Fake"] = true,
	["Explosion"] = true,
	["Hold Head Active"] = true,
	["Hold Head Inactive"] = true,
	["Roll Head Active"] = true,
	["Roll Head Inactive"] = true,
}
ret.Rotate =
{
	Up = 180,
	Down = 0,
	Left = 90,
	Right = -90,
	UpLeft = 135,
	UpRight = 225,
	Onigiri = 0,
}

-- The explosion is one actor of its own (Down Explosion.lua).
ret.Blank =
{
	["Tap Explosion Bright"] = true,
	["Tap Explosion Dim"] = true,
	["Hold Explosion"] = true,
	["Roll Explosion"] = true,
	["Hold Topcap Active"] = true,
	["Hold Topcap Inactive"] = true,
	["Roll Topcap Active"] = true,
	["Roll Topcap Inactive"] = true,
	["Hold Tail Active"] = true,
	["Hold Tail Inactive"] = true,
	["Roll Tail Active"] = true,
	["Roll Tail Inactive"] = true,
}

-- common's Load, without default's change that keeps explosions upright:
-- Dito's turns with its button.
ret.Load = ret.CommonLoad

return ret
