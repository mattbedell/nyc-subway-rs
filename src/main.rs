use std::fs::File;

use anyhow::Result;
use prost::Message;
use reqwest::{self, Response};
use serde_json;

mod gtfs;
mod proto;
mod util;

#[tokio::main]
async fn main() -> Result<()> {
    // let res =
    //     reqwest::get("https://api-endpoint.mta.info/Dataservice/mtagtfsfeeds/nyct%2Fgtfs-bdfm")
    //         .await?;

    // let bytes = res.bytes().await?;
    // let mut feed = proto::gtfs::realtime::FeedMessage::default();
    // feed.merge(bytes).unwrap();
    // let writer = File::create("gtfs-realtime.json").unwrap();
    // serde_json::to_writer(writer, &feed).unwrap();

    if gtfs::shoud_fetch() {
        let gtfs_zip = gtfs::fetch().await?;
        gtfs::unzip(gtfs_zip).await?;
    }
    Ok(())
}
