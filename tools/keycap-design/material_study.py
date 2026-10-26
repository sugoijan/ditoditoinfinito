"""Isolated Blender material comparison. Never writes the runtime atlas.

First run material_sheet.py prepare with system Python for the decal masks,
then run this script using Blender --background --factory-startup.
"""
import json
import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from generate import ROOT, geometry

OUT = ROOT / 'target/scratch/keycaps/materials'
COLORS = [('A', 'Charcoal', (.009, .010, .013)),
          ('B', 'Deep plum', (.016, .009, .024)),
          ('C', 'Ink blue', (.008, .012, .023))]
FINISHES = [('1', 'Satin', .42, 0., .2),
            ('2', 'Semi-gloss', .24, .12, .15),
            ('3', 'Gloss', .11, .35, .09)]


def printed_cap(scene, label, units, width, ui_width, xpos):
    space = label == 'space'
    vertices, faces = geometry(2, units, space=space)
    mesh = bpy.data.meshes.new(label + ' body')
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    cap = bpy.data.objects.new(mesh.name, mesh)
    scene.collection.objects.link(cap)
    cap.location.x = xpos
    for polygon in mesh.polygons:
        polygon.use_smooth = True
    # Subdivided ink follows exactly the same surface as the previous study.
    a = 6.8 + (units-1)*9.5
    ppmm = 256/23
    cos, sin = 85/math.hypot(38,85), 38/math.hypot(38,85)
    slope = math.tan(math.radians(4))
    inset = (ui_width-27.5)/2 if space else 7.5
    x0 = (inset/ui_width*width-width/2)/ppmm
    def position(px, py):
        x = (px-width/2)/ppmm
        if space:
            y = (128-py)/ppmm/cos
            for _ in range(8):
                z = 9+1.5*(1-((y-.5)/7.25)**2)
                error = cos*y+sin*(z-5)-(128-py)/ppmm
                y -= error/(cos-sin*3*(y-.5)/7.25**2)
            z = 9+1.5*(1-((y-.5)/7.25)**2)
        else:
            y = ((128-py)/ppmm-sin*(8-.5*slope+x0*x0/a**2-5))/(cos+sin*slope)
            z = 8+slope*(y-.5)+x*x/a**2
        return x,y,z+.014
    left,top,right,bottom = json.loads((OUT/'decal-bounds.json').read_text())[label]
    nx,ny = max(1,int((right-left)/3)),max(1,int((bottom-top)/3))
    verts,uv,faces = [],[],[]
    for j in range(ny+1):
        for i in range(nx+1):
            px,py = left+(right-left)*i/nx,top+(bottom-top)*j/ny
            verts.append(position(px,py)); uv.append((px/width,1-py/256))
    for j in range(ny):
        for i in range(nx):
            k=j*(nx+1)+i
            faces.append((k,k+nx+1,k+nx+2,k+1))
    dm=bpy.data.meshes.new(label+' surface printing')
    dm.from_pydata(verts,[],faces); dm.update()
    layer=dm.uv_layers.new(name='Print')
    for polygon in dm.polygons:
        polygon.use_smooth=True
        for loop in polygon.loop_indices:
            layer.data[loop].uv=uv[dm.loops[loop].vertex_index]
    decal=bpy.data.objects.new(dm.name,dm)
    scene.collection.objects.link(decal); decal.location.x=xpos
    mat=bpy.data.materials.new(label+' warm white ink'); mat.use_nodes=True
    nodes=mat.node_tree.nodes; nodes.clear()
    output=nodes.new('ShaderNodeOutputMaterial')
    mix=nodes.new('ShaderNodeMixShader')
    transparent=nodes.new('ShaderNodeBsdfTransparent')
    ink=nodes.new('ShaderNodeBsdfPrincipled')
    ink.inputs['Base Color'].default_value=(.88,.85,.80,1)
    ink.inputs['Roughness'].default_value=.4
    texture=nodes.new('ShaderNodeTexImage')
    texture.image=bpy.data.images.load(str(OUT/f'decal-{label}.png'))
    texture.image.pack()
    links=mat.node_tree.links
    links.new(texture.outputs['Alpha'],mix.inputs[0])
    links.new(transparent.outputs[0],mix.inputs[1])
    links.new(ink.outputs[0],mix.inputs[2])
    links.new(mix.outputs[0],output.inputs[0]); dm.materials.append(mat)
    return cap


