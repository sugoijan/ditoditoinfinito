"""Original OEM row study. Run in a separate background Blender process.

These are visual approximations, not manufacturer CAD or measured tooling.
Row numbers here run from the number row down to ZXCV (other brands reverse
that numbering). The cylinder's axis runs front to back.
"""
import math
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "target/scratch/keycaps/oem"
ROWS = [("Numbers", 11.5, -6), ("QWERTY", 9.6, -2),
        ("Home", 9.0, 4), ("Lower", 10.1, 10)]
WIDTHS = [1, 1.5, 2, 2.5]


def ring(a, b, corner, count=8):
    points = []
    corners = [(a-corner, b-corner, 0), (-a+corner, b-corner, 90),
               (-a+corner, -b+corner, 180), (a-corner, -b+corner, 270)]
    for i, (cx, cy, start) in enumerate(corners):
        for j in range(count):
            angle = math.radians(start + j * 90 / count)
            points.append((cx + corner * math.cos(angle), cy + corner * math.sin(angle)))
        angle = math.radians(start + 90)
        end = (cx + corner*math.cos(angle), cy + corner*math.sin(angle))
        nx, ny, na = corners[(i+1) % 4]
        begin = (nx + corner*math.cos(math.radians(na)), ny + corner*math.sin(math.radians(na)))
        for j in range(24):
            t = j / 24
            points.append((end[0]*(1-t)+begin[0]*t, end[1]*(1-t)+begin[1]*t))
    return points


