use crate::util;
use anyhow::Result;
use geo::{self, BoundingRect, GeometryCollection, MapCoords, Translate};
use serde::de::DeserializeOwned;
use serde::{de::Visitor, Deserialize, Deserializer};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Formatter;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use util::static_data::{BOROUGH_BOUNDARIES_STATIC, PARKS_STATIC};

type Coord = geo::Coord<f32>;
type Point = geo::Point<f32>;

#[derive(Debug, nyc_subway_rs_derive::Deserialize_enum_or)]
enum LocationKind {
    #[fallback]
    Platform = 0,
    Station = 1,
}

#[derive(Deserialize)]
pub struct StopRow {
    stop_id: String,
    stop_lat: f32,
    stop_lon: f32,
    location_type: LocationKind,
    parent_station: Option<String>,
}

#[derive(Deserialize)]
struct ShapeRow {
    shape_id: String,
    shape_pt_sequence: usize,
    shape_pt_lat: f32,
    shape_pt_lon: f32,
}

#[derive(Debug, Deserialize)]
pub struct Boro {
    #[serde(deserialize_with = "geojson::de::deserialize_geometry")]
    geometry: geo::geometry::Geometry<f32>,
}

#[derive(Debug, Deserialize)]
pub struct Park {
    #[serde(deserialize_with = "geojson::de::deserialize_geometry")]
    geometry: geo::geometry::Geometry<f32>,
}

#[derive(Debug)]
pub struct Stop {
    pub id: String,
    pub kind: LocationKind,
    pub coord: Coord,
    pub parent: Option<String>,
    pub status: StationStatus,
    pub index: usize,
}

#[derive(Debug)]
enum StationStatus {
    Active(Vec<String>),
    Inactive,
}

#[derive(Debug)]
pub struct ShapeSeq {
    seq: usize,
    coord: Coord,
}

#[derive(Deserialize, Debug)]
pub struct Route {
    #[serde(rename = "route_id")]
    id: String,
    #[serde(rename = "route_color")]
    #[serde(deserialize_with = "hex_to_srgb")]
    color: [f32; 3],
}

impl Route {
    pub fn color(&self) -> [f32; 3] {
        self.color
    }
}

fn hex_to_srgb<'de, D>(deserializer: D) -> Result<[f32; 3], D::Error>
where
    D: Deserializer<'de>,
{
    let mut color = [0; 3];
    let mut hex_str: &str = serde::Deserialize::deserialize(deserializer)?;

    if hex_str.len() != 6 {
        hex_str = "FFFFFF";
    }
    hex::decode_to_slice(hex_str, &mut color).unwrap();
    let linear_color = srgb::gamma::linear_from_u8(color);
    Ok(linear_color)
}

pub struct EntityCollection<T> {
    collection: T,
}

impl<K, V> EntityCollection<HashMap<K, V>>
where
    V: CollectibleEntity,
{
    pub fn translate_origin_from(&mut self, point: &Point) {
        for val in self.collection.values_mut() {
            let mut coord = val.coord().clone();
            coord = util::geo::coord_to_xy(coord, point);
            val.set_coord(coord);
        }
    }
}

impl<K, V> EntityCollection<BTreeMap<K, V>>
where
    V: CollectibleEntity,
{
    pub fn translate_origin_from(&mut self, point: &Point) {
        for val in self.collection.values_mut() {
            let mut coord = val.coord().clone();
            coord = util::geo::coord_to_xy(coord, point);
            val.set_coord(coord);
        }
    }
}

impl EntityCollection<BTreeMap<String, Vec<ShapeSeq>>> {
    pub fn translate_origin_from(&mut self, point: &Point) {
        for shape in self.collection.values_mut() {
            for seq in shape.iter_mut() {
                let mut coord = seq.coord().clone();
                coord = util::geo::coord_to_xy(coord, point);
                seq.set_coord(coord);
            }
        }
    }
}

impl EntityCollection<GeometryCollection<f32>> {
    pub fn translate_origin_from(&mut self, point: &Point) {
        self.collection = self.map_coords(|c| util::geo::coord_to_xy(c, &point));
    }
}

