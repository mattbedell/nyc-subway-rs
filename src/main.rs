use std::fs::File;

use prost::Message;
use reqwest::{self, Response};
use serde_json;
use anyhow::Result;


mod proto;
mod transit;

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

    transit::fetch_gtfs().await?;
    Ok(())
}
