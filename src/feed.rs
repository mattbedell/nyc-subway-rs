use std::{
    collections::{HashMap, HashSet, VecDeque},
    iter::Cycle,
    slice::Iter,
    sync::mpsc::Sender,
};

use geo::Coord;
use lyon::{
    geom::{euclid::num::Floor, point},
    tessellation::{BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers},
};
use prost::Message;
use reqwest::blocking::Client;

use crate::{
    entities::{CollectibleEntity, EntityCollection, Route, Stop},
    proto::gtfs::realtime::{vehicle_position::VehicleStopStatus, FeedMessage},
    render::Vertex,
};

#[derive(Debug)]
pub enum Feed {
    ACE,
    G,
    NQRW,
    S1234567,
    BDFM,
    JZ,
    L,
    SIR,
}

pub const FEEDS: [Feed; 8] = [
    Feed::ACE,
    Feed::G,
    Feed::NQRW,
    Feed::S1234567,
    Feed::BDFM,
    Feed::JZ,
    Feed::L,
    Feed::SIR,
];

// pub const FEEDS: [Feed; 1] = [
//     Feed::G,
// ];

impl Feed {
    pub fn endpoint(&self) -> &str {
        match self {
            Self::ACE => "https://api-endpoint.mta.info/Dataservice/mtagtfsfeeds/nyct%2Fgtfs-ace",
            Self::G => "https://api-endpoint.mta.info/Dataservice/mtagtfsfeeds/nyct%2Fgtfs-g",
            Self::NQRW => "https://api-endpoint.mta.info/Dataservice/mtagtfsfeeds/nyct%2Fgtfs-nqrw",
            Self::S1234567 => "https://api-endpoint.mta.info/Dataservice/mtagtfsfeeds/nyct%2Fgtfs",
            Self::BDFM => "https://api-endpoint.mta.info/Dataservice/mtagtfsfeeds/nyct%2Fgtfs-bdfm",
            Self::JZ => "https://api-endpoint.mta.info/Dataservice/mtagtfsfeeds/nyct%2Fgtfs-jz",
            Self::L => "https://api-endpoint.mta.info/Dataservice/mtagtfsfeeds/nyct%2Fgtfs-l",
            Self::SIR => "https://api-endpoint.mta.info/Dataservice/mtagtfsfeeds/nyct%2Fgtfs-si",
        }
    }
}

struct FeedEntity {
    stop_id: String,
    route_id: String,
    trip_id: String,
    timestamp: u64,
    render: Option<(lyon::geom::Point<f32>, [f32; 3])>,
}

enum FeedOp {
    Add(FeedEntity),
    Remove(String),
}

pub struct FeedManager<'a> {
    client: Client,
    feeds: Vec<FeedProcessor<'a>>,
    feed_idx: usize,
    stop_vertices: VertexBuffers<Vertex, u32>,
    tessellator: FillTessellator,
    tx: Sender<VertexBuffers<Vertex, u32>>,
}

struct FeedProcessor<'a> {
    stops: &'a EntityCollection<HashMap<String, Stop>>,
    routes: &'a EntityCollection<HashMap<String, Route>>,
    fetched_at: u64,
    queue: VecDeque<FeedOp>,
    active_stops: HashMap<String, FeedEntity>,
    feed: &'a Feed,
}

impl<'a> FeedManager<'a> {
    pub fn new(
        stops: &'a EntityCollection<HashMap<String, Stop>>,
        routes: &'a EntityCollection<HashMap<String, Route>>,
        tx: Sender<VertexBuffers<Vertex, u32>>,
    ) -> Self {
        let client = Client::new();
        let feeds = FEEDS
            .iter()
            .map(|feed| FeedProcessor {
                stops,
                routes,
                fetched_at: 0,
                queue: VecDeque::new(),
                active_stops: HashMap::new(),
                feed,
            })
            .collect::<Vec<_>>();

        Self {
            feed_idx: 0,
            client,
            feeds,
            stop_vertices: VertexBuffers::new(),
            tessellator: FillTessellator::new(),
            tx,
        }
    }

