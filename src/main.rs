use std::fs::File;

use error_chain::error_chain;
use prost::Message;
use reqwest::{self, Response};
use serde_json;


mod proto;

error_chain! {
    foreign_links {
        Io(std::io::Error);
        HttpRequest(reqwest::Error);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let res =
        reqwest::get("https://api-endpoint.mta.info/Dataservice/mtagtfsfeeds/nyct%2Fgtfs-bdfm")
            .await?;

    let bytes = res.bytes().await?;
    let mut feed = proto::gtfs::realtime::FeedMessage::default();
    feed.merge(bytes).unwrap();
    let writer = File::create("gtfs-realtime.json").unwrap();
    serde_json::to_writer(writer, &feed).unwrap();
    Ok(())
}
