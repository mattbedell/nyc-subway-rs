use std::collections::HashMap;
use std::fmt::Formatter;

use anyhow::Result;
use serde::{de::Visitor, Deserialize, Deserializer};

mod gtfs;
mod proto;
mod util;

#[derive(Debug)]
#[derive(nyc_subway_rs_derive::Deserialize_enum_or)]
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
    pub parent: Option<String>
}

impl From<StopRow> for Stop {
    fn from(v: StopRow) -> Self {
        Stop {
            id: v.stop_id,
            kind: v.location_type,
            pos: Point(0,0),
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

#[tokio::main]
async fn main() -> Result<()> {
    if gtfs::shoud_fetch() {
        let gtfs_zip = gtfs::fetch().await?;
        gtfs::unzip(gtfs_zip).await?;
    }

    let xdg = util::get_xdg()?;
    let stops_path = xdg.find_data_file("stops.txt").unwrap();

    let mut rdr = csv::Reader::from_path(stops_path)?;

    let mut stops: HashMap<String, Stop> = HashMap::new();

    let mut log = 0;

    for rec in rdr.deserialize() {
        let stop_row: StopRow = rec?;
        let stop = Stop::from(stop_row);
        if log < 10 {
            println!("{:#?}", stop);
            log += 1;
        }
        stops.insert(stop.id.clone(), stop);
    };

    // let res =
    //     reqwest::get("https://api-endpoint.mta.info/Dataservice/mtagtfsfeeds/nyct%2Fgtfs-bdfm")
    //         .await?;

    // let bytes = res.bytes().await?;
    // let mut feed = proto::gtfs::realtime::FeedMessage::default();
    // feed.merge(bytes).unwrap();
    // let writer = File::create("gtfs-realtime.json").unwrap();
    // serde_json::to_writer(writer, &feed).unwrap();

    Ok(())
}
