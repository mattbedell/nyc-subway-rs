use anyhow::Result;
use reqwest;
use std::path::PathBuf;
use std::{fs, io};
use tokio;
use zip;

use crate::util;

const TRANSIT_FILENAME: &str = "google_transit_supplemented.zip";

pub async fn fetch() -> Result<PathBuf> {
    // @todo we might not need the larger supplemented files, the regular zip may suffice
    let res = reqwest::get("http://web.mta.info/developers/files/google_transit_supplemented.zip")
        .await?
        .bytes()
        .await?;

    let xdg = util::get_xdg()?;
    let gtfs_path = xdg.place_cache_file(TRANSIT_FILENAME)?;
    tokio::fs::write(&gtfs_path, res).await?;
    Ok(gtfs_path)
}

pub async fn unzip(path: PathBuf) -> Result<()> {
    let xdg = util::get_xdg()?;

    let zipfile = fs::File::open(path)?;

    let mut archive = zip::ZipArchive::new(zipfile)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;

        if let Some(outpath) = file.enclosed_name() {
            let filename = outpath.file_name().unwrap();
            let data_path = xdg.place_data_file(filename)?;
            println!("{:?}", data_path);
            let mut outfile = fs::File::create(&data_path)?;
            io::copy(&mut file, &mut outfile)?;
        } else {
            continue;
        }
    }

    Ok(())
}

// @todo also check for staleness
pub fn shoud_fetch() -> bool {
    let xdg = util::get_xdg().unwrap();
    xdg.find_cache_file(TRANSIT_FILENAME).map_or(true, |_| false)
}
