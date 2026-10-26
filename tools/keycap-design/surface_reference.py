"""Project printed decals on the actual OEM surface in isolated Blender.

Run prepare_reference.py first. Opens only our generated study, never the
interactive session. The decal has the same font/image in every comparison;
its plane is subdivided and conformed to the model's cylindrical top.
"""
import json
import math
from pathlib import Path
import sys
import bpy
from mathutils import Vector
from bpy_extras.object_utils import world_to_camera_view

sys.path.insert(0, str(Path(__file__).resolve().parent))
from generate import geometry, ROOT, OUT

CASES = [("e", 1, 256, 32), ("tab", 1.5, 352, 48),
         ("enter", 2.5, 544, 64), ("space", 2.25, 512, 64)]


def main():
    bpy.ops.wm.open_mainfile(filepath=str(OUT / "oem-keycaps.blend"))
    scene = bpy.context.scene
    for obj in scene.objects:
        if obj.type == "MESH": obj.hide_render = True
    camera = scene.camera
    camera.location = (0, -38, 90)
    camera.rotation_euler = (Vector((0,0,5))-camera.location).to_track_quat("-Z","Y").to_euler()
    cos = 85/math.hypot(38,85)
    sin = 38/math.hypot(38,85)
    ppmm = 256/23
    slope = math.tan(math.radians(4))
    measured = {}
    studies = []
    for label, units, width, ui_width in CASES:
        space = label == "space"
        mesh = bpy.data.meshes.new(f"Reference {label}")
        vertices, faces = geometry(2,units,space=space)
        mesh.from_pydata(vertices, [], faces)
        mesh.update()
        cap = bpy.data.objects.new(mesh.name, mesh)
        scene.collection.objects.link(cap)
        cap.data.materials.append(bpy.data.materials["Infinito plum satin PBT"])
        for polygon in mesh.polygons: polygon.use_smooth = True
        camera.data.ortho_scale = 23*width/256
        scene.render.resolution_x, scene.render.resolution_y = width,256
        scene.render.resolution_percentage = 100
        # Keep blank bodies for the UI and controlled comparisons.
        scene.render.filepath = str(OUT / f"reference-{label}-blank.png")
        bpy.ops.render.render(scene=scene.name,write_still=True)
        if space:
            scene.render.filepath = str(OUT / "space.png")
            bpy.data.images["Render Result"].save_render(scene.render.filepath, scene=scene)
        image = bpy.data.images.load(str(OUT / f"decal-{label}.png"), check_existing=False)
        image.pack()
        material = bpy.data.materials.new(f"Printed {label}")
        material.use_nodes = True
        nodes = material.node_tree.nodes
        nodes.clear()
        output = nodes.new("ShaderNodeOutputMaterial")
        mix = nodes.new("ShaderNodeMixShader")
        transparent = nodes.new("ShaderNodeBsdfTransparent")
        ink = nodes.new("ShaderNodeEmission")
        texture = nodes.new("ShaderNodeTexImage")
        texture.image = image
        links = material.node_tree.links
        links.new(texture.outputs["Color"],ink.inputs["Color"])
        links.new(texture.outputs["Alpha"],mix.inputs[0])
        links.new(transparent.outputs[0],mix.inputs[1])
        links.new(ink.outputs[0],mix.inputs[2])
        links.new(mix.outputs[0],output.inputs[0])
        a = 6.8+(units-1)*9.5
        inset = (ui_width-27.5)/2 if space else 7.5
        x0 = (inset/ui_width*width-width/2)/ppmm
        # The decal's plane matches the old run at its left edge. Let the
        # mesh's actual z(x) and the camera generate the remaining bend.
        def position(px, py):
            x = (px-width/2)/ppmm
            if space:
                # Newton solve the convex crown's projected y for placement.
                y = (128-py)/ppmm/cos
                for _ in range(8):
                    z = 9+1.5*(1-((y-.5)/7.25)**2)
                    error = cos*y+sin*(z-5)-(128-py)/ppmm
                    derivative = cos-sin*3*(y-.5)/7.25**2
                    y -= error/derivative
                z = 9+1.5*(1-((y-.5)/7.25)**2)
            else:
                y = ((128-py)/ppmm-sin*(8-.5*slope+x0*x0/a**2-5))/(cos+sin*slope)
                z = 8+slope*(y-.5)+x*x/a**2
            return (x,y,z+.012)
        # Only the printing region, safely inside the top's shoulders.
        bounds = json.loads((OUT / "decal-bounds.json").read_text())[label]
        left,top,right,bottom = bounds
        nx,ny = max(1,int((right-left)/2)),max(1,int((bottom-top)/2))
        verts, uv, faces = [],[],[]
        for j in range(ny+1):
            for i in range(nx+1):
                px,py = left+(right-left)*i/nx,top+(bottom-top)*j/ny
                verts.append(position(px,py))
                uv.append((px/width,1-py/256))
        for j in range(ny):
            for i in range(nx):
                k=j*(nx+1)+i
                faces.append((k,k+nx+1,k+nx+2,k+1))
        dm = bpy.data.meshes.new(f"Surface print {label}")
        dm.from_pydata(verts,[],faces)
        dm.update()
        layer = dm.uv_layers.new(name="Print")
        for polygon in dm.polygons:
            for loop in polygon.loop_indices:
                layer.data[loop].uv = uv[dm.loops[loop].vertex_index]
        decal = bpy.data.objects.new(dm.name,dm)
        scene.collection.objects.link(decal)
        dm.materials.append(material)
        scene.render.filepath = str(OUT / f"reference-{label}.png")
        bpy.ops.render.render(scene=scene.name,write_still=True)
        if not space:
            bpy.context.view_layer.update()
            points = []
            for px in [inset,ui_width/2,ui_width-inset]:
                p=position(px/ui_width*width,80)
                projected=world_to_camera_view(scene,camera,Vector(p))
                points.append([px,(1-projected.y)*256/8-10])
            measured[label] = {"rise_ui_px":ppmm*sin/8,
                               "half_face_ui_px":a*ppmm*ui_width/width,
                               "projected_offsets_ui":points}
        cap.hide_render = decal.hide_render = True
        studies.append((cap,decal))
    (OUT / "surface-measurements.json").write_text(json.dumps(measured,indent=2))
    for i,(cap,decal) in enumerate(studies):
        cap.hide_render = decal.hide_render = False
        cap.location.x = decal.location.x = i*55
    camera.location = (82.5,-38,90)
    camera.rotation_euler = (Vector((82.5,0,5))-camera.location).to_track_quat("-Z","Y").to_euler()
    camera.data.ortho_scale = 220
    scene.render.resolution_x,scene.render.resolution_y = 1760,400
    # A separate file retains the conformed decal meshes for inspection.
    bpy.ops.wm.save_as_mainfile(filepath=str(OUT / "printed-surface-study.blend"))

if __name__ == "__main__": main()
