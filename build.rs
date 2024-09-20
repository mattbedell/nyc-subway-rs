use std::io::Result;
use prost_build::Config;

fn main() -> Result<()> {
    let mut config = Config::new();

    config.type_attribute(".", "#[derive(serde::Serialize)]");

    config.compile_protos(&["proto/gtfs/gtfs-realtime-NYCT.proto"], &["proto/"])?;
    Ok(())
}
