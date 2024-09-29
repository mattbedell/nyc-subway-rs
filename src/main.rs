use std::collections::HashMap;
use std::cell::{RefCell, Cell};

use anyhow::Result;
use serde::Deserialize;

mod gtfs;
mod proto;
mod util;

#[derive(Deserialize)]
enum LocationKind {
    Platform = 0,
    Station = 1,
}

struct Point(u16, u16);

struct Stop<'a> {
    pub id: String,
    pub kind: LocationKind,
    pub pos: Point,
    pub lat: f32,
    pub lon: f32,
    pub parent: Option<&'a Stop<'a>>
}

impl<'a> From<StopRow> for Stop<'a> {
    fn from(v: StopRow) -> Self {
        Stop {
            id: v.stop_id,
            kind: v.location_type.unwrap_or(LocationKind::Platform),
            pos: Point(0,0),
            lat: v.stop_lat,
            lon: v.stop_lon,
            parent: None,
        }
    }
}

#[derive(Deserialize)]
struct StopRow {
    stop_id: String,
    stop_lat: f32,
    stop_lon: f32,
    location_type: Option<LocationKind>,
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

    let mut stops: HashMap<String, RefCell<Stop>> = HashMap::new();

    let mut child_ids: Vec<(String, String)> = vec![];

    for rec in rdr.deserialize() {
        let stop_row: StopRow = rec?;
        let parent = stop_row.parent_station.clone();
        let stop = Stop::from(stop_row);
        if let Some(parent) = parent {
            child_ids.push((stop.id.clone(), parent));
        }
        stops.insert(stop.id.clone(), RefCell::new(stop));
    };

    for (child_id, parent_id) in child_ids {
        let parent = stops.get(&parent_id).unwrap();
        let child = stops.get(&child_id).unwrap();
        let p = *parent.borrow();
        child.borrow_mut().parent = Some(*parent.borrow());
    };


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
