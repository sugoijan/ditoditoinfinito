-- What StepMania defines for noteskins before their Lua runs, as far as
-- skins use it: `Def` and `..` (Themes/_fallback/Scripts/02 ActorDef.lua),
-- `LoadActor`, `color`, the alignment names (02 Actor.lua), `ivalues`
-- (01 base.lua), `Var` (thread variables), NOTESKIN, GAMESTATE, PREFSMAN,
-- OutFox's and `_fallback`'s few names skins read (INPUTFILTER, THEME,
-- `JudgmentLineToColor`, `GAMESTATE:GetIsFieldReversed`), and the driver
-- that plays actors' commands. Written for this game after
-- StepMania 5.1's behaviour (5_1-new @ 825467b), not copied from it.
--
-- `...` is the host table (crates/noteskin/src/lua/host.rs): Rust functions this
-- file wraps. Nothing here reaches outside the skin's files.

local H = ...
local type, pairs, error, setmetatable, getmetatable, tostring, select =
	type, pairs, error, setmetatable, getmetatable, tostring, select

-- Logging (Trace, Warn, lua.ReportScriptError): kept for the import report.
Trace = function(s) end
Warn = function(s) H.warn(tostring(s)) end
lua = {
	ReportScriptError = function(s) H.warn(tostring(s)) end,
	Trace = Trace,
	Warn = Warn,
}
print = function(...)
	local t = {}
	for i = 1, select('#', ...) do t[i] = tostring((select(i, ...))) end
	H.warn(table.concat(t, '\t'))
end

-- Like ipairs(), but returns only values.
function ivalues(t)
	local n = 0
	return function()
		n = n + 1
		return t[n]
	end
end

left = "HorizAlign_Left"
center = "HorizAlign_Center"
right = "HorizAlign_Right"
top = "VertAlign_Top"
middle = "VertAlign_Middle"
bottom = "VertAlign_Bottom"

color = H.color

-- 01 alias.lua's names for blend modes.
Blend = {
	Normal = 'BlendMode_Normal',
	Add = 'BlendMode_Add',
	Subtract = 'BlendMode_Subtract',
	Modulate = 'BlendMode_Modulate',
	CopySource = 'BlendMode_CopySrc',
	AlphaMask = 'BlendMode_AlphaMask',
	AlphaKnockOut = 'BlendMode_AlphaKnockOut',
	AlphaMultiply = 'BlendMode_AlphaMultiply',
	Multiply = 'BlendMode_WeightedMultiply',
	Invert = 'BlendMode_InvertDest',
	NoEffect = 'BlendMode_NoEffect',
}
function StringToBlend(s) return Blend[s] or nil end

-- StepMania's numeric helpers (`_fallback`'s utilities, as it defines
-- them): `scale` maps x from [l1, h1] onto [l2, h2], `clamp` bounds it.
function scale(x, l1, h1, l2, h2)
	return (x - l1) * (h2 - l2) / (h1 - l1) + l2
end
function clamp(x, low, high)
	return math.max(low, math.min(x, high))
end

function ProductFamily() return "ITGmania" end
function ProductVersion() return "1.0.0" end
function ProductID() return "ITGmania" end

-- Thread variables: what NOTESKIN:LoadActor sets while an element loads.
local vars = {}
-- Whether the element the engine is loading read its color (`Var
-- "Color"`): then it is loaded once per color, as ITGmania and OutFox load
-- notes (they do it whether or not it is read). A load the skin's own
-- script makes sees the 4ths' (below) and does not count.
local color_read = false
local depth = 0
function Var(name)
	if name == "Color" and depth == 1 then color_read = true end
	return vars[name]
end

local function with_vars(set, f, ...)
	local old = vars
	local new = {}
	for k, v in pairs(old) do new[k] = v end
	for k, v in pairs(set) do new[k] = v end
	vars = new
	local ok, r = pcall(f, ...)
	vars = old
	if not ok then error(r, 0) end
	return r
