use anyhow::Result;
use xdg;

pub fn get_xdg() -> Result<xdg::BaseDirectories> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(env!("CARGO_PKG_NAME"))?;
    Ok(xdg_dirs)
}
