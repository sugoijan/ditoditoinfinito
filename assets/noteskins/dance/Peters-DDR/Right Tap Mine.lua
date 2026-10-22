local bolt = NOTESKIN:GetPath("_mine", "electricity")
return Def.ActorFrame {
    Def.Sprite {
        Name="MineCore";
        Texture=NOTESKIN:GetPath("_mine", "right base");
    };
    Def.Sprite {
        Name="LocalElectricity";
        Texture=bolt;
        Frames=Sprite.LinearFrames(8, 0.5);
    };
    Def.Sprite {
        Name="MineLink";
        Texture=bolt;
        Frames=Sprite.LinearFrames(8, 0.5);
    };
}
