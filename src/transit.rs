use anyhow::Result;
use tokio;
use reqwest;
use xdg;

pub async fn fetch_gtfs() -> Result<()> {
    let res = reqwest::get("http://web.mta.info/developers/files/google_transit_supplemented.zip").await?.bytes().await?;

    let xdg_dirs = xdg::BaseDirectories::with_prefix(env!("CARGO_PKG_NAME"))?;
    let cache_path = xdg_dirs.place_cache_file("google_transit_supplemented.zip")?;
    println!("{:?}", cache_path);
    tokio::fs::write(cache_path, res).await?;
    Ok(())
}
