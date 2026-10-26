"""Monitor geometry sweep with the accepted faint-room-light setup.

Run in a separate background Blender process. Only our saved study is read.
Brightness, exposure, materials and viewing angle stay fixed across all cases.
Positive tilt means the screen's top leans away from the player.
"""
import json
import math
from pathlib import Path
import bpy
from mathutils import Vector

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'target/scratch/keycaps/monitor-positions'
CASES=[
    ('A','Current',.40,.27,0),
    ('B','Tilted back',.40,.27,15),
    ('C','Tilted forward',.40,.27,-15),
    ('D','Farther away',.60,.27,0),
    ('E','Lower',.40,.20,0),
    ('F','Farther + lower',.60,.20,0),
]


def main():
    OUT.mkdir(parents=True,exist_ok=True)
    bpy.ops.wm.open_mainfile(filepath=str(ROOT/'target/scratch/keycaps/monitor/monitor-light-study.blend'))
    scene=bpy.context.scene
    scene.name='Monitor position comparison / semi-gloss + faint room'
    scene.world.node_tree.nodes['Background'].inputs[1].default_value=.08
    monitor=scene.objects['Monitor panel']
    monitor.data.energy=4.5
    scene.cycles.samples=144
    records=[]
    for code,title,distance,height,tilt in CASES:
        radians=math.radians(tilt)
        normal=Vector((0,-math.cos(radians),math.sin(radians)))
        monitor.location=(0,distance,height)
        monitor.rotation_euler=normal.to_track_quat('-Z','Y').to_euler()
        scene.render.filepath=str(OUT/f'{code}.png')
        bpy.ops.render.render(write_still=True)
        records.append(dict(code=code,title=title,distance_m=distance,center_height_m=height,
                            tilt_back_degrees=tilt,normal=list(normal),power_w=4.5,world_strength=.08))
    (OUT/'positions.json').write_text(json.dumps(records,indent=2))
    # Retain every candidate as an animation frame for convenient inspection.
    for frame,(_,_,distance,height,tilt) in enumerate(CASES,1):
        angle=math.radians(tilt)
        monitor.location=(0,distance,height)
        monitor.rotation_euler=Vector((0,-math.cos(angle),math.sin(angle))).to_track_quat('-Z','Y').to_euler()
        monitor.keyframe_insert(data_path='location',frame=frame)
        monitor.keyframe_insert(data_path='rotation_euler',frame=frame)
        scene.timeline_markers.new(CASES[frame-1][0]+' / '+CASES[frame-1][1],frame=frame)
    scene.frame_start=1; scene.frame_end=len(CASES)
    scene.frame_set(1)
    bpy.ops.wm.save_as_mainfile(filepath=str(OUT/'monitor-position-study.blend'))

if __name__=='__main__': main()
