"""Original controller-button study; run in an isolated background Blender process.

No external models, textures or fonts. Dimensions are visual approximations,
not measured tooling. This authoring script never changes runtime assets.
"""
import argparse
import json
import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'target/scratch/controller-hints'
MM = .001
MATERIAL_STYLE = 'family'
COLORS = {'A': '#80d943', 'B': '#ed535a', 'X': '#42b9ed', 'Y': '#f5d642',
          'cross': '#477bff', 'circle': '#ff3157', 'square': '#f151ca', 'triangle': '#20d998'}
FAMILIES = ['xbox', 'playstation', 'dualsense', 'nintendo', 'generic']
NAMES = {'xbox': ['A', 'B', 'X', 'Y'], 'playstation': ['cross', 'circle', 'square', 'triangle'],
         'dualsense': ['cross', 'circle', 'square', 'triangle'],
         'nintendo': ['B', 'A', 'Y', 'X'], 'generic': ['south', 'east', 'west', 'north']}


def linear(c):
    x = int(c, 16) / 255
    return x / 12.92 if x <= .04045 else ((x + .055) / 1.055) ** 2.4


def material(name, color, rough=.35, coat=0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    p = m.node_tree.nodes['Principled BSDF']
    p.inputs['Base Color'].default_value = (*[linear(color[i:i+2]) for i in (1, 3, 5)], 1)
    p.inputs['Roughness'].default_value = rough
    p.inputs['Coat Weight'].default_value = coat
    p.inputs['Coat Roughness'].default_value = .12
    return m


def mesh(name, vertices, faces, mat, smooth=True):
    data = bpy.data.meshes.new(name)
    data.from_pydata([tuple(x * MM for x in p) for p in vertices], [], faces)
    data.update()
    data.materials.append(mat)
    for p in data.polygons:
        p.use_smooth = smooth
    obj = bpy.data.objects.new(name, data)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def round_body(name, radius, height, crown, mat, x=0, y=0):
    # Flat cylindrical sidewall, rolled lip and shallow convex face.
    profile = [(radius*.91, 0), (radius*.99, .16), (radius, .32),
               (radius, height-.26), (radius*.98, height-.08), (radius*.93, height)]
    profile += [(radius*t, height+crown*(1-(t/.93)**2)) for t in [.88,.78,.66,.52,.36,.18,.01]]
    n = 96
    vertices = [(x+r*math.cos(a*2*math.pi/n), y+r*math.sin(a*2*math.pi/n), z) for r,z in profile for a in range(n)]
    faces = [(i*n+j,i*n+(j+1)%n,(i+1)*n+(j+1)%n,(i+1)*n+j) for i in range(len(profile)-1) for j in range(n)]
    faces += [tuple(range((len(profile)-1)*n,len(vertices))), tuple(reversed(range(n)))]
    return mesh(name, vertices, faces, mat)


def stroke(name, points, ink, surface, width=.32, closed=False):
    points = list(points)
    if closed:
        points.append(points[0])
    dense = []
    for a,b in zip(points,points[1:]):
        count = max(2, int(math.dist(a,b)*8))
        dense += [(a[0]+(b[0]-a[0])*j/count,a[1]+(b[1]-a[1])*j/count) for j in range(count)]
    dense.append(points[-1])
    # Flat ink ribbons follow the top surface; there is no raised lettering.
    vertices=[]; faces=[]
    for a,b in zip(dense,dense[1:]):
        dx,dy=b[0]-a[0],b[1]-a[1]
        length=math.hypot(dx,dy)
        if length < 1e-9: continue
        nx,ny=-dy/length*width/2,dx/length*width/2
        k=len(vertices)
        for x,y in [(a[0]-nx,a[1]-ny),(b[0]-nx,b[1]-ny),(b[0]+nx,b[1]+ny),(a[0]+nx,a[1]+ny)]:
            vertices.append((x,y,surface(x,y)+.009))
        faces.append((k,k+1,k+2,k+3))
    # Rounded joins and endpoints are flat too.
    for x,y in points:
        k=len(vertices)
        vertices.append((x,y,surface(x,y)+.010))
        for i in range(16):
            px=x+width/2*math.cos(i*math.tau/16);py=y+width/2*math.sin(i*math.tau/16)
            vertices.append((px,py,surface(px,py)+.010))
        faces.extend((k,k+1+i,k+1+(i+1)%16) for i in range(16))
    obj=mesh(name,vertices,faces,ink)
    obj.visible_shadow=False
    return obj


def arc(cx, cy, rx, ry, a=0, b=360):
    return [(cx+rx*math.cos(math.radians(a+(b-a)*i/48)), cy+ry*math.sin(math.radians(a+(b-a)*i/48))) for i in range(49)]


def glyph_paths(label):
    if label in ('cross','X'):
        return [[(-1.7,-2.1),(1.7,2.1)],[(-1.7,2.1),(1.7,-2.1)]]
    if label=='A': return [[(-1.65,-2.1),(0,2.1),(1.65,-2.1)],[(-.98,-.5),(.98,-.5)]]
    if label=='B': return [[(-1.4,-2.1),(-1.4,2.1),(.05,2.1)],arc(.05,1.05,1.3,1.05,-90,90),arc(.05,-1.05,1.55,1.05,-90,90),[(-1.4,0),(.05,0)],[(-1.4,-2.1),(.05,-2.1)]]
    if label=='Y': return [[(-1.65,2.1),(0,.1),(1.65,2.1)],[(0,.1),(0,-2.1)]]
    if label=='circle': return [arc(0,0,2.25,2.25)]
    if label=='square': return [[(-2,-2),(-2,2),(2,2),(2,-2),(-2,-2)]]
    if label=='triangle': return [[(-2.3,-1.7),(0,2.3),(2.3,-1.7),(-2.3,-1.7)]]
    if label=='L': return [[(-1.4,2.1),(-1.4,-2.1),(1.4,-2.1)]]
    if label=='R': return [[(-1.4,-2.1),(-1.4,2.1),(.1,2.1)],arc(.1,1.05,1.3,1.05,-90,90),[(-1.4,0),(.1,0),(1.6,-2.1)]]
    if label=='1': return [[(-.8,1.3),(0,2.1),(0,-2.1)],[(-.9,-2.1),(.9,-2.1)]]
    if label=='2': return [arc(0,1.1,1.45,1,0,180)[::-1]+[(1.45,.8),(-1.5,-2.1),(1.6,-2.1)]]
    if label=='T': return [[(-1.7,2.1),(1.7,2.1)],[(0,2.1),(0,-2.1)]]
    if label=='Z': return [[(-1.7,2.1),(1.7,2.1),(-1.7,-2.1),(1.7,-2.1)]]
    if label=='E': return [[(1.6,2.1),(-1.4,2.1),(-1.4,-2.1),(1.6,-2.1)],[(-1.4,0),(1.1,0)]]
    if label=='C': return [arc(0,0,1.6,2.1,45,315)]
    if label=='S': return [[(1.5,1.7),(.7,2.1),(-.8,2.1),(-1.5,1.5),(-1.5,.7),(-.7,.1),(.7,-.1),(1.5,-.7),(1.5,-1.5),(.8,-2.1),(-.7,-2.1),(-1.5,-1.7)]]
    if label=='7': return [[(-1.6,2.1),(1.6,2.1),(-.7,-2.1)]]
    if label=='+': return [[(-1.9,0),(1.9,0)],[(0,-1.9),(0,1.9)]]
    if label=='-': return [[(-1.9,0),(1.9,0)]]
    if label=='menu': return [[(-1.7,y),(1.7,y)] for y in [-1.15,0,1.15]]
    if label=='view': return [[(-1.9,-.35),(-1.9,1.5),(.5,1.5),(.5,.65)],[(-.5,-1.5),(-.5,.6),(1.9,.6),(1.9,-1.5),(-.5,-1.5)]]
    if label=='create': return [[(-1.8,1),(-1.8,-1.6),(1.8,-1.6),(1.8,1)],[(0,.1),(0,2.3)],[(-.8,1.5),(0,2.3),(.8,1.5)]]
    raise ValueError(label)


def legend(label, ink, surface, size=1, xy=(0,0), width=.36):
    for i,path in enumerate(glyph_paths(label)):
        points = [(x*size+xy[0],y*size+xy[1]) for x,y in path]
        stroke('Ink '+label+str(i),points,ink,surface,width)


def polygon_body(name, points, h, mat, bevel=.3):
    if sum(a[0]*b[1]-a[1]*b[0] for a,b in zip(points,points[1:]+points[:1])) < 0:
        points=list(reversed(points))
    n=len(points)
    vertices=[(x,y,z) for z in [0,h] for x,y in points]
    faces=[tuple(reversed(range(n))),tuple(range(n,2*n))]
    faces += [(i,(i+1)%n,(i+1)%n+n,i+n) for i in range(n)]
    obj=mesh(name,vertices,faces,mat,False)
    mod=obj.modifiers.new('Molded edge','BEVEL'); mod.width=bevel*MM; mod.segments=5
    obj.modifiers.new('Weighted normals','WEIGHTED_NORMAL')
    return obj


def rounded_box(name, w, d, h, mat, r=.7):
    points=[]
    for x,y,a in [(w/2-r,d/2-r,0),(-w/2+r,d/2-r,90),(-w/2+r,-d/2+r,180),(w/2-r,-d/2+r,270)]:
        points += [(x+r*math.cos(math.radians(a+90*i/12)),y+r*math.sin(math.radians(a+90*i/12))) for i in range(13)]
    return polygon_body(name,points,h,mat,.18)


def body_material(family):
    if MATERIAL_STYLE=='shared':
        return material('Shared charcoal satin','#383a3d',.36,.08)
    color,rough,coat={
        'xbox':('#242628',.18,.32),'playstation':('#353638',.23,.18),
        'dualsense':('#c5c9d2',.18,.55),'nintendo':('#424446',.43,.02),
        'generic':('#7d7d7d',.42,.03)}[family]
    return material(family+' plastic',color,rough,coat)


def shoulder(family, control, ink):
    """Projected finger-contact faces; hardware-informed, optically simplified."""
    trigger=control in ('lt','rt')
    # Left-hand silhouettes, mirrored for right. Xbox: swept, angular face;
    # PlayStation: broad rounded bumper and dropping trigger; Switch Pro:
    # shallow crescent bumper, broad ZL/ZR rear paddle.
    shapes={
        'xbox': [(-8,-2.6),(-8,1.2),(-7,2.6),(-3,3.4),(4,3.1),(8,1.9),(7.5,-1.9),(3,-2.6)],
        'playstation': [(-7.6,-2.4),(-8,0),(-7.5,2.1),(-5.5,3.2),(5.5,3.2),(7.8,2),(8,-2.5),(3,-3.2),(-4,-3.2)],
        'dualsense': [(-8,-2.5),(-8.3,.2),(-7.5,2.6),(-5,3.6),(4,3.5),(7,2.5),(8,-2.4),(3,-3.1),(-4,-3.1)],
        'nintendo': [(-8,-1.8),(-7.5,.7),(-5.5,2.6),(-2,3.4),(3,3.2),(7.8,1.4),(7,-1.1),(2,-1.9),(-4,-2.3)],
        'generic': [(-7.7,-2.6),(-8,2.3),(-7,3),(7,3),(8,2.3),(7.7,-2.6)]
    }
    triggers={
        'xbox': [(-4.6,5.3),(3.8,5.3),(5.2,3.4),(4.1,-4.1),(2.2,-5.7),(-1.8,-5.8),(-3.4,-4),(-4.8,1)],
        'playstation': [(-4.6,5.1),(4.6,5.1),(4.8,2.7),(3.9,-3.7),(2.4,-5.5),(-.8,-5.8),(-3.5,-4.8),(-4.5,-1)],
        'dualsense': [(-4.7,5.4),(4.6,5.4),(4.9,3.6),(4.3,-3.6),(2.5,-5.7),(-1.2,-5.8),(-3.7,-4.4),(-4.7,-.7)],
        'nintendo': [(-7,3.7),(6.3,3.7),(5.9,.3),(4.8,-3.6),(2.6,-4.6),(-3.8,-4.1),(-6.2,-2),(-7.4,.8)],
        'generic': [(-4.6,5),(4.6,5),(4.2,-4),(2.8,-5.2),(-2.8,-5.2),(-4.2,-4)]
    }
    points=(triggers if trigger else shapes)[family]
    if control in ('rb','rt'): points=[(-x,y) for x,y in reversed(points)]
    mat=(body_material(family) if MATERIAL_STYLE=='shared' else
         material(family+' shoulder satin', '#303236' if family in ('playstation','dualsense') else '#45484b', .34,.06))
    h=2.6 if trigger else 1.8
    obj=polygon_body('Trigger' if trigger else 'Bumper',points,h,mat,.7 if trigger else .55)
    # Present the trigger contact face at a shallow slope; its rolled lower
    # edge comes from the bevel. Keep printing on this same plane.
    def surface(x,y): return h-.10*y if trigger else h
    for vertex in obj.data.vertices:
        if vertex.co.z>0: vertex.co.z=surface(vertex.co.x/MM,vertex.co.y/MM)*MM
    side='L' if control in ('lb','lt') else 'R'
    label=(('Z'+side if trigger else side) if family=='nintendo' else
           side+('2' if trigger else '1') if family in ('playstation','dualsense') else
           side+('T' if trigger else 'B'))
    size=.86
    offset=.5 if trigger else .1
    if not trigger and family in ('xbox','nintendo','generic'):
        size=.58 if family=='nintendo' else .70
        offset=.30 if family=='nintendo' else .15
    for j,c in enumerate(label):
        legend(c,ink,surface,size,((j-(len(label)-1)/2)*3.6,offset),.35 if not trigger and family in ('xbox','nintendo','generic') else .38)
    return (17 if family=='nintendo' else 13.5,1.15 if family=='nintendo' else 1) if trigger else (19,1.6)


def icon(family, control, *, blank=False):
    body=body_material(family)
    white=material('Light print','#eef0ef',.42)
    dark=material('Dark print','#333941',.45)
    neutral_ink=dark if MATERIAL_STYLE=='family' and family in ('generic','dualsense') else white
    height,crown={'xbox':(1.8,.24),'playstation':(1.65,.09),'dualsense':(1.65,.09),
                  'nintendo':(2.35,.12),'generic':(1.8,.24)}[family]
    surface=lambda x,y: height+crown*(1-(x*x+y*y)/(4.557**2))
    if control in ('south','east','west','north'):
        if family=='generic':
            positions={'south':(0,-3.1),'east':(3.1,0),'west':(-3.1,0),'north':(0,3.1)}
            for k,(x,y) in positions.items():
                round_body('Position '+k,1.75,1.1,.16,white if k==control else body,x,y)
        else:
            label=NAMES[family][['south','east','west','north'].index(control)]
            round_body('Face button',4.9,height,crown,body)
            ink=neutral_ink if family=='dualsense' else material('Legend '+label,COLORS[label] if family in ('xbox','playstation') else '#e6e7e5',.48 if family=='playstation' else .38)
            if family=='playstation':
                ink.node_tree.nodes['Principled BSDF'].inputs['Specular IOR Level'].default_value=.15
            legend(label,ink,surface,size=1.18 if family=='xbox' else 1.06,width=.44 if family=='xbox' else .34)
        return 11.7, 1
    if control in ('lb','rb','lt','rt'):
        return shoulder(family,control,white)
    if control in ('start','back'):
        if family=='generic':
            label='START' if control=='start' else 'SELECT'
            rounded_box('Generic named center',18.8,6.8,1.5,body,2.5)
            for j,c in enumerate(label):
                legend(c,neutral_ink,lambda x,y:1.51,.65,((j-(len(label)-1)/2)*2.6,-.15),.27)
            return 21,2.1
        label={'xbox':('menu','view'),'playstation':('menu','create'),'dualsense':('menu','create'),'nintendo':('+','-'),'generic':('menu','view')}[family][control=='back']
        if family in ('playstation','dualsense'):
            rounded_box('Center pill',6.1,8.1,1.3,body,2.5)
            legend(label,neutral_ink,lambda x,y:1.31,.7,width=.32)
        else:
            round_body('Center button',3.8,1.5,.16,body)
            legend(label,white if family!='generic' else dark,lambda x,y:1.5+.16*(1-(x*x+y*y)/3.53**2),.85,width=.36)
        return 11.7, 1
    if control.startswith('dpad'):
        if family=='xbox':
            # Hybrid circular dish: raised outer rim and low center.
            n=64; rings=[(10,0),(10,1.3),(9.6,2),(8.8,2.1),(4,1.25),(.01,1.1)]
            verts=[(r*math.cos(i*2*math.pi/n),r*math.sin(i*2*math.pi/n),z) for r,z in rings for i in range(n)]
            faces=[(j*n+i,j*n+(i+1)%n,(j+1)*n+(i+1)%n,(j+1)*n+i) for j in range(len(rings)-1) for i in range(n)]
            mesh('Hybrid dish',verts,faces,body)
            points=[(-3,8.7),(3,8.7),(3,3),(8.7,3),(8.7,-3),(3,-3),(3,-8.7),(-3,-8.7),(-3,-3),(-8.7,-3),(-8.7,3),(-3,3)]
            polygon_body('Cross in dish',points,1.7,body,.35)
        elif family in ('playstation','dualsense'):
            for a in [0,90,180,270]:
                theta=math.radians(a)
                points=[(-2.6,8.7),(2.6,8.7),(3.2,5),(0,1.4),(-3.2,5)]
                points=[(x*math.cos(theta)-y*math.sin(theta),x*math.sin(theta)+y*math.cos(theta)) for x,y in points]
                polygon_body('Separate directional button',list(reversed(points)),1.8,body,.65)
        else:
            points=[(-3,9),(3,9),(3,3),(9,3),(9,-3),(3,-3),(3,-9),(-3,-9),(-3,-3),(-9,-3),(-9,3),(-3,3)]
            polygon_body('Cross',list(reversed(points)),1.8,body,.35)
        # A direction set is alternatives, not a simultaneous-button chord.
        directions={'dpad':[0],'dpad_ud':[0,180],'dpad_lr':[90,270],'dpad_left':[90],'dpad_down':[180],'dpad_right':[270],
                    'dpad_all':[0,90,180,270]}[control]
        for angle in ([] if blank else directions):
            a=math.radians(angle)
            points=[(-1.15,5.4),(0,7),(1.15,5.4)]
            points=[(x*math.cos(a)-y*math.sin(a),x*math.sin(a)+y*math.cos(a)) for x,y in points]
            def print_surface(x,y):
                if family=='xbox':
                    return max(1.7,1.25+.85*(math.hypot(x,y)-4)/4.8)+.015
                return 1.81
            stroke('Direction '+str(angle),points,neutral_ink,
                   print_surface,.7)
        return 23,1
    if control=='raw':
        round_body('Raw numbered button',4.9,height,crown,body)
        if not blank:
            legend('7',neutral_ink,surface,width=.4)
        return 11.7,1
    raise ValueError(control)


def rig(scene, exposure):
    scene.render.engine='CYCLES';scene.cycles.samples=64;scene.cycles.use_denoising=True
    scene.render.film_transparent=True
    scene.render.image_settings.file_format='PNG';scene.render.image_settings.color_mode='RGBA'
    scene.render.resolution_percentage=100
    scene.view_settings.view_transform='AgX';scene.view_settings.exposure=exposure
    scene.world=bpy.data.worlds.new('Faint neutral room');scene.world.use_nodes=True
    bg=scene.world.node_tree.nodes['Background'];bg.inputs[0].default_value=(.7,.7,.7,1);bg.inputs[1].default_value=.08
    light=bpy.data.lights.new('Monitor 60 × 34 cm','AREA');light.shape='RECTANGLE';light.size=.6;light.size_y=.34;light.energy=4.5
    panel=bpy.data.objects.new('Monitor B+D',light);scene.collection.objects.link(panel);panel.location=(0,.6,.27)
    angle=math.radians(15)
    panel.rotation_euler=Vector((0,-math.cos(angle),math.sin(angle))).to_track_quat('-Z','Y').to_euler()
    camera=bpy.data.objects.new('Near-overhead camera',bpy.data.cameras.new('Camera'));scene.collection.objects.link(camera);scene.camera=camera
    camera.data.type='ORTHO';camera.data.clip_start=.001;camera.data.clip_end=10
    camera.location=(0,-.038,.0862);camera.rotation_euler=(Vector((0,0,.0012))-camera.location).to_track_quat('-Z','Y').to_euler()
    scene.unit_settings.system='METRIC'
    return camera


def main():
    global MATERIAL_STYLE, OUT
    args=argparse.ArgumentParser();args.add_argument('--quick',action='store_true');args.add_argument('--only',default='');args.add_argument('--exposure',type=float,default=1.3)
    args.add_argument('--material',choices=['family','shared'],default='family')
    opts=args.parse_args(sys.argv[sys.argv.index('--')+1:] if '--' in sys.argv else [])
    MATERIAL_STYLE=opts.material
    if MATERIAL_STYLE=='shared': OUT=OUT/'shared'
    OUT.mkdir(parents=True,exist_ok=True)
    bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
    scene=bpy.context.scene;scene.name='Controller hints · B+D material study'
    camera=rig(scene,opts.exposure)
    records=[];collections=[]
    controls=['south','east','west','north','lb','rb','lt','rt','start','back','dpad','dpad_ud','dpad_lr','dpad_all','dpad_left','dpad_down','dpad_right']
    for family in FAMILIES:
        for control in (['south'] if opts.quick else controls+(['raw'] if family=='generic' else [])):
            before=set(scene.objects)
            width,aspect=icon(family,control)
            objects=set(scene.objects)-before
            collection=bpy.data.collections.new(f'{family} · {control}');scene.collection.children.link(collection)
            for obj in objects:
                for c in list(obj.users_collection):c.objects.unlink(obj)
                collection.objects.link(obj)
            for c in collections:c.hide_render=True;c.hide_viewport=True
            collections.append(collection)
            camera.data.ortho_scale=width*MM
            scene.render.resolution_x=round(384*aspect);scene.render.resolution_y=384
            scene.render.filepath=str(OUT/f'{family}-{control}.png')
            if not opts.only or family+'-'+control in opts.only.split(','):
                bpy.ops.render.render(write_still=True)
            records.append({'family':family,'control':control,'file':f'{family}-{control}.png','aspect':aspect})
    # The editable scene opens as a laid-out collection of all specimens.
    # Individual PNGs above always use the same centered light and camera.
    for record,collection in zip(records,collections):
        row=FAMILIES.index(record['family'])
        col=(controls+['raw']).index(record['control'])
        collection.hide_render=False;collection.hide_viewport=False
        for obj in collection.objects:
            obj.location+=Vector(((col-7)*.024,(2-row)*.028,0))
    camera.data.ortho_scale=.38
    scene.render.resolution_x=2000;scene.render.resolution_y=1400
    scene.render.filepath=str(OUT/'editable-scene.png')
    bpy.ops.wm.save_as_mainfile(filepath=str(OUT/'controller-buttons.blend'))
    (OUT/'study.json').write_text(json.dumps({'icons':records,'material':MATERIAL_STYLE,'monitor':{'size_m':[.6,.34],'center_m':[0,.6,.27],'tilt_degrees':15,'watts':4.5,'world_strength':.08},'exposure_stops':opts.exposure,'note':'Original visual approximations. UI exposure lift; not measured hardware or calibrated luminance.'},indent=2))

if __name__=='__main__': main()
