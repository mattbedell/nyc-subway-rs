use crate::util;
use anyhow::Result;
use geo::{self, BoundingRect, Coord, GeometryCollection, MapCoords, Translate};
use serde::{de::Visitor, Deserialize, Deserializer};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Formatter;
use std::ops::{Deref, DerefMut};
use util::static_data::BOROUGH_BOUNDARIES_STATIC;

#[derive(Debug, nyc_subway_rs_derive::Deserialize_enum_or)]
enum LocationKind {
    #[fallback]
    Platform = 0,
    Station = 1,
}

#[derive(Deserialize)]
struct StopRow {
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
    shape_pt_lat: f64,
    shape_pt_lon: f64,
}

#[derive(Debug, Deserialize)]
pub struct Boro {
    #[serde(deserialize_with = "geojson::de::deserialize_geometry")]
    geometry: geo::geometry::Geometry,
}

pub struct Stop {
    pub id: String,
    pub kind: LocationKind,
    pub coord: Coord,
    pub parent: Option<String>,
    pub status: StationStatus,
}

enum StationStatus {
    Active(Vec<String>),
    Inactive,
}

// impl Stop {
//     pub fn new(row: StopRow, center: geo::Point) -> Self {
//         let pos = geo::coord! { x: row.stop_lon as f64, y: row.stop_lat as f64 };
//         let xy = util::geo::coord_to_xy(pos, &center);
//         Self {
//             id: row.stop_id,
//             kind: row.location_type,
//             coord: xy,
//             parent: row.parent_station,
//             status: StationStatus::Inactive,
//         }
//     }
// }

#[derive(Debug)]
pub struct ShapeSeq {
    seq: usize,
    coord: Coord,
}

pub struct ShapeCollection {
    collection: BTreeMap<String, Vec<ShapeSeq>>,
}

pub struct EntityCollection<T> {
    collection: T,
}

impl<K, V> EntityCollection<HashMap<K, V>>
where
    V: CollectableEntity,
{
    pub fn translate_origin_from(&mut self, point: &geo::Point) {
        for val in self.collection.values_mut() {
            let mut coord = val.coord().clone();
            coord = util::geo::coord_to_xy(coord, point);
            val.set_coord(coord);
        }
    }
}

impl EntityCollection<GeometryCollection> {
    pub fn translate_origin_from(&mut self, point: &geo::Point) {
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

pub trait CollectableEntity {
    type Collection;
    fn coord(&self) -> geo::Coord;
    fn set_coord(&mut self, coord: geo::Coord);
    fn collection() -> Self::Collection;
    fn load_collection() -> Result<Self::Collection>;
}

impl CollectableEntity for Stop {
    type Collection = EntityCollection<HashMap<String, Self>>;
    fn coord(&self) -> geo::Coord {
        self.coord
    }

    fn set_coord(&mut self, coord: geo::Coord) {
        self.coord = coord;
    }

    fn collection() -> Self::Collection {
        EntityCollection {
            collection: HashMap::new(),
        }
    }

    fn load_collection() -> Result<Self::Collection> {
        let xdg = util::get_xdg()?;
        let stops_path = xdg.find_data_file("stops.txt").unwrap();
        let mut rdr = csv::Reader::from_path(stops_path)?;
        let mut collection = Self::collection();
        for rec in rdr.deserialize() {
            let row: StopRow = rec?;
            let stop = Stop {
                id: row.stop_id,
                kind: row.location_type,
                coord: geo::coord! { x: row.stop_lon as f64, y: row.stop_lat as f64 },
                parent: row.parent_station,
                status: StationStatus::Inactive,
            };
            collection.insert(stop.id.clone(), stop);
        }
        Ok(collection)
    }
}

impl CollectableEntity for Boro {
    type Collection = EntityCollection<GeometryCollection>;
    fn coord(&self) -> geo::Coord {
        self.geometry.bounding_rect().unwrap().center()
    }

    fn set_coord(&mut self, coord: geo::Coord) {
        let center = self.coord();
        self.geometry
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

// impl EntityCollection<Stop> {
//     pub fn new(origin: geo::Point) -> Self {
//         Self {
//             collection: HashMap::new(),
//             origin
//         }
//     }

//     pub fn load(&mut self) -> Result<()> {
//         let xdg = util::get_xdg()?;
//         let stops_path = xdg.find_data_file("stops.txt").unwrap();
//         let mut rdr = csv::Reader::from_path(stops_path)?;
//         for rec in rdr.deserialize() {
//             let row: StopRow = rec?;
//             let stop = Stop::new(row, self.origin);
//             self.collection.insert(stop.id.clone(), stop);
//         }
//         Ok(())
//     }
// }

// impl EntityCollection<BTreeMap<String, Vec<ShapeSeq>>> {
//     pub fn new(origin: geo::Point) -> Self {
//         Self {
//             collection: BTreeMap::new(),
//             origin,
//         }
//     }

//     pub fn load(&mut self) -> Result<()> {
//         let xdg = util::get_xdg()?;
//         let stops_path = xdg.find_data_file("shapes.txt").unwrap();
//         let mut rdr = csv::Reader::from_path(stops_path)?;
//         for rec in rdr.deserialize() {
//             let row: ShapeRow = rec?;
//             let seq = self.collection.entry(row.shape_id).or_insert(Vec::new());
//             let coord = util::geo::coord_to_xy(
//                 geo::coord! { x: row.shape_pt_lon, y: row.shape_pt_lat },
//                 &self.origin,
//             );
//             seq.push(ShapeSeq {
//                 seq: row.shape_pt_sequence,
//                 coord,
//             });
//         }

//         // for key in self.collection.keys() {
//         //     self.collection.entry(key.clone()).and_modify(|e| e.sort_by(|a, b| a.seq.cmp(&b.seq)));
//         // }
//         for value in self.collection.values_mut() {
//             value.sort_by(|a, b| a.seq.cmp(&b.seq));
//         }
//         Ok(())
//     }
// }