end

-- Actor definitions.

local function MergeTables(left, right)
	local ret = {}
	for key, val in pairs(left) do
		ret[key] = val
	end
	for key, val in pairs(right) do
		if ret[key] then
			if type(val) == "function" and type(ret[key]) == "function" then
				local f1 = ret[key]
				local f2 = val
				val = function(...)
					f1(...)
					return f2(...)
				end
			else
				Warn(string.format("Overriding \"%s\": %s with %s", tostring(key),
					type(ret[key]), type(val)))
			end
		end
		ret[key] = val
	end
	setmetatable(ret, getmetatable(left))
	return ret
end

DefMetatable = {
	__concat = function(left, right)
		return MergeTables(left, right)
	end
}

-- `Def.Class { … }`: the table, its class, and the folder of the skin file
-- that wrote it (relative paths are resolved from there).
Def = setmetatable({}, {
	__index = function(self, Class)
		return function(t)
			if not H.is_class(Class) then
				error(Class .. " is not a registered actor class", 2)
			end
			t.Class = Class
			t._Source, t._Line, t._Dir = H.caller()
			setmetatable(t, DefMetatable)
			return t
		end
	end,
})

ActorUtil = {
	IsRegisteredClass = function(c) return H.is_class(c) end,
	GetFileType = function(path) return H.file_type(path) end,
	ResolvePath = function(path, level, optional) return H.resolve(path, optional) end,
}

function ResolveRelativePath(path, level, optional)
	if path:sub(1, 1) ~= "/" then
		local _, _, dir = H.caller()
		if not dir then error(path .. ": no folder to resolve it from", 2) end
		path = dir .. path
	end
	return H.resolve(path, optional)
end

function loadfile(path)
	if type(path) ~= "string" then
		return nil, "loadfile: the path must be a string"
	end
	if path:sub(1, 1) ~= "/" then
		local _, _, dir = H.caller()
		if dir then path = dir .. path end
	end
	return H.loadfile(path)
end

-- Load an actor template.
function LoadActorFunc(path, level)
	if path == "" then
		error("Passing in a blank filename is a great way to eat up RAM. Good thing we warn you about this.")
	end
	local resolved = ResolveRelativePath(path, 1)
	if not resolved then
		error(path .. ": not found", 2)
	end
	path = resolved
	local Type = H.file_type(path)
	if Type == "FileType_Lua" then
		local chunk, errmsg = H.loadfile(path)
		if not chunk then error(errmsg, 0) end
		return chunk
	end
	if Type == "FileType_Bitmap" or Type == "FileType_Movie" then
		return function() return Def.Sprite { Texture = path } end
	elseif Type == "FileType_Sound" then
		return function() return Def.Sound { File = path } end
	elseif Type == "FileType_Model" then
		return function()
			return Def.Model { Meshes = path, Materials = path, Bones = path }
		end
	elseif Type == "FileType_Directory" then
		return function() return Def.BGAnimation { AniDir = path } end
	end
	error(path .. ": unknown file type (" .. tostring(Type) .. ")", 2)
end

function LoadActor(path, ...)
	local t = LoadActorFunc(path, 2)
	return t(...)
end

function LoadActorWithParams(path, params, ...)
	local t = LoadActorFunc(path, 2)
	return with_vars(params or {}, t, ...)
end

function WrapInActorFrame(t)
	return Def.ActorFrame { children = t }
end

NullActor = { Class = "Actor", _Source = "(null actor)" }

-- The noteskin.

NOTESKIN = {}

function NOTESKIN:GetPath(button, element)
	return H.get_path(tostring(button), tostring(element))
end

function NOTESKIN:GetMetric(button, name)
	return H.metric(tostring(button), tostring(name))
end

function NOTESKIN:GetMetricF(button, name)
	return H.metric_number(tostring(button), tostring(name), "f")
end

function NOTESKIN:GetMetricI(button, name)
	return H.metric_number(tostring(button), tostring(name), "i")
