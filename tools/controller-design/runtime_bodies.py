"""Blank bodies for runtime direction masks and live raw-control labels.
Run in a factory-startup background Blender, never the interactive scene.
"""
from pathlib import Path
import runpy
import bpy
source=runpy.run_path(str(Path(__file__).with_name('generate.py')))
icon=source['icon']
icon.__globals__['MATERIAL_STYLE']='shared'
out=source['OUT']/'runtime'
out.mkdir(parents=True,exist_ok=True)
for family,control in [(f,'dpad') for f in source['FAMILIES']]+[('generic','raw')]:
    bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
    scene=bpy.context.scene
    camera=source['rig'](scene,1.3)
    width,aspect=icon(family,control,blank=True)
    camera.data.ortho_scale=width*source['MM']
    scene.render.resolution_x=384;scene.render.resolution_y=384
    scene.render.filepath=str(out/f'{family}-{control}.png')
    bpy.ops.render.render(write_still=True)