def geometry(row, units, space=False):
    _, height, angle = ROWS[row]
    slope = math.tan(math.radians(angle))
    extra = (units - 1) * 9.5
    a, b = 6.8 + extra, 7.25
    def surface(x, y):
        if space:
            # A spacebar has a convex crown across its depth, not a scoop
            # across its width. This is a condensed UI bar, not a 6.25u part.
            return 9.0 + 1.5 * (1 - (y / b)**2)
        return height + slope*y - 1.0*(1-(x/a)**2)
    vertices, faces = [], []
    rings = []
    # The walls are tapered, with a small rolled edge rather than a pillow.
    for x, y, radius, z in [(8.65, 8.65, .8, 0), (8.95, 8.95, .9, .25),
                           (9, 9, .9, .55), (8.93, 8.93, .9, 1.0)]:
        rings.append([(px, py, z) for px, py in ring(x+extra, y, radius)])
    for inset, down in [(-.35, .55), (-.20, .20), (0, 0)]:
        rings.append([(x, y + .5, surface(x, y) - down)
                      for x, y in ring(a-inset, b-inset, 1.0)])
    # Concentric top rings all lie on one cylinder, with no spherical dish.
    for t in [.96, .90, .80, .68, .54, .40, .26, .12, .02]:
        rings.append([(x, y + .5, surface(x, y))
                      for x, y in ring(a*t, b*t, min(1, b*t*.3))])
    n = len(rings[0])
    for points in rings:
        vertices.extend(points)
    for i in range(len(rings)-1):
        for j in range(n):
            k = i*n+j
            next_k = i*n+(j+1)%n
            faces.append((k, next_k, next_k+n, k+n))
    faces.append(tuple(range((len(rings)-1)*n, len(vertices))))
    faces.append(tuple(reversed(range(n))))
    return vertices, faces


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    scene = bpy.data.scenes.new("OEM keycap study")
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 96
    scene.cycles.use_denoising = True
    scene.render.film_transparent = True
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.view_settings.view_transform = "AgX"
    scene.world = bpy.data.worlds.new("OEM studio")
    scene.world.use_nodes = True
    bg = scene.world.node_tree.nodes["Background"]
    bg.inputs[0].default_value = (.65, .70, .8, 1)
    bg.inputs[1].default_value = .4
    mat = bpy.data.materials.new("Infinito plum satin PBT")
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    bsdf.inputs["Base Color"].default_value = (.045, .035, .060, 1)
    bsdf.inputs["Roughness"].default_value = .5
    for name, pos, power, size in [("Key", (-35, 5, 55), 34000, 38),
                                    ("Fill", (25, -35, 30), 10000, 35)]:
        data = bpy.data.lights.new(name, "AREA")
        data.energy, data.size = power, size
        obj = bpy.data.objects.new(name, data)
        scene.collection.objects.link(obj)
        obj.location = pos
        obj.rotation_euler = (-obj.location).to_track_quat("-Z", "Y").to_euler()
    camera = bpy.data.objects.new("Camera", bpy.data.cameras.new("OEM camera"))
    scene.collection.objects.link(camera)
    camera.data.type = "ORTHO"
    scene.camera = camera
    caps = []
    for row in range(4):
        for wi, units in enumerate(WIDTHS):
            mesh = bpy.data.meshes.new(f"R{row+1} {units}u")
            vertices, faces = geometry(row, units)
            mesh.from_pydata(vertices, [], faces)
            mesh.update()
            cap = bpy.data.objects.new(mesh.name, mesh)
            scene.collection.objects.link(cap)
            cap.data.materials.append(mat)
            for polygon in mesh.polygons:
                polygon.use_smooth = True
            cap.hide_render = True
            caps.append(cap)
    for row in range(4):
        for wi, units in enumerate(WIDTHS):
            cap = caps[row*4+wi]
            cap.hide_render = False
            camera.location = (0, -38, 90)
            camera.rotation_euler = (Vector((0, 0, 5)) - camera.location).to_track_quat("-Z", "Y").to_euler()
            width = 256 + wi*96
            camera.data.ortho_scale = 23 * width / 256
            scene.render.resolution_x, scene.render.resolution_y = width, 256
            scene.render.resolution_percentage = 100
            scene.render.filepath = str(OUT / f"r{row+1}-{wi}.png")
            bpy.ops.render.render(scene=scene.name, write_still=True)
            if wi == 0:
                camera.location = (70, -3, 15)
                camera.rotation_euler = (Vector((0, 0, 5)) - camera.location).to_track_quat("-Z", "Y").to_euler()
                camera.data.ortho_scale = 23
                scene.render.filepath = str(OUT / f"r{row+1}-side.png")
                bpy.ops.render.render(scene=scene.name, write_still=True)
            cap.hide_render = True
    mesh = bpy.data.meshes.new("Convex spacebar")
    vertices, faces = geometry(3, 2.25, space=True)
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    bar = bpy.data.objects.new(mesh.name, mesh)
    scene.collection.objects.link(bar)
    bar.data.materials.append(mat)
    for polygon in mesh.polygons:
        polygon.use_smooth = True
    camera.location = (0, -38, 90)
    camera.rotation_euler = (Vector((0, 0, 5)) - camera.location).to_track_quat("-Z", "Y").to_euler()
    camera.data.ortho_scale = 23 * 2
    scene.render.resolution_x, scene.render.resolution_y = 512, 256
    scene.render.filepath = str(OUT / "space.png")
    bpy.ops.render.render(scene=scene.name, write_still=True)
    camera.location = (70, -3, 15)
    camera.rotation_euler = (Vector((0, 0, 5)) - camera.location).to_track_quat("-Z", "Y").to_euler()
    camera.data.ortho_scale = 23
    scene.render.resolution_x = 256
    scene.render.filepath = str(OUT / "space-side.png")
    bpy.ops.render.render(scene=scene.name, write_still=True)
    bar.location = (44, -30, 0)
    for i, cap in enumerate(caps):
        cap.hide_render = False
        cap.location = ((i % 4)*44, (i//4)*27, 0)
    camera.location = (66, -90, 185)
    camera.rotation_euler = (Vector((66, 40, 4))-camera.location).to_track_quat("-Z", "Y").to_euler()
    camera.data.ortho_scale = 190
    scene.render.resolution_x, scene.render.resolution_y = 1600, 1000
    bpy.context.window.scene = scene
    bpy.ops.wm.save_as_mainfile(filepath=str(OUT / "oem-keycaps.blend"))


if __name__ == "__main__":
    main()