    pub fn update(&mut self) {
        if self.feed_idx >= self.feeds.len() {
            self.feed_idx = 0;
        }

        let feed = &mut self.feeds[self.feed_idx];

        let batch = feed.queue.len() as f32 / 10.;

        for _ in 0..batch.ceil().max(1.) as u32 {
            let feed = &mut self.feeds[self.feed_idx];
            if let Some(_) = feed.update() {
                self.stop_vertices.clear();
                let mut active_stops: Vec<_> = self
                    .feeds
                    .iter()
                    .flat_map(|feed| feed.active_stops.values())
                    .collect();
                active_stops.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

                for stop in active_stops {
                    let (point, color) = stop.render.unwrap();
                    let _ = self.tessellator.tessellate_circle(
                        point,
                        200.,
                        &FillOptions::default(),
                        &mut BuffersBuilder::new(&mut self.stop_vertices, |vertex: FillVertex| {
                            Vertex {
                                position: vertex.position().to_3d().to_array(),
                                normal: [0.0, 0.0, 0.0],
                                color,
                                miter: 0.0,
                            }
                        }),
                    );
                }

                self.tx.send(self.stop_vertices.clone()).unwrap();
            } else {
                feed.fetch(&self.client);
                feed.update();
                break;
            }
        }
    self.feed_idx += 1;
    }
}

impl FeedProcessor<'_> {
    fn update(&mut self) -> Option<()> {
        match self.queue.pop_front() {
            Some(FeedOp::Add(mut feed_entity)) => {
                let route = self.routes.get(&feed_entity.route_id);
                let stop = self.stops.get(&feed_entity.stop_id);

                // some stops are not public stations and are not part of the static schedule, e.g. R60S, R60N
                if route.is_none() || stop.is_none() {
                    return Some(());
                }

                let color = route.unwrap().color();
                let coord = stop.unwrap().coord;

                feed_entity.render = Some((point(coord.x, coord.y), color));
                self.active_stops
                    .insert(feed_entity.trip_id.to_owned(), feed_entity);
                Some(())
            }
            Some(FeedOp::Remove(trip_id)) => {
                self.active_stops.remove(&trip_id);
                Some(())
            }
            None => None,
        }
    }

    pub fn fetch(&mut self, client: &Client) {
        let response = client.get(self.feed.endpoint()).send().unwrap();
        let msg = FeedMessage::decode(response.bytes().unwrap()).unwrap();
        let timestamp = msg.header.timestamp();

        if self.fetched_at >= timestamp {
            return;
        }
        self.fetched_at = timestamp;

        let mut latest_trip_stop: HashMap<String, String> = HashMap::new();
        let mut vehicle_updates = Vec::new();
        for entity in msg.entity {
            // get stopped vehicles
            if let Some(vehicle_pos) = entity.vehicle {
                if vehicle_pos.stop_id.is_some() && vehicle_pos.trip.is_some() {
                    if let VehicleStopStatus::StoppedAt = vehicle_pos.current_status() {
                        let trip = vehicle_pos.trip.as_ref().unwrap();
                        vehicle_updates.push(FeedEntity {
                            trip_id: trip.trip_id().to_owned(),
                            timestamp: vehicle_pos.timestamp(),
                            route_id: trip.route_id().to_owned(),
                            stop_id: vehicle_pos.stop_id().to_owned(),
                            render: None,
                        });
                    }
                }
            }
            // get the latest stop_time_update for each trip, which contains the next stop being approached or stopped at
            if let Some(trip_update) = entity.trip_update {
                let trip_id = trip_update.trip.trip_id();
                if let Some(stop_update) = trip_update.stop_time_update.first() {
                    let stop_id = stop_update.stop_id();
                    if let None = stop_update.stop_id {
                        println!("NO STOP ID FOR TRIP {trip_id}");
                    }
                    latest_trip_stop.insert(trip_id.into(), stop_id.into());
                }
            }
        }

        // only get vehicles that are at the current stop for the trip
        // vehicle positions are only updated when they stop at a stop, so remove vehicles that are in transit to the current stop for the trip
        let current_stopped: HashMap<String, FeedEntity> = vehicle_updates
            .into_iter()
            .filter(|fe| {
                if let Some(latest_stop_id) = latest_trip_stop.get(&fe.trip_id) {
                    *latest_stop_id == fe.stop_id
                } else {
                    false
                }
            })
            .fold(HashMap::new(), |mut acc, fe| {
                acc.insert(fe.trip_id.to_owned(), fe);
                acc
            });

        // queue remove old stops from state
        for prev in self.active_stops.values() {
            if current_stopped.contains_key(&prev.trip_id) == false {
                self.queue
                    .push_back(FeedOp::Remove(prev.trip_id.to_owned()));
            }
        }

        // queue add new stops to state
        for entity in current_stopped.into_values() {
            if self.active_stops.contains_key(&entity.stop_id) == false {
                self.queue.push_back(FeedOp::Add(entity));
            }
        }
    }
}