impl<T> Deref for EntityCollection<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.collection
    }
}

impl<T> DerefMut for EntityCollection<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.collection
    }
}

pub trait CollectibleEntity {
    type Collection;
    fn coord(&self) -> Coord;
    fn set_coord(&mut self, coord: Coord);
    fn collection() -> Self::Collection;
    fn load_collection() -> Result<Self::Collection>;
}

impl CollectibleEntity for Stop {
    type Collection = EntityCollection<BTreeMap<String, Self>>;
    fn coord(&self) -> geo::Coord<f32> {
        self.coord
    }

    fn set_coord(&mut self, coord: Coord) {
        self.coord = coord;
    }

    fn collection() -> Self::Collection {
        EntityCollection {
            collection: BTreeMap::new(),
        }
    }

    fn load_collection() -> Result<Self::Collection> {
        let xdg = util::get_xdg()?;
        let stops_path = xdg.find_data_file("stops.txt").unwrap();
        let mut rdr = csv::Reader::from_path(stops_path)?;
        let mut collection = Self::collection();
        let mut parent_idxs = HashMap::new();
        let mut idx = 0;
        for rec in rdr.deserialize() {
            let row: StopRow = rec?;
            let index = if let None = row.parent_station {
                let ndx = idx;
                parent_idxs.insert(row.stop_id.to_owned(), ndx);
                idx += 1;
                ndx
            } else {
                0
            };

            let stop = Stop {
                id: row.stop_id,
                kind: row.location_type,
                coord: geo::coord! { x: row.stop_lon, y: row.stop_lat },
                parent: row.parent_station,
                status: StationStatus::Inactive,
                index,
            };
            collection.insert(stop.id.clone(), stop);
        }

        // only parent stops are rendered, but we still want to look up the rendered stop instances by child stop_id
        for stop in collection.values_mut() {
            if let Some(parent_id) = &stop.parent {
                stop.index = parent_idxs.get(parent_id).unwrap().to_owned();
            }
        }

        Ok(collection)
    }
}

impl CollectibleEntity for ShapeSeq {
    type Collection = EntityCollection<BTreeMap<String, Vec<Self>>>;
    fn coord(&self) -> Coord {
        self.coord
    }

    fn set_coord(&mut self, coord: Coord) {
        self.coord = coord;
    }

    fn collection() -> Self::Collection {
        EntityCollection {
            collection: BTreeMap::new(),
        }
    }

    fn load_collection() -> Result<Self::Collection> {
        let xdg = util::get_xdg()?;
        let stops_path = xdg.find_data_file("shapes.txt").unwrap();
        let mut rdr = csv::Reader::from_path(stops_path)?;
        let mut collection = Self::collection();
        for rec in rdr.deserialize() {
            let row: ShapeRow = rec?;
            let shape = ShapeSeq {
                coord: geo::coord! { x: row.shape_pt_lon, y: row.shape_pt_lat },
                seq: row.shape_pt_sequence,
            };
            let seq = collection
                .entry(row.shape_id.clone())
                .or_insert_with(|| Vec::new());
            seq.push(shape);
        }
        for seq in collection.values_mut() {
            seq.sort_by(|a, b| a.seq.cmp(&b.seq));
        }

        Ok(collection)
    }
}

impl CollectibleEntity for Route {
    type Collection = EntityCollection<HashMap<String, Route>>;
    fn coord(&self) -> Coord {
        Coord::zero()
    }

    fn set_coord(&mut self, _coord: Coord) {}

    fn collection() -> Self::Collection {
        EntityCollection {
            collection: HashMap::new(),
        }
    }

    fn load_collection() -> Result<Self::Collection> {
        let xdg = util::get_xdg()?;
        let path = xdg.find_data_file("routes.txt").unwrap();
        let mut rdr = csv::Reader::from_path(path)?;
        let mut collection = Self::collection();
        for rec in rdr.deserialize() {
            let row: Route = rec?;
            collection.insert(row.id.clone(), row);
        }
        Ok(collection)
    }
}

impl CollectibleEntity for Boro {
    type Collection = EntityCollection<GeometryCollection<f32>>;
    fn coord(&self) -> Coord {
        self.geometry.bounding_rect().unwrap().center()
    }

