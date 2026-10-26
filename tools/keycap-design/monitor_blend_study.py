"""Midpoint and combined B+D monitor positions; isolated background Blender."""
import json
import math
from pathlib import Path
import bpy
from mathutils import Vector
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'target/scratch/keycaps/monitor-blends'
CASES=[('M','Midpoint',.50,.27,7.5),('BD','B + D',.60,.27,15)]

def main():
    OUT.mkdir(parents=True,exist_ok=True)
    bpy.ops.wm.open_mainfile(filepath=str(ROOT/'target/scratch/keycaps/monitor/monitor-light-study.blend'))
    scene=bpy.context.scene
    scene.name='Monitor midpoint and combined B+D'
    scene.world.node_tree.nodes['Background'].inputs[1].default_value=.08
    monitor=scene.objects['Monitor panel']; monitor.data.energy=4.5
    scene.cycles.samples=144
    records=[]
    for code,title,distance,height,tilt in CASES:
        angle=math.radians(tilt)
        monitor.location=(0,distance,height)
        monitor.rotation_euler=Vector((0,-math.cos(angle),math.sin(angle))).to_track_quat('-Z','Y').to_euler()
        scene.render.filepath=str(OUT/f'{code}.png')
        bpy.ops.render.render(write_still=True)
        records.append(dict(code=code,title=title,distance_m=distance,center_height_m=height,
                            tilt_back_degrees=tilt,power_w=4.5,world_strength=.08))
        bpy.ops.wm.save_as_mainfile(filepath=str(OUT/f'{code}-monitor-study.blend'))
    (OUT/'positions.json').write_text(json.dumps(records,indent=2))

if __name__=='__main__': main()
