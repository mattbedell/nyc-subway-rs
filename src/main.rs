use std::sync::mpsc::{channel, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};
use lyon::geom::point;
use lyon::path::Path;
use lyon::tessellation::{BuffersBuilder, StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers};
use tokio;

use lyon;

use winit::{
    dpi::PhysicalSize,
    event::*,
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::WindowBuilder,
};

use anyhow::Result;
use env_logger;
use geo::{
    BoundingRect, Coord, CoordsIter, MultiPolygon,
    Point, Rect, Translate, TriangulateEarcut,
};

use entities::CollectableEntity;
use render::{CameraUniform, Vertex};
use util::static_data::{self, BOROUGH_BOUNDARIES_STATIC, COASTLINE_STATIC, GTFS_STATIC};

mod entities;
mod proto;
mod render;
mod util;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let xdg = util::get_xdg()?;
    if static_data::shoud_fetch(GTFS_STATIC) {
        let gtfs_zip = static_data::fetch(GTFS_STATIC, None).await?;
        static_data::unzip(gtfs_zip).await?;
    }

    if static_data::shoud_fetch(COASTLINE_STATIC) {
        static_data::fetch(COASTLINE_STATIC, Some(xdg.get_data_home())).await?;
    }

    if static_data::shoud_fetch(BOROUGH_BOUNDARIES_STATIC) {
        static_data::fetch(BOROUGH_BOUNDARIES_STATIC, Some(xdg.get_data_home())).await?;
    }

    let mut boros = entities::Boro::load_collection()?;
    let mut shapes = entities::ShapeSeq::load_collection()?;

    let o_rect = boros.bounding_rect().unwrap();
    let origin: Point<f32> = o_rect.center().into();

    boros.translate_origin_from(&origin);
    shapes.translate_origin_from(&origin);

    let boros_rect = boros.bounding_rect().unwrap();
    let v_scale = 1.;
    let mut viewport = Rect::new(
        Coord::zero(),
        Coord {
            x: boros_rect.height().max(boros_rect.width()) * v_scale,
            y: boros_rect.height().max(boros_rect.width()) * v_scale,
        },
    );
    viewport.translate_mut(viewport.center().x * -1. * 0.8, viewport.center().y * -1.);

    let camera_uniform = CameraUniform::new(viewport);
    let boro_vertices: Vec<Vertex> = boros
        .iter()
        .flat_map(|geo| {
            let geo = geo.clone();
            let poly: MultiPolygon<f32> = geo.try_into().unwrap();
            poly.into_iter().flat_map(|p| {
                p.earcut_triangles()
                    .into_iter()
                    .flat_map(|tri| tri.coords_iter().map(|coord| Vertex::from(coord)))
            })
        })
        .collect();

    let mut geo: VertexBuffers<Vertex, u32> = VertexBuffers::new();
    let mut builder = Path::builder();

    for shape in shapes.values() {
        let first = shape[0].coord();
        builder.begin(point(first.x as f32, first.y as f32));
        for seq in &shape[1..] {
            let coord = seq.coord();
            builder.line_to(point(coord.x as f32, coord.y as f32));
        }
        builder.end(false);
    }

    let path = builder.build();

    let mut tess = StrokeTessellator::new();


    tess.tessellate_path(&path, &StrokeOptions::default().with_line_width(70.), &mut BuffersBuilder::new(&mut geo, |vertex: StrokeVertex| {
        Vertex {
            position: vertex.position().to_3d().to_array(),
            normal: [0.0, 0.0, 0.0],
            color: [1.0, 1.0, 1.0],
            miter: 0.0,
        }
    })).unwrap();

    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    window.set_min_inner_size(Some(PhysicalSize::new(1600, 1600)));
    window.set_max_inner_size(Some(PhysicalSize::new(1600, 1600)));

    let mut state = render::State::new(
        &window,
        camera_uniform,
        &boro_vertices[..],
        geo,
    )
    .await;

    let (tx, rx) = channel();

    thread::spawn(move || loop {
        let now = Instant::now();
        let msg = format!("hello thread: {:?}", now);
        tx.send(msg).unwrap();
        thread::sleep(Duration::from_secs(5))
    });

    let _ = event_loop.run(move |event, control_flow| match event {
        Event::WindowEvent {
            ref event,
            window_id,
        } if window_id == state.window().id() => {
            if !state.input(event) {
                match event {
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                state: ElementState::Pressed,
                                physical_key: PhysicalKey::Code(KeyCode::Escape),
                                ..
                            },
                        ..
                    } => control_flow.exit(),
                    WindowEvent::Resized(physical_size) => {
                        state.resize(*physical_size);
                    }
                    WindowEvent::RedrawRequested => {
                        state.window().request_redraw();
                        match rx.try_recv() {
                            Ok(data) => println!("{}", data),
                            Err(TryRecvError::Disconnected) => {
                                panic!("Unable to fetch data");
                            }
                            _ => {}
                        }

                        // if !surface_configured {
                        //     return;
                        // }

                        state.update();
                        match state.render() {
                            Ok(_) => {}
                            // Reconfigure the surface if it's lost or outdated
                            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                state.resize(state.size)
                            }
                            // The system is out of memory, we should probably quit
                            Err(wgpu::SurfaceError::OutOfMemory) => {
                                log::error!("OutOfMemory");
                                control_flow.exit();
                            }

                            // This happens when the a frame takes too long to present
                            Err(wgpu::SurfaceError::Timeout) => {
                                log::warn!("Surface timeout")
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    });
    Ok(())
}
