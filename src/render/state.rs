use crate::StaticRanges;
use geo::{Coord, Rect};
use std::ops::Range;
use wgpu::util::DeviceExt;
use wgpu::Buffer;
use winit::event::WindowEvent;
use winit::window::Window;

// https://sotrh.github.io/learn-wgpu/beginner/tutorial2-surface/#state-new
pub struct State<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    window: &'a Window,
    clear_color: wgpu::Color,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    static_ranges: StaticRanges,
    shape_buffer: wgpu::Buffer,
    shape_pipeline: wgpu::RenderPipeline,
    num_shape_vertices: Vec<Range<u32>>,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    // index_buffer: wgpu::Buffer,
    // num_indices: u32,
    // poly_slices: Vec<u32>,
}

impl<'a> State<'a> {
    // https://sotrh.github.io/learn-wgpu/beginner/tutorial2-surface/#state-new
    pub async fn new(
        window: &'a Window,
        camera: CameraUniform,
        static_ranges: StaticRanges,
        static_verts: &[Vertex],
        shape_vertices: &[Vertex],
        shape_ranges: Vec<Range<u32>>,
    ) -> State<'a> {
        let size = window.inner_size();

        // The instance is a handle to our GPU
        // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::GL,
            ..Default::default()
        });

        let surface = instance.create_surface(window).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    // WebGL doesn't support all of wgpu's features, so if
                    // we're building for the web, we'll have to disable some.
                    required_limits: if cfg!(target_arch = "wasm32") {
                        wgpu::Limits::downlevel_webgl2_defaults()
                    } else {
                        wgpu::Limits::default()
                    },
                    label: None,
                    memory_hints: Default::default(),
                },
                None, // Trace path
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        // Shader code in this tutorial assumes an sRGB surface texture. Using a different
        // one will result in all the colors coming out darker. If you want to support non
        // sRGB surfaces, you'll need to account for that when drawing to the frame.
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let camera_buffer = camera.into_buffer(&device);

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("camera_bind_group"),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",     // 1.
                buffers: &[Vertex::desc()], // 2.
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                // 3.
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    // 4.
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList, // 1.
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // 2.
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None, // 1.
            multisample: wgpu::MultisampleState {
                count: 1,                         // 2.
                mask: !0,                         // 3.
                alpha_to_coverage_enabled: false, // 4.
            },
            multiview: None, // 5.
            cache: None,     // 6.
        });
        let shape_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shape Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",     // 1.
                buffers: &[Vertex::desc()], // 2.
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                // 3.
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    // 4.
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineStrip, // 1.
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // 2.
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None, // 1.
            multisample: wgpu::MultisampleState {
                count: 1,                         // 2.
                mask: !0,                         // 3.
                alpha_to_coverage_enabled: false, // 4.
            },
            multiview: None, // 5.
            cache: None,     // 6.
        });

        // let bb: Vec<Vertex> = boros[..]
        //     .iter()
        //     .flat_map(|boro| {
        //         boro.coords_iter().map(|c| Vertex {
        //             position: [c.x as f32, c.y as f32, 0.0],
        //             color: [1.0, 1.0, 1.0],
        //         })
        //     })
        //     .collect();

        // let poly_slices: Vec<u32> = boros[..].iter().flat_map(|boro| {
        //     boro.iter().map(|f| f.coords_count() as u32)
        // }).collect();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&static_verts[..]),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let shape_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&shape_vertices[..]),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        //     label: Some("Index Buffer"),
        //     contents: bytemuck::cast_slice(&bb[..]),
        //     usage: wgpu::BufferUsages::INDEX,
        // });

        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            clear_color: wgpu::Color {
                r: 0.05,
                g: 0.05,
                b: 0.05,
                a: 1.0,
            },
            render_pipeline,
            vertex_buffer,
            static_ranges,
            shape_buffer,
            shape_pipeline,
            num_shape_vertices: shape_ranges,
            camera_buffer,
            camera_bind_group,
            // index_buffer,
            // num_indices: INDICES.len() as u32,
            // poly_slices,
        }
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    // impl State
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        // match event {
        //     WindowEvent::CursorMoved {
        //         device_id: _,
        //         position,
        //     } => {
        //         self.window().request_redraw();
        //         println!("{} {}", position.x, position.y);
        //         self.clear_color = wgpu::Color {
        //             r: position.x.abs() / self.size.width as f64,
        //             g: position.y.abs() / self.size.height as f64,
        //             ..self.clear_color
        //         };
        //         true
        //     }
        //     _ => false,
        // }
        false
    }

    pub fn update(&mut self) {}

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.draw(self.static_ranges.boros.clone(), 0..1);

            render_pass.set_pipeline(&self.shape_pipeline);
            render_pass.set_vertex_buffer(0, self.shape_buffer.slice(..));

            for range in &self.num_shape_vertices {
                render_pass.draw(range.clone(), 0..1);
            }

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.draw(self.static_ranges.stops.clone(), 0..1);

            // let mut last_idx = 0;
            // println!("RANGE {:?} TOTAL: {:?}", self.poly_slices, self.num_vertices);
            // for idx in &self.poly_slices {
            //     let next_idx = last_idx + idx;
            //     render_pass.draw(last_idx..next_idx, 0..1);
            //     last_idx = next_idx;
            // }

            // render_pass.draw(0..self.num_vertices, 0..1);
            // render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            // render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    width: f32,
    height: f32,
    min: [f32; 2],
}

