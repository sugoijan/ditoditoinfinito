"""A1/A2 mechanical press references under the accepted B+D lighting.

Run in a separate background Blender process. Four millimeters is the chosen
study travel, not a claim about every switch. Caps translate without scaling,
changing material or moving the camera; printing travels with each cap.
"""
import json
from pathlib import Path
import bpy
from mathutils import Vector
from bpy_extras.object_utils import world_to_camera_view

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'target/scratch/keycaps/pressed'
KEYS=['e','enter','space']
MATERIALS=[('A1','Charcoal / Satin'),('A2','Charcoal / Semi-gloss')]


def main():
    OUT.mkdir(parents=True,exist_ok=True)
    bpy.ops.wm.open_mainfile(filepath=str(ROOT/'target/scratch/keycaps/bd-materials/BD-material-comparison.blend'))
    scene=bpy.context.scene
    scene.name='A1-A2 pressed key references / B+D lighting'
    scene.cycles.samples=96
    parts={key:[scene.objects[key+' body'],scene.objects[key+' surface printing']] for key in KEYS}
    objects=[obj for group in parts.values() for obj in group]
    def pose(depth_mm, keys=KEYS):
        for key,group in parts.items():
            for obj in group:
                obj.location.z=-depth_mm*.001 if key in keys else 0
        bpy.context.view_layer.update()
    def render(path):
        scene.render.filepath=str(path)
        bpy.ops.render.render(write_still=True)
    projection=[]
    for depth in range(5):
        p=world_to_camera_view(scene,scene.camera,Vector((0,0,.008-depth*.001)))
        projection.append({'travel_mm':depth,'fraction':depth/4,'image_y_px':(1-p.y)*scene.render.resolution_y})
    y0=projection[0]['image_y_px']
    for sample in projection:
        sample['image_down_px']=sample['image_y_px']-y0
    for code,name in MATERIALS:
        folder=OUT/code; folder.mkdir(exist_ok=True)
        for obj in objects:
            obj.animation_data_clear()
        scene.timeline_markers.clear()
        scene.frame_set(1)
        for key in KEYS:
            cap=parts[key][0]
            cap.data.materials.clear()
            cap.data.materials.append(bpy.data.materials[f'{code} {name}'])
        for depth in range(5):
            pose(depth)
            render(folder/f'depth-{depth}.png')
        for key in KEYS:
            pose(4,[key])
            render(folder/f'pressed-{key}.png')
        # Editable reference animation; each body/print pair has its own
        # track, so subsequent tutorial studies can animate individual keys.
        scene.render.fps=24
        for frame,depth in [(1,0),(10,0),(15,4),(29,4),(34,0),(44,0)]:
            pose(depth)
            for obj in objects:
                obj.keyframe_insert(data_path='location',frame=frame)
        for frame,label in [(1,'Released'),(10,'Press'),(15,'Held / 4 mm'),(29,'Release'),(34,'Released')]:
            scene.timeline_markers.new(label,frame=frame)
        scene.frame_start=1; scene.frame_end=44
        scene.frame_set(1)
        bpy.ops.wm.save_as_mainfile(filepath=str(OUT/f'{code}-pressed-study.blend'))
    (OUT/'travel.json').write_text(json.dumps({
        'travel_mm':4,'axis':'world -Z','cap_and_print_move_together':True,
        'materials':['A1','A2'],'lighting':'B+D plus faint room light',
        'screen_position_m':[0,.60,.27],'screen_tilt_back_deg':15,
        'screen_power_w':scene.objects['Monitor panel'].data.energy,
        'ambient_strength':scene.world.node_tree.nodes['Background'].inputs[1].default_value,
        'projection':projection,
        'travel_at_32px_artboard_px':projection[-1]['image_down_px']*32/(23*scene.render.resolution_x/(scene.camera.data.ortho_scale*1000)),
        'note':'Actual translated meshes, fixed camera and lighting. No keyboard deck/occlusion is introduced. Animation timing is illustrative, not switch dynamics.'
    },indent=2))

if __name__=='__main__': main()
