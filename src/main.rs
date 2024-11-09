use cgmath::Vector3;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Formatter;
use std::ops::Range;
use std::sync::mpsc::{channel, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};
use tokio;

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
    self, BoundingRect, Contains, Coord, CoordsIter, GeometryCollection, MapCoords, MultiPolygon,
    Point, Polygon, Rect, Relate, Scale, Translate, TriangulateEarcut,
};
use geojson;
use serde::{de::Visitor, Deserialize, Deserializer};

use render::{CameraUniform, Vertex};
use util::static_data::{self, BOROUGH_BOUNDARIES_STATIC, COASTLINE_STATIC, GTFS_STATIC};

mod proto;
mod render;
mod util;

#[derive(Debug, nyc_subway_rs_derive::Deserialize_enum_or)]
enum LocationKind {
    #[fallback]
    Platform = 0,
    Station = 1,
}

#[derive(Debug)]
struct Stop {
    pub id: String,
    pub kind: LocationKind,
    pub geometry: Polygon,
    pub parent: Option<String>,
}

impl Stop {
    pub fn new(row: StopRow, rel_center: &Point) -> Self {
        let pos = geo::coord! { x: row.stop_lon as f64, y: row.stop_lat as f64 };
        let xy = util::geo::coord_to_xy(pos, rel_center);
        Self {
            id: row.stop_id,
            kind: row.location_type,
            geometry: util::geo::circle(xy, 90.),
            parent: row.parent_station,
        }
    }
}

#[derive(Deserialize)]
struct StopRow {
    stop_id: String,
    stop_lat: f32,
    stop_lon: f32,
    location_type: LocationKind,
    parent_station: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Boro {
    #[serde(deserialize_with = "geojson::de::deserialize_geometry")]
    geometry: geo::geometry::Geometry,
}

#[derive(Deserialize)]
struct ShapeRow {
    shape_id: String,
    shape_pt_sequence: usize,
    shape_pt_lat: f64,
    shape_pt_lon: f64,
}

struct StaticRanges {
    pub boros: Range<u32>,
    pub stops: Range<u32>,
}

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

    let stops_path = xdg.find_data_file("stops.txt").unwrap();
    let shapes_path = xdg.find_data_file("shapes.txt").unwrap();

    let mut stops_rdr = csv::Reader::from_path(stops_path)?;
    let mut shapes_rdr = csv::Reader::from_path(shapes_path)?;

    let feature_reader = {
        use std::fs::File;
        let file = File::open(xdg.find_data_file(BOROUGH_BOUNDARIES_STATIC.1).unwrap()).unwrap();
        geojson::FeatureReader::from_reader(file)
    };

    let mut boro_geo: Vec<geo::Geometry> = Vec::new();
    for rec in feature_reader.deserialize().unwrap() {
        let boro: Boro = rec?;
        boro_geo.push(boro.geometry);
    }

    let mut geoc = GeometryCollection(boro_geo);
    let o_center: Point = geoc.bounding_rect().unwrap().center().into();

    let mut stops: HashMap<String, Stop> = HashMap::new();
    for rec in stops_rdr.deserialize() {
        let stop_row: StopRow = rec?;
        let stop = Stop::new(stop_row, &o_center);
        stops.insert(stop.id.clone(), stop);
    }

    let mut shapes: BTreeMap<String, BTreeMap<usize, Coord>> = BTreeMap::new();
    for rec in shapes_rdr.deserialize() {
        let shape_row: ShapeRow = rec?;
        let seq = shapes.entry(shape_row.shape_id).or_insert(BTreeMap::new());
        let coord = util::geo::coord_to_xy(
            geo::coord! { x: shape_row.shape_pt_lon, y: shape_row.shape_pt_lat },
            &o_center,
        );
        seq.insert(shape_row.shape_pt_sequence, coord);
    }
    let shape_vertices = shapes
        .values()
        .fold((Vec::new(), Vec::new()), |mut acc, shape| {
            let vertices: Vec<Vertex> = shape
                .values()
                .map(|c| Vertex {
                    color: [1.0, 1.0, 1.0],
                    ..Vertex::from(c)
                })
                .collect();
            let alen = acc.1.len() as u32;
            acc.1.extend(vertices);
            acc.0.push(alen..acc.1.len() as u32);
            acc
        });

    geoc = geoc.map_coords(|coord| util::geo::coord_to_xy(coord, &o_center));

    let vp_br = geoc.bounding_rect().unwrap();
    let v_scale = 0.6;
    let mut viewport = Rect::new(
        Coord::zero(),
        Coord {
            x: vp_br.height().max(vp_br.width()) * v_scale,
            y: vp_br.height().max(vp_br.width()) * v_scale,
        },
    );
    viewport.translate_mut(viewport.center().x * -1. * 0.8, viewport.center().y * -1.);

    let camera_uniform = CameraUniform::new(viewport);
    let mut boro_vertices: Vec<Vertex> = geoc
        .into_iter()
        .flat_map(|geo| {
            let poly: MultiPolygon = geo.try_into().unwrap();
            poly.into_iter().flat_map(|p| {
                p.earcut_triangles()
                    .into_iter()
                    .flat_map(|tri| tri.coords_iter().map(|coord| Vertex::from(coord)))
            })
        })
        .collect();

    let stop_vertices = stops
        .values()
        .filter(|stop| {
            if let LocationKind::Station = stop.kind {
                true
            } else {
                false
            }
        })
        .flat_map(|stop| {
            stop.geometry
                .scale(v_scale)
                .earcut_triangles_iter()
                .fold(vec![], |mut acc, tri| {
                    let color = [1.0, 1.0, 1.0];
                    acc.push(Vertex::new(tri.0, color));
                    acc.push(Vertex::new(tri.1, color));
                    acc.push(Vertex::new(tri.2, color));
                    acc
                })
        });

    let b_len = boro_vertices.len() as u32;
    boro_vertices.extend(stop_vertices);

    let static_ranges = StaticRanges {
        boros: 0..b_len,
        stops: b_len..boro_vertices.len() as u32,
    };

    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    window.set_min_inner_size(Some(PhysicalSize::new(1600, 1600)));
    window.set_max_inner_size(Some(PhysicalSize::new(1600, 1600)));

    let mut state = render::State::new(
        &window,
        camera_uniform,
        static_ranges,
        &boro_vertices[..],
        &shape_vertices.1[..],
        shape_vertices.0,
    )
    .await;

    // tokio::task::spawn_blocking(move || async move {
    //     println!("foo");
    //     let sleep_ms = 5000;
    //     let sleep = tokio::time::sleep(Duration::from_secs(sleep_ms));
    //     tokio::pin!(sleep);
    //     loop {
    //         tokio::select! {
    //             () = &mut sleep => {
    //                 println!("{:?}", tokio::time::Instant::now());
    //                 sleep.as_mut().reset(tokio::time::Instant::now() + tokio::time::Duration::from_secs(sleep_ms));
    //             },
    //         }
    //     }
    // });

    // rx.recv()

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
