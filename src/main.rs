use std::cmp;
use std::collections::HashMap;
use std::fmt::Formatter;

use anyhow::Result;
use env_logger;
use geo::{self, BoundingRect, Coord, Rect};
use geojson;
use serde::{de::Visitor, Deserialize, Deserializer};

use util::static_data::{self, BOROUGH_BOUNDARIES_STATIC, COASTLINE_STATIC, GTFS_STATIC};

mod proto;
mod util;
mod render;

#[derive(Debug, nyc_subway_rs_derive::Deserialize_enum_or)]
enum LocationKind {
    #[fallback]
    Platform = 0,
    Station = 1,
}

#[derive(Debug)]
struct Point(u16, u16);

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
            pos: Point(0, 0),
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

    let mut log = 0;

    for rec in rdr.deserialize() {
        let stop_row: StopRow = rec?;
        let stop = Stop::from(stop_row);
        if log < 10 {
            // println!("{:#?}", stop);
            log += 1;
        }
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
    let f = boros.first().unwrap();
    let rect = f.geometry.bounding_rect();
    let bounding_rect = boros
        .iter()
        .map(|boro| boro.geometry.bounding_rect().unwrap())
        .reduce(|acc, rect| {
            let Coord { x: min_x, y: min_y } = acc.min();
            let Coord {
                x: omin_x,
                y: omin_y,
            } = rect.min();
            let Coord { x: max_x, y: max_y } = acc.max();
            let Coord {
                x: omax_x,
                y: omax_y,
            } = rect.max();
            let nmin = Coord {
                x: min_x.min(omin_x),
                y: min_y.min(omin_y),
            };
            let nmax = Coord {
                x: max_x.max(omax_x),
                y: max_y.max(omax_y),
            };
            Rect::new(nmin, nmax)
        }).unwrap();
    println!("{:?}: width: {} height: {}", bounding_rect, f.geometry.bounding_rect().unwrap().width(), f.geometry.bounding_rect().unwrap().height());
    render::run().await;
    Ok(())
}
