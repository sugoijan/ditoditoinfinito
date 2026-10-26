"""Original OSA-inspired keycap study. Run with Blender's --background --python.

The mesh is an original rounded, tapered shell with a shallow concave crown,
not a copy of manufacturer CAD. Millimeter-like modeling units; no switch
mount or underside, since this is UI artwork rather than a printable part.
"""
import math
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "target/scratch/keycaps"
OUT.mkdir(parents=True, exist_ok=True)


def material(name, color):
    mat = bpy.data.materials.new(name)
    mat.diffuse_color = (*color, 1)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    bsdf.inputs["Base Color"].default_value = (*color, 1)
    bsdf.inputs["Roughness"].default_value = 0.36
    return mat


def keycap(scene, name, units, mat):
    # Rounded-square rings: smooth shoulders roll into a shallow bowl.
    rings = [(8.55, 0.0, 4), (8.92, .25, 4), (9.0, .6, 4),
             (8.98, 1.0, 4), (7.8, 7.9, 3.6), (7.60, 8.65, 3.6),
             (7.28, 9.05, 3.6), (6.85, 9.15, 3.6),
             (6.4, 9.02, 3.6), (5.7, 8.78, 3.6),
             (4.7, 8.48, 3.6), (3.5, 8.23, 3.6),
             (2.1, 8.04, 3.6), (.7, 7.96, 3.6)]
    vertices, faces = [], []
    n = 96
    extra = (units - 1) * 9.5
    for radius, z, exponent in rings:
        for j in range(n):
            a = math.tau * j / n
            co, si = math.cos(a), math.sin(a)
            x = math.copysign(abs(co) ** (2 / exponent), co)
            y = math.copysign(abs(si) ** (2 / exponent), si)
            vertices.append((x * (radius + extra), y * radius, z + y * .3))
    for i in range(len(rings) - 1):
        for j in range(n):
            a, b = i * n + j, i * n + (j + 1) % n
            faces.append((a, b, b + n, a + n))
    faces.append(tuple(range((len(rings) - 1) * n, len(vertices))))
    faces.append(tuple(reversed(range(n))))
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    scene.collection.objects.link(obj)
    obj.data.materials.append(mat)
    for polygon in mesh.polygons:
        polygon.use_smooth = True
    return obj


def area(scene, name, location, power, size):
    data = bpy.data.lights.new(name, "AREA")
    data.energy = power
    data.shape = "DISK"
    data.size = size
    obj = bpy.data.objects.new(name, data)
    scene.collection.objects.link(obj)
    obj.location = location
    obj.rotation_euler = (-obj.location).to_track_quat("-Z", "Y").to_euler()


def main():
    scene = bpy.data.scenes.new("Infinito keycaps")
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 48
    scene.cycles.use_denoising = True
    scene.render.film_transparent = True
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.view_settings.view_transform = "AgX"
    scene.world = bpy.data.worlds.new("Keycap studio")
    scene.world.use_nodes = True
    scene.world.node_tree.nodes["Background"].inputs[0].default_value = (.65, .7, .8, 1)
    scene.world.node_tree.nodes["Background"].inputs[1].default_value = .35
    area(scene, "Softbox upper left", (-35, 15, 55), 38000, 38)
    area(scene, "Front fill", (20, -35, 22), 7500, 30)
    cam = bpy.data.objects.new("Near overhead orthographic", bpy.data.cameras.new("Camera"))
    scene.collection.objects.link(cam)
    target = Vector((0, 0, 4))
    cam.location = (0, -38, 90)
    cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
    cam.data.type = "ORTHO"
    scene.camera = cam
    plum = material("Infinito dark plum PBT", (.045, .035, .060))
    caps = [keycap(scene, f"Cap {u}u", u, plum) for u in (1, 1.5, 2, 2.5)]
    for i, cap in enumerate(caps):
        for obj in caps:
            obj.hide_render = obj != cap
        width = 128 + i * 48
        scene.render.resolution_x = width
        scene.render.resolution_y = 128
        scene.render.resolution_percentage = 100
        cam.data.ortho_scale = 23 * width / 128
        scene.render.filepath = str(OUT / f"cap-{i}.png")
        bpy.ops.render.render(scene=scene.name, write_still=True)
    for i, cap in enumerate(caps):
        cap.hide_render = False
        cap.location.x = i * 42
    target = Vector((63, 0, 4))
    cam.location = (63, -38, 90)
    cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
    cam.data.ortho_scale = 174
    scene.render.resolution_x = 1280
    scene.render.resolution_y = 240
    bpy.data.libraries.write(str(OUT / "keycaps.blend"), {scene}, fake_user=True)


if __name__ == "__main__":
    main()
