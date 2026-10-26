"""Light-gray satin calibration in an isolated background Blender process.

Opens only the saved B+D study; never touches the interactive project.
Printing is hidden so the reference isolates the new cap material.
"""
from pathlib import Path
import bpy

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'target/scratch/keycaps/light'
OUT.mkdir(parents=True, exist_ok=True)
bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'target/scratch/keycaps/monitor-blends/BD-monitor-study.blend'))
scene = bpy.context.scene
scene.name = 'Light gray satin / B+D monitor reference'
material = bpy.data.materials['A1 Charcoal / Satin'].copy()
material.name = 'Light gray / Satin'
material.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value = (.55, .56, .58, 1)
for label in ('e', 'enter', 'space'):
    cap = scene.objects[label + ' body']
    cap.data.materials.clear()
    cap.data.materials.append(material)
    scene.objects[label + ' surface printing'].hide_render = True
scene.cycles.samples = 64
scene.render.filepath = str(OUT / 'light-satin.png')
bpy.ops.wm.save_as_mainfile(filepath=str(OUT / 'light-satin-study.blend'))
bpy.ops.render.render(write_still=True)