impl CameraUniform {
    pub fn new(rect: Rect) -> Self {
        Self {
            width: rect.width() as f32,
            height: rect.height() as f32,
            min: [rect.min().x as f32, rect.min().y as f32],
        }
    }

    pub fn into_buffer(self, device: &wgpu::Device) -> Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[self]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub normal: [f32; 3],
    pub miter: f32,
}

// lib.rs
impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32];

    pub fn new(coord: Coord, color: [f32; 3]) -> Self {
        Self {
            color,
            ..Self::from(coord)
        }
    }

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

// impl FromIterator<&Coord> for Vec<Vertex> {
//     fn from_iter<T: IntoIterator<Item = &Coord>>(iter: T) -> Self {
//         let mut v: Vec<Vertex> = Vec::new();
//         for c in iter {
//             v.push(Vertex::from(c));
//         }
//         v
//     }
// }

impl From<Coord> for Vertex {
    fn from(value: Coord) -> Self {
        Vertex {
            position: [value.x as f32, value.y as f32, 0.0],
            color: [0.3, 0.3, 0.3],
            normal: [0.0, 0.0, 0.0],
            miter: 0.,
        }
    }
}

impl From<&Coord> for Vertex {
    fn from(value: &Coord) -> Self {
        Vertex {
            position: [value.x as f32, value.y as f32, 0.0],
            color: [1.0, 1.0, 1.0],
            normal: [0.0, 0.0, 0.0],
            miter: 0.,
        }
    }
}

// const VERTICES: &[Vertex] = &[
//     Vertex {
//         position: [-0.0868241, 0.49240386, 0.0],
//         color: [1.0, 1.0, 1.0],
//     }, // A
//     Vertex {
//         position: [-0.49513406, 0.06958647, 0.0],
//         color: [1.0, 1.0, 1.0],
//     }, // B
//     Vertex {
//         position: [-0.21918549, -0.44939706, 0.0],
//         color: [1.0, 1.0, 1.0],
//     }, // C
//     Vertex {
//         position: [0.35966998, -0.3473291, 0.0],
//         color: [1.0, 1.0, 1.0],
//     }, // D
//     Vertex {
//         position: [0.44147372, 0.2347359, 0.0],
//         color: [1.0, 1.0, 1.0],
//     }, // E
// ];

// const INDICES: &[u16] = &[0, 1, 2, 3];
