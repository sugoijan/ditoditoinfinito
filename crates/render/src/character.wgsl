struct Camera {
 vp: mat4x4<f32>, light: vec4<f32>, tips: array<vec4<f32>,2>,
 eye: vec4<f32>, tint: vec4<f32>, lighting: vec4<f32>, surface: vec4<f32>,
 mat_color: vec4<f32>, accent: vec4<f32>, tiles: array<vec4<f32>,3>, tile_heights: array<vec4<f32>,3>,
 tile_colors: array<vec4<f32>,9>, effects: vec4<f32>,
 capsules_a: array<vec4<f32>,6>, capsules_b: array<vec4<f32>,6>,
}
@group(0) @binding(0) var<uniform> camera: Camera;
struct Material { color: vec4<f32>, role: vec4<f32> }
@group(1) @binding(0) var<uniform> material: Material;
struct Out { @builtin(position) clip: vec4<f32>, @location(0) world: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) detail: vec4<f32> }
@vertex fn vs_main(@location(0) position:vec3<f32>, @location(1) normal:vec3<f32>, @location(2) detail:vec4<f32>)->Out {
 var o:Out; o.clip=camera.vp*vec4(position,1.);o.world=position;o.normal=normal;o.detail=detail;return o;
}
fn tile_height(index:u32)->f32 {
 if camera.surface.w>0.5 {return select(0.001,0.,index==u32(camera.surface.w)-1u);}
 return camera.tile_heights[index/4u][index%4u];
}
@vertex fn vs_tiles(@location(0) position:vec3<f32>, @location(1) normal:vec3<f32>, @location(2) detail:vec4<f32>)->Out {
 var p=position;p.y=position.y*tile_height(u32(detail.x));
 var o:Out;o.clip=camera.vp*vec4(p,1.);o.world=p;o.normal=normal;o.detail=detail;return o;
}
fn encode(c:vec3<f32>)->vec3<f32> {return select(1.055*pow(max(c,vec3(0.)),vec3(1./2.4))-0.055,c*12.92,c<=vec3(0.0031308));}
fn display(c:vec3<f32>)->vec3<f32> {let v=max(c,vec3(0.));return encode(v/(vec3(1.)+v));}
fn safe_normal(v:vec3<f32>)->vec3<f32> {return v*inverseSqrt(max(dot(v,v),1e-12));}
fn tile_hit(index:u32)->f32 {
 var hit=camera.tiles[index/4u][index%4u];
 if camera.surface.w>0.5 {hit=select(0.,1.,index==u32(camera.surface.w)-1u);}
 return hit*camera.surface.z;
}
// Six inset finger capsules approximate visibility of the broad key light.
// They are render proxies, never collision geometry or changes to the skeleton.
fn visibility(p:vec3<f32>,l:vec3<f32>)->f32 {
 var result=1.;
 for(var i=0u;i<6u;i++) {
  let a=camera.capsules_a[i];let b=camera.capsules_b[i];let e=b.xyz-a.xyz;
  let w=p-a.xyz;let ed=dot(e,l);let ee=dot(e,e);
  let s=clamp((dot(w,e)-dot(w,l)*ed)/max(ee-ed*ed,1e-10),0.,1.);
  let q=a.xyz+e*s-p;let along=dot(q,l);
  let distance=length(q-l*max(along,0.));let blur=0.001+max(along,0.)*0.14;
  let blocked=(1.-smoothstep(a.w-blur,a.w+blur,distance))*select(0.,0.8,along>0.001&&a.w>0.);
  result*=1.-blocked;
 }
 return result;
}
// GGX dielectric surface: rough highlights, Fresnel and masking, with broad
// warm/cool studio sources. This is raster shading, not path tracing.
fn surface_light(n:vec3<f32>,v:vec3<f32>,l:vec3<f32>,base:vec3<f32>,surface:vec2<f32>)->vec3<f32> {
 let nl=max(dot(n,l),0.);let nv=max(dot(n,v),0.001);let h=safe_normal(l+v);
 let nh=max(dot(n,h),0.);let vh=max(dot(v,h),0.);
 let rough=clamp(surface.x,0.12,1.);let a=rough*rough;let a2=a*a;
 let d=a2/(3.14159265*pow(nh*nh*(a2-1.)+1.,2.));
 let k=pow(rough+1.,2.)/8.;let g=(nl/(nl*(1.-k)+k))*(nv/(nv*(1.-k)+k));
 let f0=0.015+surface.y*0.075;
 let fresnel=f0+(1.-f0)*pow(1.-vh,5.);
 let spec=d*g*fresnel/max(4.*nv*nl,0.001);
 return (base*(1.-fresnel)/3.14159265+vec3(spec))*nl;
}
fn mat_light(p:vec3<f32>,n:vec3<f32>,v:vec3<f32>,base:vec3<f32>,surface:vec2<f32>)->vec3<f32> {
 var result=vec3(0.);
 for(var i=0u;i<9u;i++) {
  let hit=tile_hit(i);
  if hit>0.001 {
   let center=vec2(f32(i%3u)-1.,f32(i/3u)-1.)*0.024;
   // Four samples over the luminous area avoid a point-light hotspot.
   for(var j=0u;j<4u;j++) {
    let offset=vec2(f32(j%2u)*2.-1.,f32(j/2u)*2.-1.)*0.006;
    let delta=vec3(center.x+offset.x,tile_height(i)+0.0001,center.y+offset.y)-p;
    let d2=dot(delta,delta);let l=safe_normal(delta);
    let attenuation=0.00012/(0.00012+d2)*exp(-max(p.y,0.)/0.035);
    result+=surface_light(n,v,l,base,surface)*camera.tile_colors[i].rgb*hit*attenuation;
   }
  }
 }
 return result*camera.effects.y*3.5;
}
@fragment fn fs_hand(o:Out,@builtin(front_facing) front:bool)->@location(0) vec4<f32> {
 let n=normalize(o.normal)*select(-1.,1.,front);let l=normalize(camera.light.xyz);
 if camera.light.w<0.5 {return vec4(encode(material.color.rgb*(0.38+0.62*max(dot(n,l),0.))),1.);}
 if material.role.x>1.5 && material.role.x<2.5 {
  let edge=pow(smoothstep(0.45,1.,o.detail.w),2.);
  return vec4(encode(mix(vec3(0.0003),camera.tint.rgb*0.035,edge)),1.);
 }
 let view=safe_normal(camera.eye.xyz-o.world);
 let nail=(1.-smoothstep(0.90,1.02,length(o.detail.xy)))*smoothstep(0.25,0.7,o.detail.z);
 let nail_amount=max(nail,select(0.,1.,material.role.x>3.5));
 let surface=mix(camera.surface.xy,vec2(0.20,0.85),nail_amount);
 var base=mix(material.color.rgb,camera.tint.rgb,0.9)*select(0.25,1.,max(material.color.r,max(material.color.g,material.color.b))>0.2);
 base=mix(base,mix(camera.tint.rgb,vec3(0.94,0.85,1.),0.40),nail_amount);
 if material.role.x>0.5 && material.role.x<1.5 {base=camera.tint.rgb*0.35;}
 if camera.effects.x<0.5 {
  let half_vector=safe_normal(l+view);let rough=clamp(surface.x,0.08,1.);
  let spec=pow(max(dot(n,half_vector),0.),mix(160.,5.,rough))*surface.y;
  let rim=pow(1.-max(dot(n,view),0.),3.)*camera.lighting.z;
  let diffuse=camera.lighting.w+camera.lighting.x*max(dot(n,l),0.)+camera.lighting.y*max(dot(n,normalize(vec3(-l.x,0.3,-l.z))),0.);
  return vec4(encode(base*diffuse+vec3(spec*camera.lighting.x)+vec3(0.62,0.8,1.)*rim*0.3),1.);
 }
 let fill=normalize(vec3(-l.x,0.35,-l.z));let rim=normalize(vec3(0.25,0.6,-1.));
 let hemi=mix(vec3(0.12,0.1,0.09),vec3(0.6,0.7,0.85),n.y*0.5+0.5);
 var lit=base*hemi*camera.lighting.w;
 lit+=surface_light(n,view,l,base,surface)*vec3(1.,0.92,0.81)*camera.lighting.x*3.5*visibility(o.world+n*0.0008,l);
 lit+=surface_light(n,view,fill,base,surface)*vec3(0.69,0.81,1.)*camera.lighting.y*2.5;
 lit+=surface_light(n,view,rim,base,surface)*vec3(0.76,0.86,1.)*camera.lighting.z*2.;
 lit+=mat_light(o.world,n,view,base,surface);
 return vec4(display(lit),1.);
}
@fragment fn fs_floor(o:Out)->@location(0) vec4<f32> {
 if camera.light.w<0.5 {
  let uv=o.world.xz/0.025;let edge=abs(fract(uv+0.5)-0.5)/max(fwidth(uv),vec2(0.001));let grid=1.-clamp(min(edge.x,edge.y),0.,1.);var shade=0.;
  for(var i=0;i<2;i++){let d=o.world.xz-camera.tips[i].xy;shade+=0.15*exp(-dot(d,d)/0.00012)*exp(-max(camera.tips[i].z,0.)/0.012);}
  return vec4(vec3(0.89-grid*0.12-shade),1.);
 }
 let uv=o.world.xz/0.024;let cell=floor(uv+0.5);let local=abs(uv-cell);
 let aa=max(max(fwidth(uv.x),fwidth(uv.y)),0.001);
 let corner=length(max(local-vec2(0.3625),vec2(0.)))+min(max(local.x-0.3625,local.y-0.3625),0.)-0.1166667;
 let coverage=select(1.,1.-smoothstep(-aa*0.75,0.,corner),o.normal.y>0.5);
 if max(abs(uv.x),abs(uv.y))>=1.5||coverage<=0.001 {discard;}
 let index=u32((cell.y+1.)*3.+cell.x+1.);let hit=tile_hit(index);
 // Flat satin tiles: no decorative bevel or corner glints. Smooth coverage
 // keeps rounded boundaries from leaving isolated hard pixels.
 var shadow=0.;
 for(var i=0;i<2;i++){let d=o.world.xz-camera.tips[i].xy;shadow+=0.25*exp(-dot(d,d)/0.00006)*exp(-max(camera.tips[i].z,0.)/0.008);}
 let key=normalize(camera.light.xyz);
 let visibility_key=select(1.,visibility(o.world+vec3(0.,0.0005,0.),key),camera.effects.x>0.5);
 let light=0.45+camera.lighting.x*0.55*visibility_key+camera.lighting.y*0.25+camera.lighting.w;
 let side=select(0.42,1.,o.normal.y>0.5);
 let paint=(camera.mat_color.rgb*light*(1.-shadow)+camera.tile_colors[index].rgb*hit*1.8)*side;
 let rgb=display(paint);
 if camera.effects.z>0.5 {return vec4(rgb,coverage);}
 return vec4(mix(vec3(0.91,0.9,0.88),rgb,coverage),1.);
}