end

function NOTESKIN:GetMetricB(button, name)
	return H.metric_number(tostring(button), tostring(name), "i") ~= 0
end

-- `ActorUtil::ParseActorCommands`: the metric compiled to a function.
function NOTESKIN:GetMetricA(button, name)
	local chunk = H.metric_function(tostring(button), tostring(name))
	if chunk then return chunk() end
	return nil
end

local loader -- the table the NoteSkin.lua chain returned

-- `NoteSkinManager::PushActorTemplate`: the skin's `Load` with the element's
-- thread variables. `color` is the note type's (`4th` … `192nd`) for an
-- element the engine loads; a script's own `NOTESKIN:LoadActor` loads the
-- 4ths', as ITGmania's does.
local function template(button, element, sprite_only, color)
	if not loader or type(loader.Load) ~= "function" then
		error("No loader for noteskin!", 0)
	end
	if depth == 0 then color_read = false end
	depth = depth + 1
	local ok, t = pcall(with_vars, {
		Button = button,
		-- The game's button (OutFox): a style's buttons are the game's.
		GameButton = button,
		Element = element,
		SpriteOnly = sprite_only,
		Player = "PlayerNumber_P1",
		Controller = "GameController_1",
		Color = color or "4th",
	}, loader.Load)
	depth = depth - 1
	if not ok then error(t, 0) end
	if type(t) ~= "table" then
		error("Load: must return a table", 0)
	end
	return t, depth == 0 and color_read
end

function NOTESKIN:LoadActor(button, element)
	-- One value: a call ending a table constructor would add the second.
	return (template(tostring(button), tostring(element), false))
end

function NOTESKIN:GetNoteSkinNames() return { H.skin_name() } end
function NOTESKIN:DoesNoteSkinExist(name) return name == H.skin_name() end

-- The engine's state, as far as skins read it while loading and playing.

local style_info = H.style()
local game = { GetName = function() return style_info.game end }
local style = {
	ColumnsPerPlayer = function() return style_info.columns end,
	GetStyleType = function() return style_info.style_type end,
	GetStepsType = function() return style_info.steps_type end,
	GetName = function() return style_info.name end,
}
local player_options = {}
local player_state = {
	GetPlayerOptionsString = function() return H.player_options() end,
	GetPlayerNumber = function() return "PlayerNumber_P1" end,
	GetPlayerOptions = function() return player_options end,
	GetSongPosition = function() return GAMESTATE:GetSongPosition() end,
}
local song_position = {
	GetMusicSeconds = function() return H.song_position("seconds") end,
	GetSongBeat = function() return H.song_position("beat") end,
	GetSongBeatVisible = function() return H.song_position("beat") end,
	GetMusicSecondsVisible = function() return H.song_position("seconds") end,
	GetCurBPS = function() return H.song_position("bps") end,
	GetFreeze = function() return false end,
	GetDelay = function() return false end,
}
local timing = {
	GetBeatFromElapsedTime = function(_, t) return H.timing("beat", t) end,
	GetBPMAtBeat = function(_, b) return H.timing("bps", b) * 60 end,
	GetElapsedTimeFromBeat = function(_, b) return H.timing("seconds", b) end,
}
local steps = {
	GetTimingData = function() return timing end,
	GetStepsType = function() return style_info.steps_type end,
	GetDifficulty = function() return H.difficulty() end,
}
local song = {
	GetTimingData = function() return timing end,
	GetFirstBeat = function() return H.timing("first_beat") end,
	GetLastBeat = function() return H.timing("last_beat") end,
}

