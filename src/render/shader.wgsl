struct CameraUniform {
  width: f32,
  height: f32,
  min: vec2<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) miter: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.color = model.color;
    let xy = model.position + vec3(model.normal * 20.0/2.0);
    let x = 2.0 * (xy[1] - camera.min[0]) / camera.height - 1.0;
    let y = 2.0 * (xy[0] - camera.min[1]) / camera.width - 1.0;
    out.clip_position = vec4<f32>(x, y, model.position[2], 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}

