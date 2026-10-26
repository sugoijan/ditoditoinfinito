"""Compare studio reflections with a uniform monitor-sized area light.

Run only in a separate Blender --background --factory-startup process.
The source is our saved material study; the interactive project is untouched.
"""
import json
from pathlib import Path
import bpy
from mathutils import Vector

ROOT=Path(__file__).resolve().parents[2]
SOURCE=ROOT/'target/scratch/keycaps/materials'
OUT=ROOT/'target/scratch/keycaps/monitor'


def main():
    OUT.mkdir(parents=True,exist_ok=True)
    bpy.ops.wm.open_mainfile(filepath=str(SOURCE/'dark-material-study.blend'))
    scene=bpy.context.scene
    scene.name='Uniform monitor illumination'
    # The authoring geometry used numeric millimeters. Convert to meters so
    # this rig's dimensions and its watt parameter use ordinary scene units.
    for obj in list(scene.objects):
        if obj.type=='LIGHT':
            bpy.data.objects.remove(obj,do_unlink=True)
        else:
            obj.location *= .001
            obj.scale *= .001
    scene.unit_settings.system='METRIC'
    camera=scene.camera
    camera.data.ortho_scale *= .001
    camera.data.clip_start=.001
    camera.data.clip_end=10
    scene.world.node_tree.nodes['Background'].inputs[1].default_value=0
    scene.cycles.samples=128
    scene.render.resolution_x=1500; scene.render.resolution_y=600
    # A roughly 27-inch, 16:9 screen. Its bottom edge is 10 cm above
    # the desktop; the center lies 40 cm behind the displayed key group.
    light=bpy.data.lights.new('Uniform white monitor · 60 × 34 cm','AREA')
    light.shape='RECTANGLE'; light.size=.60; light.size_y=.34
    light.energy=4.5; light.color=(1,1,1)
    monitor=bpy.data.objects.new('Monitor panel',light)
    scene.collection.objects.link(monitor)
    monitor.rotation_euler=Vector((0,-1,0)).to_track_quat('-Z','Y').to_euler()
    caps=[scene.objects[n+' body'] for n in ['e','enter','space']]
    for position,ambient in [('centered',0.),('room',.08)]:
        monitor.location=(0,.40,.27)
        scene.world.node_tree.nodes['Background'].inputs[1].default_value=ambient
        for number,finish in [('1','Satin'),('2','Semi-gloss'),('3','Gloss')]:
            material=bpy.data.materials[f'B{number} Deep plum / {finish}']
            for cap in caps:
                cap.data.materials.clear(); cap.data.materials.append(material)
            scene.render.filepath=str(OUT/f'{position}-B{number}.png')
            bpy.ops.render.render(write_still=True)
    monitor.location=(0,.40,.27)
    scene.world.node_tree.nodes['Background'].inputs[1].default_value=0
    for cap in caps:
        cap.data.materials.clear()
        cap.data.materials.append(bpy.data.materials['B2 Deep plum / Semi-gloss'])
    bpy.ops.wm.save_as_mainfile(filepath=str(OUT/'monitor-light-study.blend'))
    (OUT/'lighting.json').write_text(json.dumps({
        'dimensions_m':[.60,.34], 'center_m':[0,.40,.27],
        'normal':[0,-1,0], 'room_comparison_world_strength':.08,
        'power_w':4.5, 'world_strength':0,
        'note':'Uniform white panel; illustrative brightness, not calibrated monitor luminance. Same material, geometry and viewing angle as the studio study.'
    },indent=2))

if __name__=='__main__': main()
