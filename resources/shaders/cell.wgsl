// Cell rendering shader for clux terminal multiplexer.
// Renders terminal cells as instanced quads with either solid color (background)
// or texture-sampled glyphs (foreground).

struct Uniforms {
    screen_width: f32,
    screen_height: f32,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var atlas_texture: texture_2d<f32>;
@group(1) @binding(1)
var atlas_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
}

struct InstanceInput {
    @location(1) pos: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) uv_rect: vec4<f32>,
    @location(5) mode: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mode: f32,
}

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    // Scale vertex (0..1) to instance size in pixels, then offset
    let pixel_pos = instance.pos + vertex.position * instance.size;

    // Convert pixel coordinates to NDC: x -> [-1, 1], y -> [1, -1] (top-left origin)
    let ndc_x = (pixel_pos.x / uniforms.screen_width) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_pos.y / uniforms.screen_height) * 2.0;

    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = instance.color;

    // Interpolate UV from the atlas rect
    out.uv = instance.uv_rect.xy + vertex.position * instance.uv_rect.zw;

    out.mode = instance.mode;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.mode < 0.5 {
        // Background mode: solid color fill
        return in.color;
    } else {
        // Foreground/glyph mode: sample atlas texture, tint with color
        let alpha = textureSample(atlas_texture, atlas_sampler, in.uv).r;
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
    }
}