GAMESTATE = {
	GetCurrentGame = function() return game end,
	GetCurrentStyle = function() return style end,
	GetPlayerState = function() return player_state end,
	GetCurrentSteps = function() return steps end,
	GetCurrentSong = function() return song end,
	GetCurrentTrail = function() return nil end,
	IsCourseMode = function() return false end,
	GetMasterPlayerNumber = function() return "PlayerNumber_P1" end,
	GetNumPlayersEnabled = function() return 1 end,
	IsPlayerEnabled = function(_, pn) return pn == "PlayerNumber_P1" or pn == 0 end,
	IsHumanPlayer = function(_, pn) return pn == "PlayerNumber_P1" or pn == 0 end,
	GetGameplayLeadIn = function() return H.song_position("lead_in") end,
	GetSongPosition = function() return song_position end,
	GetSongBeat = function() return H.song_position("beat") end,
	Env = function() return H.env() end,
}

PREFSMAN = {
	GetPreference = function(_, name) return H.preference(tostring(name)) end,
}

-- What OutFox and StepMania's `_fallback` theme add that skins read (Dake):
-- the field's direction, the key a game button is mapped to (none: the
-- player's bindings are not shown to skins), a few theme metrics, and a
-- judgement's color. The colors are the game's own (the receptor
-- flashes', `crates/render/src/scene.rs`; a test keeps them equal), not
-- `_fallback`'s, which shows Pro judgements in white and unknown names in
-- black: here Pro judgements take the plain ones' colors and unknown
-- names white.
GAMESTATE.GetIsFieldReversed = function() return H.field_reversed() end

INPUTFILTER = {
	GameButtonToKeyMapped = function() return "" end,
}

local theme_metrics = {
	NoteField = { NoteFieldType = "", ShowBoard = false },
}
THEME = {
	GetMetric = function(_, group, name)
		local g = theme_metrics[tostring(group)]
		if g then return g[tostring(name)] end
		return nil
	end,
	HasMetric = function(_, group, name)
		local g = theme_metrics[tostring(group)]
		return g ~= nil and g[tostring(name)] ~= nil
	end,
}

local judgment_colors = {
	W1 = "0.55,0.95,1,1",
	W2 = "1,0.9,0.35,1",
	W3 = "0.45,0.95,0.45,1",
	W4 = "0.75,0.5,1,1",
	W5 = "1,0.55,0.3,1",
	Held = "0.55,0.95,0.65,1",
	MaxCombo = "1,0.9,0.35,1",
	Miss = "1,0.3,0.3,1",
}
function JudgmentLineToColor(line)
	local name = tostring(line):gsub("^JudgmentLine_", ""):gsub("^Pro", "")
	return color(judgment_colors[name] or "1,1,1,1")
end

PLAYER_1 = "PlayerNumber_P1"
PLAYER_2 = "PlayerNumber_P2"

-- Actors: `Actor`, `Sprite` and `ActorFrame` are the method tables (skins
-- add methods to them, as StepMania's own scripts do); the host fills them
-- with the native methods.

Actor = H.actor_methods
Sprite = H.sprite_methods
ActorFrame = H.frame_methods
Model = H.model_methods

-- 02 Sprite.lua.
function Sprite.LinearFrames(NumFrames, Seconds)
	local Frames = {}
	for i = 0, NumFrames - 1 do
		Frames[#Frames + 1] = {
			Frame = i,
			Delay = (1 / NumFrames) * Seconds
		}
	end
	return Frames
end

-- A texture's playback rate (movies): not played here.
function Sprite:rate(f) return self end


local actors, commands, children = H.actors, H.commands, H.children

-- `PlayCommandNoRecurse`. A command that fails is reported and the rest
-- go on, as StepMania's `RunScriptOnStack` does.
local function play_own(id, name, params)
	local c = commands[id]
	local f = c and c[name]
	if f then
		local ok, err = pcall(f, actors[id], params)
		if not ok then H.warn(name .. "Command: " .. tostring(err)) end
	end
end

-- `playcommand`: the actor's command, then its children's, each called
-- with the actor and the parameters.
local function play(id, name, params, depth)
	if depth > 64 then error(name .. ": commands play each other too deeply", 0) end
	play_own(id, name, params)
	local k = children[id]
	if k then
		for i = 1, #k do play(k[i], name, params, depth + 1) end
	end
end

-- `ActorFrame::SortByDrawOrder`: the frame's children by draw order,
-- played and drawn in that order from then on.
local function sort_children(id)
	if H.sort_children(actors[id]) then
		children[id] = H.child_ids(actors[id])
	end
end

function ActorFrame:SortByDrawOrder()
	sort_children(H.actor_id(self))
	return self
end

-- Several children of one name: a table of them whose methods call the
-- last one's (`ActorFrame.cpp`, `CreateChildTable`).
local function same_named(list)
	return setmetatable(list, { __index = function(t, k)
		if type(k) == "number" then return rawget(t, k) end
		local last = rawget(t, #t)
		local f = last[k]
		return function(_, ...) return f(last, ...) end
	end })
end

function Actor:GetChildren()
	local out = {}
	local ids = H.child_ids(self)
	for i = 1, #ids do
		local a = actors[ids[i]]
		local name = a:GetName()
		local seen = out[name]
		if seen == nil then
			out[name] = a
		elseif type(seen) == "userdata" then
			out[name] = same_named({ seen, a })
		else
			seen[#seen + 1] = a
		end
	end
	return out
end

function Actor:GetChild(name)
	local found
	local ids = H.child_ids(self)
	for i = 1, #ids do
		local a = actors[ids[i]]
		if a:GetName() == name then
			if found == nil then
				found = a
			elseif type(found) == "userdata" then
				found = same_named({ found, a })
			else
				found[#found + 1] = a
			end
		end
	end
	return found
end

function Actor:playcommand(name, params)
	play(H.actor_id(self), tostring(name), params, 0)
	return self
end

-- Init, children first, as each actor plays it once it is loaded.
local function init(id, depth)
	if depth > 64 then error("actors nested too deeply", 0) end
	local k = children[id]
	if k then
		for i = 1, #k do init(k[i], depth + 1) end
		-- A frame sorts its children once they are loaded, before its own
		-- `Init` (`ActorFrame::LoadChildrenFromNode`).
		sort_children(id)
	end
	-- `PlayCommandNoRecurse(Message("Init"))`: an empty table.
	play_own(id, "Init", {})
end

-- What a skin reads that is not there stays nil, as in StepMania; noted
-- when the host traces it.
if H.trace_missing() then
	setmetatable(_G, { __index = function(_, k)
		H.warn("global " .. tostring(k) .. " is not available")
		return nil
	end })
end

-- What the host calls.
return {
	-- Runs the `NoteSkin.lua` chain: each script gets the previous table;
	-- one that fails is reported and the previous table kept.
	chain = function(scripts)
		local ret
		for i = 1, #scripts do
			local ok, r = pcall(scripts[i], ret)
			if ok then ret = r else H.warn(tostring(r)) end
		end
		loader = ret
	end,
	template = template,
	-- A song's background or foreground (`Skin::song`): `LoadActor` of its
	-- folder (an absolute path). A background with `color1` is in the frame
	-- the `UpperLeft` effect puts it in, tinted
	-- (`BackgroundEffects/UpperLeft.lua`); a foreground is the actor itself
	-- (`Foreground::LoadFromSong`).
	actor = function(path, color1)
		if not color1 then return LoadActor(path) end
		local c = color(color1)
		return Def.ActorFrame {
			LoadActor(path) .. {
				OnCommand = function(self) self:diffuse(c):effectclock("music") end,
			},
		}
	end,
	init = function(id) init(id, 0) end,
	play = function(id, name, params) play(id, name, params, 0) end,
	play_own = play_own,
	-- A frame's update function, as `ActorFrame::UpdateInternal` calls
	-- it (an error is reported and the play goes on).
	update = function(id, dt)
		local f = H.update_functions[id]
		if f then
			local ok, err = pcall(f, actors[id], dt)
			if not ok then H.warn("UpdateFunction: " .. tostring(err)) end
		end
	end,
}
