"""Original SVG cap bodies matching the A1/A2 B+D Blender references.

Paths and gradients only; printing stays live in the UI. Width and travel
are parameters. The current sprite bridge uses four standard widths plus
Space; generated standalone bodies can use arbitrary widths.
"""
import math
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'target/scratch/keycaps/svg'
ASSETS=ROOT/'assets/ui'
SCALE=32/23
COS=85/math.hypot(85,38)
SIN=38/math.hypot(85,38)


def ring(a,b,r):
    points=[]
    corners=[(a-r,b-r,0),(-a+r,b-r,90),(-a+r,-b+r,180),(a-r,-b+r,270)]
    for n,(x,y,start) in enumerate(corners):
        for i in range(9):
            t=math.radians(start+90*i/8)
            points.append((x+r*math.cos(t),y+r*math.sin(t)))
        nx,ny,angle=corners[(n+1)%4]
        end=points[-1]; begin=(nx+r*math.cos(math.radians(angle)),ny+r*math.sin(math.radians(angle)))
        for i in range(1,25):
            t=i/25
            points.append((end[0]*(1-t)+begin[0]*t,end[1]*(1-t)+begin[1]*t))
    return points


def path(points):
    return 'M'+' L'.join(f'{x:.4f},{y:.4f}' for x,y in points)+' Z'


