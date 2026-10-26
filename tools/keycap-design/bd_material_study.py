"""All nine original materials under the selected B+D monitor lighting.

Run in separate background Blender; only the saved B+D study is opened.
The interactive project and runtime atlas are never touched.
"""
import json
from pathlib import Path
import bpy
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'target/scratch/keycaps/bd-materials'
MATERIALS=ROOT/'target/scratch/keycaps/materials/materials.json'


def main():
    OUT.mkdir(parents=True,exist_ok=True)
    bpy.ops.wm.open_mainfile(filepath=str(ROOT/'target/scratch/keycaps/monitor-blends/BD-monitor-study.blend'))
    scene=bpy.context.scene
    scene.name='Material comparison / selected B+D monitor lighting'
    caps=[scene.objects[label+' body'] for label in ['e','enter','space']]
    records=json.loads(MATERIALS.read_text())
    def assign(material):
        for cap in caps:
            cap.data.materials.clear(); cap.data.materials.append(material)
    for record in records:
        code=record['code']
        assign(bpy.data.materials[f"{code} {record['color']} / {record['finish']}"])
        scene.render.filepath=str(OUT/f'{code}.png')
        bpy.ops.render.render(write_still=True)
    assign(bpy.data.materials['Reference / previous plum satin'])
    scene.render.filepath=str(OUT/'reference.png')
    bpy.ops.render.render(write_still=True)
    assign(bpy.data.materials['B2 Deep plum / Semi-gloss'])
    bpy.ops.wm.save_as_mainfile(filepath=str(OUT/'BD-material-comparison.blend'))
    (OUT/'materials.json').write_text(json.dumps(records,indent=2))
    (OUT/'lighting.json').write_text(json.dumps({
        'selection':'B+D', 'panel_m':[.60,.34], 'center_m':[0,.60,.27],
        'tilt_back_degrees':15, 'power_w':scene.objects['Monitor panel'].data.energy,
        'world_strength':scene.world.node_tree.nodes['Background'].inputs[1].default_value,
        'note':'Selected lighting held fixed; all nine materials unchanged from the original material study.'
    },indent=2))

if __name__=='__main__': main()
