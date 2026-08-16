struct Instance {
    mvp: mat4x4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0)
var<storage, read> instances: array<Instance>;

@group(0) @binding(1)
var diffuse_texture: texture_2d<f32>;

@group(0) @binding(2)
var diffuse_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let instance = instances[instance_index];
    var output: VertexOutput;
    output.position = instance.mvp * vec4<f32>(position, 1.0);
    output.color = instance.color;
    output.uv = uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(diffuse_texture, diffuse_sampler, input.uv) * input.color;
}