def body(width, finish='a1', space=False, ident='cap', press=0):
    assert finish in ('a1','a2','light','gray','deep-gray') and width>=24
    units=2.25+(width-64)/(19*SCALE) if space else 1+(width-32)/24
    a=6.8+(units-1)*9.5; b=7.25
    def z(x,y):
        return 9+1.5*(1-(y/b)**2) if space else 8+math.tan(math.radians(4))*y+(x/a)**2
    points=[(width/2+x*SCALE,16-(COS*(y+.5)+SIN*(z(x,y)-5))*SCALE) for x,y in ring(a,b,1)]
    face=path(points)
    left=width/2-a*SCALE; right=width-left
    bottom=max(1.4,width/2-(9+(units-1)*9.5)*SCALE)
    top=min(y for _,y in points)
    front=16-(COS*(-b+.5)+SIN*(z(a,-b)-5))*SCALE
    semi=finish=='a2'
    light=finish in ('light','gray','deep-gray')
    wall_top='#29292a' if semi else '#202021'
    center='#090a0c' if semi else '#111213'
    edge='#111214' if semi else '#191a1b'
    rim='#a0a1a2' if semi else '#77797b'
    wall_mid,wall_bottom,front_top,front_bottom='#161617','#070708','#030304','#070708'
    outer_stroke,face_stroke='#161617','#28292b'
    space_stops=([(0,'#959595'),(.04,'#8f8f8f'),(.09,'#4f4f50'),(.14,'#2d2d2e'),(.20,'#1b1c1d'),(.27,'#121315'),(.65,'#060607'),(1,'#040405')] if semi else
                 [(0,'#6e6f6f'),(.04,'#696a6a'),(.09,'#535354'),(.15,'#444545'),(.22,'#343435'),(.28,'#272728'),(.40,'#111213'),(.72,'#070809'),(1,'#040404')])
    if light:
        # Same satin geometry/highlight placement. Lifted diffuse tones keep
        # dark 10 px printing readable under the UI's fixed dark background.
        # Equal RGB channels keep both gray alternatives neutral.
        wall_top,wall_mid,wall_bottom='#929292','#747474','#515151'
        center,edge,rim='#a2a2a2','#ababab','#dedede'
        front_top,front_bottom='#454545','#565656'
        outer_stroke,face_stroke='#626262','#bcbcbc'
        space_stops=[(0,'#cdcdcd'),(.09,'#c1c1c1'),(.22,'#b3b3b3'),(.40,'#a2a2a2'),(.72,'#919191'),(1,'#838383')]
        if finish in ('gray','deep-gray'):
            wall_top,wall_mid,wall_bottom='#787878','#606060','#454545'
            center,edge,rim='#858585','#8e8e8e','#c1c1c1'
            front_top,front_bottom='#393939','#484848'
            outer_stroke,face_stroke='#515151','#9e9e9e'
            space_stops=[(0,'#b0b0b0'),(.09,'#a4a4a4'),(.22,'#969696'),(.40,'#858585'),(.72,'#797979'),(1,'#6e6e6e')]
        if finish == 'deep-gray':
            wall_top,wall_mid,wall_bottom='#707070','#595959','#404040'
            center,edge,rim='#7d7d7d','#868686','#b9b9b9'
            front_top,front_bottom='#353535','#434343'
            outer_stroke,face_stroke='#4b4b4b','#969696'
            space_stops=[(0,'#a8a8a8'),(.09,'#9c9c9c'),(.22,'#8e8e8e'),(.40,'#7d7d7d'),(.72,'#717171'),(1,'#666666')]
    stops=''.join(f'<stop offset="{t}" stop-color="{c}"/>' for t,c in space_stops)
    face_gradient=(f'<linearGradient id="{ident}-face" gradientUnits="userSpaceOnUse" x1="0" y1="{top}" x2="0" y2="{front}">{stops}</linearGradient>' if space else
       f'<linearGradient id="{ident}-face"><stop stop-color="{edge}"/><stop offset=".18" stop-color="{center}"/><stop offset=".65" stop-color="{center}"/><stop offset="1" stop-color="{edge}"/></linearGradient>')
    outer=f'M{left},{top+.2} Q{width/2},{top+.9} {right},{top+.2} L{width-bottom},8 V29.3 Q{width-bottom},30.2 {width-bottom-1},30.2 H{bottom+1} Q{bottom},30.2 {bottom},29.3 V8 Z'
    front_path=f'M{left},{front-.2} Q{width/2},{front+1.05} {right},{front-.2} L{width-bottom-.1},29.4 Q{width-bottom-.1},30.1 {width-bottom-1},30.1 H{bottom+1} Q{bottom+.1},30.1 {bottom+.1},29.4 Z'
    return f'''<g transform="translate(0,{2.271*press:.4f})">
<defs>
 <linearGradient id="{ident}-wall" x2="0" y2="1"><stop stop-color="{wall_top}"/><stop offset=".5" stop-color="{wall_mid}"/><stop offset="1" stop-color="{wall_bottom}"/></linearGradient>
 <linearGradient id="{ident}-front" x2="0" y2="1"><stop stop-color="{front_top}"/><stop offset="1" stop-color="{front_bottom}"/></linearGradient>
 {face_gradient}
 <linearGradient id="{ident}-rim" gradientUnits="userSpaceOnUse" x1="0" y1="{top}" x2="0" y2="{top+3}"><stop stop-color="{rim}"/><stop offset=".18" stop-color="{rim}" stop-opacity=".25"/><stop offset="1" stop-color="{rim}" stop-opacity="0"/></linearGradient>
</defs>
<path d="{outer}" fill="url(#{ident}-wall)" stroke="{outer_stroke}" stroke-width=".12"/>
<path d="{front_path}" fill="url(#{ident}-front)"/>
<path d="{face}" fill="url(#{ident}-face)" stroke="{face_stroke}" stroke-opacity=".45" stroke-width=".12"/>
<path d="{face}" fill="none" stroke="url(#{ident}-rim)" stroke-width="{'.16' if semi else '.24'}" stroke-linejoin="round"/>
</g>'''


def svg(width,height,content,view=None):
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {view or str(width)+" "+str(height)}">{content}</svg>\n'


def main():
    OUT.mkdir(parents=True,exist_ok=True); ASSETS.mkdir(exist_ok=True)
    for finish in ['a1','a2','light','gray','deep-gray']:
        pieces=[]
        for row,(width,space) in enumerate([(32,False),(44,False),(56,False),(68,False),(64,True)]):
            cap=body(width,finish,space,f'{finish}-{row}')
            pieces.append(f'<g transform="translate(0,{32*row})">{cap}</g>')
            name='space' if space else f'normal-{width}'
            (OUT/f'{finish}-{name}.svg').write_text(svg(width*8,256,cap,f'{width} 32'))
            (OUT/f'{finish}-{name}-pressed.svg').write_text(svg(width*8,280,body(width,finish,space,f'{finish}-p{row}',1),f'{width} 35'))
        (ASSETS/f'keycaps-{finish}.svg').write_text(svg(768,1280,''.join(pieces),'96 160'))

if __name__=='__main__': main()
