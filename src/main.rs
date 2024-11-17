use feed::FeedManager;
use lyon::geom::point;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers,
};
use std::sync::mpsc::{channel, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
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
    BoundingRect, Coord, CoordsIter, MultiPolygon, Point, Rect, Translate,
    TriangulateEarcut,
};

use entities::CollectibleEntity;
use render::{CameraUniform, Vertex};
use util::static_data::{
    self, BOROUGH_BOUNDARIES_STATIC, COASTLINE_STATIC, GTFS_STATIC, PARKS_STATIC,
};

mod entities;
mod feed;
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

    if static_data::shoud_fetch(PARKS_STATIC) {
        static_data::fetch(PARKS_STATIC, Some(xdg.get_data_home())).await?;
    }

    let mut boros = entities::Boro::load_collection()?;
    let mut shapes = entities::ShapeSeq::load_collection()?;
    let mut stops = entities::Stop::load_collection()?;
    let mut parks = entities::Park::load_collection()?;
    let routes = entities::Route::load_collection()?;

    let o_rect = boros.bounding_rect().unwrap();
    let origin: Point<f32> = o_rect.center().into();

    boros.translate_origin_from(&origin);
    parks.translate_origin_from(&origin);
    shapes.translate_origin_from(&origin);
    stops.translate_origin_from(&origin);
    let rc_stops = Arc::new(stops);
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
    let boro_vertices: Vec<_> = boros
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

    // let park_vertices = parks.iter().flat_map(|geo| {
    //     let geo = geo.clone();
    //     let poly: MultiPolygon<f32> = geo.try_into().unwrap();
    //     poly.into_iter().flat_map(|p| {
    //         p.earcut_triangles().into_iter().flat_map(|tri| {
    //             tri.coords_iter().map(|coord| Vertex {
    //                 position: [coord.x, coord.y, 0.0],
    //                 color: [0.20, 0.3, 0.20],
    //                 ..Vertex::default()
    //             })
    //         })
    //     })
    // });

    // boro_vertices.extend(park_vertices);

    let mut geo: VertexBuffers<Vertex, u32> = VertexBuffers::new();
    let mut stroke = Path::builder();

    for shape in shapes.values() {
        let first = shape[0].coord();
        stroke.begin(point(first.x, first.y));
        for seq in &shape[1..] {
            let coord = seq.coord();
            stroke.line_to(point(coord.x, coord.y));
        }
        stroke.end(false);
    }

    let stroke_path = stroke.build();

    let mut stroke_tessellator = StrokeTessellator::new();
    let mut fill_tessellator = FillTessellator::new();

    stroke_tessellator
        .tessellate_path(
            &stroke_path,
            &StrokeOptions::default().with_line_width(70.),
            &mut BuffersBuilder::new(&mut geo, |vertex: StrokeVertex| Vertex {
                position: vertex.position().to_3d().to_array(),
                normal: [0.0, 0.0, 0.0],
                color: [1.0, 1.0, 1.0],
                miter: 0.0,
            }),
        )
        .unwrap();
    {
        let builder = &mut BuffersBuilder::new(&mut geo, |vertex: FillVertex| Vertex {
            position: vertex.position().to_3d().to_array(),
            normal: [0.0, 0.0, 0.0],
            color: [1.0, 1.0, 1.0],
            miter: 0.0,
        });
        for stop in rc_stops.values() {
            fill_tessellator
                .tessellate_circle(
                    point(stop.coord.x, stop.coord.y),
                    120.,
                    &FillOptions::default(),
                    builder,
                )
                .unwrap();
        }
    }

    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    window.set_min_inner_size(Some(PhysicalSize::new(1600, 1600)));
    window.set_max_inner_size(Some(PhysicalSize::new(1600, 1600)));

    let mut state = render::State::new(&window, camera_uniform, &boro_vertices[..], geo).await;

    let (tx, rx) = channel();
    let stops_collection = rc_stops.clone();
    thread::spawn(move || {
        let mut feed_manager = FeedManager::new(&stops_collection, &routes, tx);

        loop {
            feed_manager.update();
            thread::sleep(Duration::from_millis(200));
        }
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
                            Ok(data) => {
                                state.update_stops(data);
                            }
                            Err(TryRecvError::Disconnected) => {
                                panic!("Unable to fetch data");
                            }
                            _ => {}
                        }

                        // if !surface_configured {
                        //     return;
                        // }

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