def main():
    OUT.mkdir(parents=True,exist_ok=True)
    # Factory-startup is only in this separate background process.
    bpy.ops.object.select_all(action='SELECT'); bpy.ops.object.delete(use_global=False)
    scene=bpy.context.scene; scene.name='Dark keycap material comparison'
    scene.render.engine='CYCLES'; scene.cycles.samples=80
    scene.cycles.use_denoising=True
    scene.render.film_transparent=True
    scene.render.image_settings.file_format='PNG'; scene.render.image_settings.color_mode='RGBA'
    scene.render.resolution_x=1200; scene.render.resolution_y=480
    scene.render.resolution_percentage=100
    scene.view_settings.view_transform='AgX'
    scene.world=bpy.data.worlds.new('Neutral studio'); scene.world.use_nodes=True
    bg=scene.world.node_tree.nodes['Background']
    bg.inputs[0].default_value=(.70,.70,.70,1); bg.inputs[1].default_value=.18
    # Broad key plus strip: recognizable highlights at every roughness.
    for name,pos,target,power,sx,sy in [
        ('Large softbox',(-35,-15,65),(0,0,5),45000,100,45),
        ('Back strip',(10,65,48),(0,0,5),30000,125,9),
        ('Gentle fill',(40,-45,35),(0,0,5),8000,75,60),
    ]:
        light=bpy.data.lights.new(name,'AREA'); light.energy=power
        light.shape='RECTANGLE'; light.size=sx; light.size_y=sy
        obj=bpy.data.objects.new(name,light); scene.collection.objects.link(obj)
        obj.location=pos; obj.rotation_euler=(Vector(target)-obj.location).to_track_quat('-Z','Y').to_euler()
    camera=bpy.data.objects.new('Camera',bpy.data.cameras.new('Comparison camera'))
    scene.collection.objects.link(camera); scene.camera=camera
    camera.data.type='ORTHO'; camera.data.ortho_scale=128
    camera.location=(0,-38,90)
    camera.rotation_euler=(Vector((0,0,5))-camera.location).to_track_quat('-Z','Y').to_euler()
    caps=[printed_cap(scene,*args) for args in [
        ('e',1,256,32,-46),('enter',2.5,544,64,-10),('space',2.25,512,64,40)]]
    records=[]
    for letter,color_name,rgb in COLORS:
        for digit,finish,roughness,coat,coat_roughness in FINISHES:
            code=letter+digit
            mat=bpy.data.materials.new(f'{code} {color_name} / {finish}')
            mat.use_nodes=True; node=mat.node_tree.nodes.get('Principled BSDF')
            node.inputs['Base Color'].default_value=(*rgb,1)
            node.inputs['Metallic'].default_value=0
            node.inputs['IOR'].default_value=1.46
            node.inputs['Roughness'].default_value=roughness
            node.inputs['Coat Weight'].default_value=coat
            node.inputs['Coat Roughness'].default_value=coat_roughness
            mat.use_fake_user=True
            for cap in caps:
                cap.data.materials.clear(); cap.data.materials.append(mat)
            scene.render.filepath=str(OUT/f'{code}.png')
            bpy.ops.render.render(write_still=True)
            records.append(dict(code=code,color=color_name,finish=finish,
                base_color_linear=rgb,roughness=roughness,coat=coat,coat_roughness=coat_roughness))
            (OUT/'materials.json').write_text(json.dumps(records,indent=2))
    # Reference makes the color darkening comparable under this same rig.
    reference=bpy.data.materials.new('Reference / previous plum satin')
    reference.use_nodes=True; reference.use_fake_user=True
    node=reference.node_tree.nodes.get('Principled BSDF')
    node.inputs['Base Color'].default_value=(.045,.035,.060,1)
    node.inputs['Roughness'].default_value=.5
    for cap in caps:
        cap.data.materials.clear(); cap.data.materials.append(reference)
    scene.render.filepath=str(OUT/'reference.png')
    bpy.ops.render.render(write_still=True)
    # Reopen on a moderate option, with all nine named materials retained.
    for cap in caps:
        cap.data.materials.clear(); cap.data.materials.append(bpy.data.materials['B2 Deep plum / Semi-gloss'])
    bpy.ops.wm.save_as_mainfile(filepath=str(OUT/'dark-material-study.blend'))

if __name__=='__main__': main()
