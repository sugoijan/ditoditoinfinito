-- Judgment instances are driven by the engine; Lua runs only at setup.
if NOTESKIN:GetParameter("Explosions") == false then return Def.ActorFrame {} end
local button = Var "Button"
if button == "Onigiri" and NOTESKIN:GetParameter("Onigiri") == false then button = "Center" end
local shape = button == "Center" and "center" or button == "Onigiri" and "onigiri" or "arrow"
local turn = ({Up=0,Right=90,Down=180,Left=270,UpRight=45,DownRight=135,DownLeft=225,UpLeft=315})[button] or 0
local offset = ({Up=1,Down=-1,Onigiri=-7})[button] or 0
local vary = NOTESKIN:GetParameter("HitVariation") ~= false and 1 or 0
local parts = {}
local function curve(keys, length)
    return Bind.Curve {clock="hit", keys=keys, length=length}
end
local function layer(name, when, tint, opacity, zoom, angle, image, salt)
    local material = Def.Material {File="_spiral effect.wgsl", Space="note",
        Params={tint,{image and 1 or 0,image and vary or 0,salt or 0}},
        Images=image and {"_spiral hit " .. image .. ".svg"} or {}}
    parts[shape .. "-" .. name] = {When=when, Blend="add", Y=offset,
        Opacity=opacity, Zoom=zoom or 1, Rotation=angle or 0, Material=material}
end
local white, yellow, green, mint = "#ffffff", "#fff064", "#3fff3f", "#3effbf"
-- One echo geometry, three independently tinted judgment layers.
-- Framed tint is unnecessary: separate parts keep When explicit and editable.
for _, echo in ipairs({{"tap-echo","W2",yellow}, {"tap-echo-w3","W3",green}, {"tap-echo-w4","W4",mint}}) do
    layer(echo[1], {echo[2]}, echo[3], curve({0,0.9,0.15,0}), curve({0,1,0.15,1.25}), turn)
end
for i,alpha in ipairs({1,0.8}) do
    layer("burst-"..i, {"W1"}, white, curve({0,alpha,0.025,alpha,0.175,0}),
        curve({0,0.85,0.175,1.25}), 0, "burst", 11)
end
layer("best-core", {"W1"}, white, curve({0,0.3,0.045,0}), 0.6)
layer("best-outline", {"W1"}, white, curve({0,0.25,0.025,0.25,0.175,0}),
    curve({0,1,0.175,1.25}), turn)
layer("perfect-core", {"W2"}, white, curve({0,0.9,5/60-0.000001,0.9,5/60,0}),
    curve({0,0.54,3/60,0.7}), 0, "burst", 23)
-- Three overlapping 150 ms echoes, emitted every four frames (the old
-- emitter's slots), with a startup gate to avoid pulses before their start.
for i=1,3 do
    local start, cycle, life = (i-1)*4/60, 12/60, 9/60
    local times = {0,cycle,start,(start+life)%cycle}
    table.sort(times)
    local fade, size = {}, {}
    for _,t in ipairs(times) do
        if t == start and start > 0 then
            fade[#fade+1]=t;fade[#fade+1]=0
            size[#size+1]=t;size[#size+1]=1.25
        end
        local u = start==0 and t==cycle and cycle or (t-start)%cycle
        fade[#fade+1]=t;fade[#fade+1]=0.9*math.max(0,1-u/life)
        size[#size+1]=t;size[#size+1]=1+0.25*math.min(1,u/life)
    end
    layer("hold-"..i, {"Holding","Rolling"}, yellow, curve(fade,cycle), curve(size,cycle), turn)
    -- The longer mine discharge must not extend the existing held-effect tail.
    parts[shape.."-hold-"..i].Tint={"#ffffffff","#ffffff00"}
    parts[shape.."-hold-"..i].TintMix=Bind.Curve {clock="ended",keys={0,0,0.25,0,0.250001,1}}
    if start > 0 then parts[shape.."-hold-"..i].Visible=curve({0,0,start-0.000001,0,start,1}) end
end
layer("finish", {"Held"}, yellow, curve({0,0.9,0.1,0}),
    curve({0,0.7,0.1,0.95}), 0, "burst", 37)
-- A discharge stays around the receptor: fixed size, travelling gaps and
-- flickering filaments shared with live mines, over a red damage flash.
-- No outward scale tween or central burst.
parts[shape.."-mine-core"] = {When={"HitMine"}, Blend="add", Y=offset,
    Tint="#ff1648", Rotation=turn,
    Zoom=curve({0,0.9,1/60,0.9,5/60,1}),
    Opacity=curve({0,0.45,0.035,0.2,0.08,0.5,0.13,0.18,0.21,0.4,0.3,0.1,0.42,0})}
-- The silhouette follows its artwork alignment and press zoom; the circular
-- electricity surrounds the lane anchor, just like the live mine's rim.
for _, side in ipairs({"left","right","link"}) do
for _, layer in ipairs({"glow","wire"}) do
    parts[shape.."-mine-"..layer.."-"..side] = {When={"HitMine"}, Blend="add",
        -- Shared phase keeps every neighboring contour's joins continuous,
        -- even when the mine hits occur at slightly different times.
        Frame=Bind.Steps {clock="seconds",length=0.5,count=16},
        Opacity=layer=="glow"
            and curve({0,0.9,0.03,0.65,0.065,1,0.11,0.4,0.15,0.95,0.23,0.5,0.3,0.75,0.39,0.35,0.45,0})
            or curve({0,0.95,0.02,0.45,0.05,0.9,0.1,0.2,0.14,0.8,0.2,0.25,0.26,0.65,0.34,0.15,0.42,0.4,0.45,0})}
end
end
return Def.Component {File="_spiral explosions.svg",Id="explosion-"..shape,Duration=0.5,Parts=parts}
