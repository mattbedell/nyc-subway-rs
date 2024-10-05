use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::cell::{RefCell, Cell};
use std::ops::DerefMut;

use anyhow::Result;
use serde::Deserialize;
use serde_repr::{Deserialize_repr};

mod gtfs;
mod proto;
mod util;

#[derive(Debug, Deserialize_repr, PartialEq)]
#[repr(u8)]
enum LocationKind {
    Station = 1,
    #[serde(other)]
    Platform = 0,
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
    fn from(mut v: StopRow) -> Self {
        Stop {
            id: v.stop_id,
            kind: v.location_type,
            pos: Point(0,0),
            lat: v.stop_lat,
            lon: v.stop_lon,
            parent: v.parent_station.take(),
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

    // let mut stops: HashMap<String, RefCell<Stop>> = HashMap::new();
    let mut stops: HashMap<String, Stop> = HashMap::new();

    for rec in rdr.deserialize() {
        let stop_row: StopRow = rec?;
        let stop = Stop::from(stop_row);
        stops.insert(stop.id.clone(), stop);
    };

    let some_stops: Vec<&Stop> = stops.values().take(10).collect();
    println!("{:?}", some_stops);

    // children.into_iter().map(|(mut child, parent_id)| {
    //     let parent = stops.get(&parent_id);
    //     child.parent = parent;
    //     child
    // }).for_each(|child| {
    //     stops.insert(child.id.clone(), child);
    // });

















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
