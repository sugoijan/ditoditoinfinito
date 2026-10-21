-- What a song's background and foreground scripts read besides what skins
-- read (`docs/plans/scripted-backgrounds.md`): StepMania's screen
-- (`SCREEN_*`, `_screen`, from `Themes/_fallback/metrics.ini` and
-- `Scripts/02 Utilities.lua`) and the song's folder. Run after the prelude,
-- with the host's table.
local H = ...

local w, h = H.screen()
SCREEN_WIDTH = w
SCREEN_HEIGHT = h
SCREEN_LEFT = 0
SCREEN_TOP = 0
SCREEN_RIGHT = w
SCREEN_BOTTOM = h
SCREEN_CENTER_X = w / 2
SCREEN_CENTER_Y = h / 2
_screen = { w = w, h = h, cx = w / 2, cy = h / 2 }

local song = GAMESTATE:GetCurrentSong()
song.GetSongDir = function() return H.song_dir() end

-- `Def.Sound`'s `play` (`ActorSound`): the shell plays its file on the
-- play's sound; a sprite's `play` is the native one. Its sound's settings
-- are not kept (`get()` answers but changes nothing), nor is `stop`.
local native_play = Actor.play
function Actor:play(...)
	if H.sound(self) then return self end
	return native_play(self, ...)
end
local sound = {}
setmetatable(sound, { __index = function() return function() return sound end end })
function Actor:get() return sound end

-- `Themes/_fallback/Scripts/02 Actor.lua`'s placements on that screen.
function Actor:Center() return self:xy(SCREEN_CENTER_X, SCREEN_CENTER_Y) end
function Actor:CenterX() return self:x(SCREEN_CENTER_X) end
function Actor:CenterY() return self:y(SCREEN_CENTER_Y) end
function Actor:FullScreen() return self:stretchto(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT) end
function Actor:align(h, v) return self:halign(h):valign(v) end
-- `BackgroundFitMode_CoverPreserve`, the default.
function Actor:scale_or_crop_background_no_move()
	return self:scaletocover(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT)
end
function Actor:scale_or_crop_background()
	return self:scale_or_crop_background_no_move():xy(SCREEN_CENTER_X, SCREEN_CENTER_Y)
end
