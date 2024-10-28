use std::cmp;
use std::collections::HashMap;
use std::fmt::Formatter;

use winit::{
    event::*,
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::WindowBuilder,
};

use anyhow::Result;
use env_logger;
use geo::{
    self, triangulate_earcut::RawTriangulation, Area, BoundingRect, Centroid, Coord,
    CoordinatePosition, CoordsIter, HausdorffDistance, HaversineBearing, HaversineDistance,
    LineString, MapCoords, MapCoordsInPlace, MultiPolygon, Point, Rect, Scale, TriangulateEarcut,
};
use geojson;
use serde::{de::Visitor, Deserialize, Deserializer};

use render::Vertex;
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
    pub pos: Point,
    pub lat: f32,
    pub lon: f32,
    pub parent: Option<String>,
}

impl From<StopRow> for Stop {
    fn from(v: StopRow) -> Self {
        Stop {
            id: v.stop_id,
            kind: v.location_type,
            pos: Point::new(0., 0.),
            lat: v.stop_lat,
            lon: v.stop_lon,
            parent: v.parent_station,
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
    boro_name: String,
    #[serde(deserialize_with = "geojson::de::deserialize_geometry")]
    geometry: geo::geometry::Geometry,
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

    let mut rdr = csv::Reader::from_path(stops_path)?;

    let mut stops: HashMap<String, Stop> = HashMap::new();

    for rec in rdr.deserialize() {
        let stop_row: StopRow = rec?;
        let stop = Stop::from(stop_row);
        stops.insert(stop.id.clone(), stop);
    }

    let feature_reader = {
        use std::fs::File;
        let file = File::open(xdg.find_data_file(BOROUGH_BOUNDARIES_STATIC.1).unwrap()).unwrap();
        geojson::FeatureReader::from_reader(file)
    };

    let mut boros: Vec<Boro> = Vec::new();
    for rec in feature_reader.deserialize().unwrap() {
        let boro: Boro = rec?;
        boros.push(boro);
    }

    let bounding_rect = boros
        .iter()
        .map(|boro| boro.geometry.bounding_rect().unwrap())
        .reduce(util::geo::combine_bounding_rect)
        .unwrap();

    let centroid = bounding_rect.centroid();
    boros = boros
        .into_iter()
        .map(|mut boro| {
            boro.geometry
                .map_coords_in_place(|coord| util::geo::coord_to_xy(coord, centroid));
            boro
        })
        .collect();

    let n_br = boros
        .iter()
        .map(|boro| boro.geometry.bounding_rect().unwrap())
        .reduce(util::geo::combine_bounding_rect)
        .unwrap();

    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    let window_size = window.inner_size();
    let window_area = (window_size.width as f64 * window_size.height as f64);
    let scale_factor = window_area / (n_br.unsigned_area() * 20.);

    let zero: Point = Coord { x: 0., y: 0. }.into();
    boros = boros
        .into_iter()
        .map(|mut boro| {
            boro.geometry
                .scale_around_point_mut(scale_factor, scale_factor, zero);
            boro
        })
        .collect();

    let xy_centroid = n_br.centroid();

    let mp: Vec<Vertex> = boros
        .into_iter()
        .flat_map(|boro| {
            let poly: MultiPolygon = boro.geometry.try_into().unwrap();
            poly.into_iter().flat_map(|p| {
                p.earcut_triangles()
                    .into_iter()
                    .flat_map(|tri| tri.coords_iter().map(|coord| Vertex::from(coord)))
            })
            // let tri = poly.iter().map(|p| p.earcut_triangles_raw());
            // poly.scale_around_point(scale_factor, scale_factor, xy_centroid)
            // poly.scale(scale_factor / 1000.)
        })
        .collect();

    // println!(
    //     "{:?} {:?} {:?} {:?} {:?}",
    //     &mp[0].coords_iter().collect::<Vec<Coord>>()[0..6],
    //     scale_factor,
    //     window_area,
    //     n_br.unsigned_area(),
    //     xy_centroid,
    // );
    let mut state = render::State::new(&window, &mp[..]).await;

    event_loop.run(move |event, control_flow| match event {
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
