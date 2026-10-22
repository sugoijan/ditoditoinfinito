local ret = ... or {};

--Defining on which direction the other directions should be based on
--This will let us use less files which is quite handy to keep the noteskin directory nice
--Do remember this will Redirect all the files of that Direction to the Direction its pointed to
ret.RedirTable =
{
	Up = "Down",
	Down = "Down",
	Left = "Down",
	Right = "Down",
	UpLeft = "Down",
	UpRight = "Down",
};

-- < 
--Between here we usually put all the commands the noteskin.lua needs to do, some are external in other files
local DerivedLongLanes = { Center=true, Onigiri=true, UpLeft=true, UpRight=true, DownLeft=true, DownRight=true };
local OldRedir = ret.Redir;
ret.Redir = function(sButton, sElement)
    local e = string.lower(sElement);
    if e == "tap mine" then
        return sButton == "Onigiri" and "Center" or sButton, sElement;
    end
    if string.find(e, "^roll head") or string.find(e, "^roll body") or string.find(e, "^roll bottomcap") then
        return sButton == "Onigiri" and "Center" or sButton, sElement;
    end
    if DerivedLongLanes[sButton] and
        (string.find(e, "^hold head") or string.find(e, "^roll head") or
         string.find(e, "^hold body") or string.find(e, "^hold bottomcap")) then
        return sButton == "Onigiri" and "Center" or sButton, sElement;
    end
	-- Our center extension also supplies the onigiri lane's fallback.
	-- Heads, receptors, long notes and mines follow the square silhouette.
	if sButton == "Center" or sButton == "Onigiri" then
		if sElement == "Hold Body Active" or sElement == "Hold Body Inactive" or
            sElement == "Hold Bottomcap Active" or sElement == "Hold Bottomcap Inactive" then
            return "Center", sElement;
        end
        if sElement == "Tap Fake" then
			return "Center", "Tap Note";
		end
		if sElement == "Tap Note" or sElement == "Tap Lift" or
			sElement == "Receptor" or sElement == "Flash Bright" or
			sElement == "Flash Dim" or string.find(sElement, "Head") then
			return "Center", sElement;
		end
		sButton = "Down";
	end
	sButton, sElement = OldRedir(sButton, sElement);

	if not string.find(sElement, "Head") and
	not string.find(sElement, "Explosion") then
		if string.find(sElement, "Hold") or
		string.find(sElement, "Hold") then
			return sButton, sElement;
		end
	end

	if not string.find(sElement, "Mine") then
     sButton = ret.RedirTable[sButton];
	end

	return sButton, sElement;
end

local OldFunc = ret.Load;
function ret.Load()
	local t = OldFunc();
    local e = string.lower(Var "Element");
    if string.find(e, "^roll head") then t.BaseRotationZ = nil; end
    if DerivedLongLanes[Var "Button"] and
        (string.find(e, "^hold head") or string.find(e, "^roll head")) then
        t.BaseRotationZ = nil;
    end

	--Explosion should not be rotated; it calls other actors.
	if Var "Element" == "Explosion"	then
		t.BaseRotationZ = nil;
	end
	return t;
end
-- >


-- Parts of noteskins which we want to rotate
ret.PartsToRotate =
{
	["Receptor"] = true,
	["Flash Bright"] = true,
	["Flash Dim"] = true,
	["Tap Note"] = true,
	["Tap Fake"] = true,
	["Tap Lift"] = true,
	["Tap Addition"] = true,
	["Hold Explosion"] = true,
	["Hold Head Active"] = true,
	["Hold Head Inactive"] = true,
	["Roll Explosion"] = true,
	["Roll Head Active"] = true,
	["Roll Head Inactive"] = true,
};
-- Defined the parts to be rotated at which degree
ret.Rotate =
{
	Up = 180,
	Down = 0,
	Left = 90,
	Right = -90,
	UpLeft = 135,
	UpRight = 225,
	Center = 0,
	Onigiri = 0,
};

-- Parts that should be Redirected to _Blank.png
-- you can add/remove stuff if you want
ret.Blank =
{
	["Hold Topcap Active"] = true,
	["Hold Topcap Inactive"] = true,
	["Roll Topcap Active"] = true,
	["Roll Topcap Inactive"] = true,
	["Hold Tail Active"] = true,
	["Hold Tail Inactive"] = true,
	["Roll Tail Active"] = true,
	["Roll Tail Inactive"] = true,
};

-- Explicit lower diagonal lanes also need viable taps/receptors so the
-- loader keeps their new hold artwork instead of falling back to Down.
ret.RedirTable.DownLeft = "Down";
ret.RedirTable.DownRight = "Down";
ret.Rotate.DownLeft = 45;
ret.Rotate.DownRight = 315;

-- Don't forget to close the ret cuz else it wont work ;>
return ret;
