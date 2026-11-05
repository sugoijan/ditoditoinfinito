// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
// Screen-space silhouette backing. All inputs/outputs are straight-alpha sRGB
// in RGBA8Unorm; distance intermediates are ordinary unorm data, not colors.
struct Settings { color: vec4<f32>, shape: vec4<f32> }
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var mask: texture_2d<f32>;
@group(0) @binding(2) var filtering: sampler;
@group(0) @binding(3) var<uniform> settings: Settings;
struct Out { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> Out {
 let p = vec2(f32((i << 1u) & 2u), f32(i & 2u));
 var o: Out; o.position = vec4(p * vec2(2., -2.) + vec2(-1., 1.), 0., 1.); o.uv = p; return o;
}
@fragment fn fs_seed(o: Out) -> @location(0) vec4<f32> {
 let step = 0.25 / settings.shape.yz;
 var coverage = 0.;
 for (var i = 0u; i < 4u; i++) {
  let offset = vec2(f32(i % 2u) * 2. - 1., f32(i / 2u) * 2. - 1.) * step;
  coverage = max(coverage, textureSampleLevel(source, filtering, o.uv + offset, 0.).a);
 }
 return vec4(coverage, 0., 0., 1.);
}
@fragment fn fs_horizontal(o: Out) -> @location(0) vec4<f32> {
 let size = vec2<i32>(textureDimensions(mask)); let at = vec2<i32>(o.position.xy);
 let radius = i32(ceil(settings.shape.x + 1.)); var distance = 33.;
 for (var x = -radius; x <= radius; x++) {
  let p = at + vec2(x, 0);
  if p.x >= 0 && p.x < size.x {
   if textureLoad(mask, p, 0).r > 0.01 { distance = min(distance, abs(f32(x))); }
  }
 }
 return vec4(distance / 33., 0., 0., 1.);
}
fn decode(v: vec3<f32>) -> vec3<f32> {
 return select(v / 12.92, pow((v + vec3(0.055)) / 1.055, vec3(2.4)), v > vec3(0.04045));
}
fn encode(v: vec3<f32>) -> vec3<f32> {
 return select(v * 12.92, 1.055 * pow(max(v, vec3(0.)), vec3(1. / 2.4)) - vec3(0.055), v > vec3(0.0031308));
}
@fragment fn fs_vertical(o: Out) -> @location(0) vec4<f32> {
 let size = vec2<i32>(textureDimensions(mask)); let at = vec2<i32>(o.position.xy);
 let radius = i32(ceil(settings.shape.x + 1.)); var distance = 33.;
 for (var y = -radius; y <= radius; y++) {
  let p = at + vec2(0, y);
  if p.y >= 0 && p.y < size.y {
   let dx = textureLoad(mask, p, 0).r * 33.;
   distance = min(distance, length(vec2(dx, f32(y))));
  }
 }
 let coverage = 1. - smoothstep(settings.shape.x - 0.5, settings.shape.x + 0.5, distance);
 return vec4(coverage, 0., 0., 1.);
}
@fragment fn fs_finish(o: Out) -> @location(0) vec4<f32> {
 let original = textureLoad(source, vec2<i32>(o.position.xy), 0);
 if original.a >= 1. { return original; }
 let backing = textureSampleLevel(mask, filtering, o.uv, 0.).r * settings.color.a;
 let behind = backing * (1. - original.a); let alpha = original.a + behind;
 if alpha <= 0. { return vec4(0.); }
 return vec4(encode((decode(original.rgb) * original.a + decode(settings.color.rgb) * behind) / alpha), alpha);
}