    fn set_coord(&mut self, coord: Coord) {
        let center = self.coord();
        self.geometry = self
            .geometry
            .translate(coord.x - center.x, coord.y - center.y);
    }
    fn collection() -> Self::Collection {
        EntityCollection {
            collection: GeometryCollection::default(),
        }
    }
    fn load_collection() -> Result<Self::Collection> {
        let xdg = util::get_xdg()?;
        let feature_reader = {
            use std::fs::File;
            let file =
                File::open(xdg.find_data_file(BOROUGH_BOUNDARIES_STATIC.1).unwrap()).unwrap();
            geojson::FeatureReader::from_reader(file)
        };

        let mut geos = Vec::new();
        for rec in feature_reader.deserialize().unwrap() {
            let boro: Boro = rec?;
            geos.push(boro.geometry);
        }

        Ok(EntityCollection {
            collection: GeometryCollection(geos),
        })
    }
}

impl CollectibleEntity for Park {
    type Collection = EntityCollection<GeometryCollection<f32>>;
    fn coord(&self) -> Coord {
        self.geometry.bounding_rect().unwrap().center()
    }

    fn set_coord(&mut self, coord: Coord) {
        let center = self.coord();
        self.geometry = self
            .geometry
            .translate(coord.x - center.x, coord.y - center.y);
    }
    fn collection() -> Self::Collection {
        EntityCollection {
            collection: GeometryCollection::default(),
        }
    }
    fn load_collection() -> Result<Self::Collection> {
        let xdg = util::get_xdg()?;
        let feature_reader = {
            use std::fs::File;
            let file = File::open(xdg.find_data_file(PARKS_STATIC.1).unwrap()).unwrap();
            geojson::FeatureReader::from_reader(file)
        };

        let mut geos = Vec::new();
        for rec in feature_reader.deserialize().unwrap() {
            let park: Park = rec?;
            geos.push(park.geometry);
        }

        Ok(EntityCollection {
            collection: GeometryCollection(geos),
        })
    }
}

trait StaticData {
    const FILENAME: &'static str;
    fn filepath() -> PathBuf {
        let xdg = util::get_xdg().unwrap();
        let path = xdg
            .find_data_file(Self::FILENAME)
            .expect(format!("Could not find load {} data", Self::FILENAME).as_str());
        path
    }
}

pub trait GTFSData<T>: Default
where
    T: StaticData + DeserializeOwned,
{
    fn load() -> Self {
        let path = T::filepath();
        let mut rdr = csv::Reader::from_path(path).unwrap();
        let mut collection = Self::default();
        for rec in rdr.deserialize() {
            let row: T = rec.unwrap();
            collection.insert_row(row);
        }
        collection
    }

    fn insert_row(&mut self, value: T);
}

impl StaticData for StopRow {
    const FILENAME: &'static str = "stops.txt";
}

impl GTFSData<StopRow> for HashMap<String, Stop> {
    fn insert_row(&mut self, value: StopRow) {
        self.insert(
            value.stop_id.clone(),
            Stop {
                id: value.stop_id,
                kind: value.location_type,
                coord: geo::coord! { x: value.stop_lon, y: value.stop_lat },
                parent: value.parent_station,
                status: StationStatus::Inactive,
                index: 0,
            },
        );
    }
}

impl StaticData for ShapeRow {
    const FILENAME: &'static str = "shapes.txt";
}

impl GTFSData<ShapeRow> for BTreeMap<String, Vec<ShapeSeq>> {
    fn insert_row(&mut self, value: ShapeRow) {
        let shape = ShapeSeq {
            coord: geo::coord! { x: value.shape_pt_lon, y: value.shape_pt_lat },
            seq: value.shape_pt_sequence,
        };
        let seq = self
            .entry(value.shape_id.clone())
            .or_insert_with(|| Vec::new());
        seq.push(shape);
    }
}

impl StaticData for Route {
    const FILENAME: &'static str = "routes.txt";
}

impl GTFSData<Route> for HashMap<String, Route> {
    fn insert_row(&mut self, value: Route) {
        self.insert(value.id.clone(), value);
    }
}
