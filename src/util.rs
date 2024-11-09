use anyhow::Result;
use log::info;
use reqwest;
use std::path::PathBuf;
use std::{fs, io};
use tokio;
use xdg;
use zip;

pub mod geo;

pub fn get_xdg() -> Result<xdg::BaseDirectories> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(env!("CARGO_PKG_NAME"))?;
    Ok(xdg_dirs)
}

pub mod static_data {
    use super::*;

    pub type StaticDataEndpoint = (&'static str, &'static str);

    pub const GTFS_STATIC: StaticDataEndpoint = (
        "http://web.mta.info/developers/files/google_transit_supplemented.zip",
        "nyc_gtfs_supplemented.zip",
    );
    pub const COASTLINE_STATIC: StaticDataEndpoint = (
        "https://data.cityofnewyork.us/resource/59xk-wagz.geojson",
        "nyc_coastline.geojson",
    );
    pub const BOROUGH_BOUNDARIES_STATIC: StaticDataEndpoint = (
        "https://data.cityofnewyork.us/resource/7t3b-ywvw.geojson",
        "nyc_boroughs.geojson",
    );

    pub const PARKS_STATIC: StaticDataEndpoint = (
        "https://data.cityofnewyork.us/resource/enfh-gkve.geojson?typecategory=Flagship Park",
        "nyc_parks.geojson",
    );

    pub async fn fetch(
        endpoint: StaticDataEndpoint,
        base_path: Option<PathBuf>,
    ) -> Result<PathBuf> {
        let xdg = get_xdg()?;
        let mut outfile_path = if let Some(path) = base_path {
            path
        } else {
            xdg.get_cache_home()
        };
        fs::create_dir_all(&outfile_path)?;
        let (static_endpoint, outfile) = endpoint;
        outfile_path.push(outfile);

        info!("Fetching: '{}'", static_endpoint);
        let res = reqwest::get(static_endpoint).await?.bytes().await?;

        tokio::fs::write(&outfile_path, res).await?;
        Ok(outfile_path)
    }
    pub async fn unzip(path: PathBuf) -> Result<()> {
        info!("Unzipping: '{}'", path.display());
        let xdg = get_xdg()?;

        let zipfile = fs::File::open(path)?;

        let mut archive = zip::ZipArchive::new(zipfile)?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;

            if let Some(outpath) = file.enclosed_name() {
                let filename = outpath.file_name().unwrap();
                let data_path = xdg.place_data_file(filename)?;
                let mut outfile = fs::File::create(&data_path)?;
                io::copy(&mut file, &mut outfile)?;
            } else {
                continue;
            }
        }

        Ok(())
    }
    // @todo also check for staleness
    pub fn shoud_fetch(endpoint: StaticDataEndpoint) -> bool {
        let (_, file) = endpoint;
        let xdg = get_xdg().unwrap();
        xdg.find_cache_file(file).or_else(|| xdg.find_data_file(file)).is_none()
    }
}
