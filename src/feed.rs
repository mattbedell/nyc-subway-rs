use prost::Message;
use reqwest::blocking::Client;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::mpsc::Sender,
};

use crate::{
    entities::{EntityCollection, Route, Stop},
    proto::gtfs::realtime::{vehicle_position::VehicleStopStatus, FeedMessage},
    render::stop::{StopInstance, StopState},
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

struct FeedEntity<'a> {
    stop_id: &'a String,
    route_id: String,
    trip_id: String,
    timestamp: u64,
    color: Option<[f32; 3]>,
}

enum FeedOp<'a> {
    Add(FeedEntity<'a>),
    Remove(String),
}

pub struct FeedManager<'a> {
    client: Client,
    feeds: Vec<FeedProcessor<'a>>,
    feed_idx: usize,
    tx: Sender<Vec<StopInstance>>,
    stops: &'a EntityCollection<BTreeMap<String, Stop>>,
    parent_stops: Vec<&'a String>,
}

struct FeedProcessor<'a> {
    stops: &'a EntityCollection<BTreeMap<String, Stop>>,
    routes: &'a EntityCollection<HashMap<String, Route>>,
    fetched_at: u64,
    queue: VecDeque<FeedOp<'a>>,
    active_stops: HashMap<String, FeedEntity<'a>>,
    active_stops_current: HashMap<String, bool>,
    feed: &'a Feed,
}

impl<'a> FeedManager<'a> {
    pub fn new(
        stops: &'a EntityCollection<BTreeMap<String, Stop>>,
        routes: &'a EntityCollection<HashMap<String, Route>>,
        tx: Sender<Vec<StopInstance>>,
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
                active_stops_current: HashMap::new(),
                feed,
            })
            .collect::<Vec<_>>();

        Self {
            feed_idx: 0,
            client,
            feeds,
            stops,
            parent_stops: stops
                .values()
                .filter_map(|s| {
                    if let None = &s.parent {
                        Some(&s.id)
                    } else {
                        None
                    }
                })
                .collect(),
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
                let mut active_stops: Vec<_> = self
                    .feeds
                    .iter()
                    .flat_map(|feed| feed.active_stops.values())
                    .collect();
                active_stops.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

                let sorted_stops = active_stops
                    .into_iter()
                    .fold(HashMap::new(), |mut acc, fe| {
                        acc.entry(&fe.stop_id).or_insert(fe);
                        acc
                    });

                let mut stateful_instances: Vec<_> = self
                    .parent_stops
                    .iter()
                    .map(|stop_id| {
                        if !sorted_stops.contains_key(stop_id) {
                            let stop = self.stops.get(*stop_id).unwrap();
                            StopState::Inactive(StopInstance {
                                position: [stop.coord.x, stop.coord.y, 0.0],
                                ..Default::default()
                            })
                        } else {
                            let feed_entity = sorted_stops.get(stop_id).unwrap();
                            let stop = self.stops.get(*stop_id).unwrap();
                            StopState::Active(StopInstance {
                                position: [stop.coord.x, stop.coord.y, 0.0],
                                color: feed_entity.color.unwrap(),
                                scale: 0.5,
                            })
                        }
                    })
                    .collect();
                stateful_instances.sort();
                let instances: Vec<_> = stateful_instances
                    .into_iter()
                    .map(StopInstance::from)
                    .collect();

                self.tx.send(instances).unwrap();
                // for (idx, state) in old_state.into_iter().enumerate() {
                //     if !sorted_stops.contains(&idx) && state == true {
                //         self.stops[idx] = false;
                //         self.tx.send(StopState::Inactive(idx)).unwrap();
                //     }

                //     if sorted_stops.contains(&idx) && state == false {
                //         self.stops[idx] = true;
                //         let fe = sorted_stops.get(&idx).unwrap();
                //         self.tx
                //             .send(StopState::Active((idx, fe.color.unwrap())))
                //             .unwrap();
                //     }
                // }
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

                if route.is_none() {
                    return Some(());
                }

                let color = route.unwrap().color();

                feed_entity.color = Some(color);
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

        let mut latest_trip_stop: HashMap<String, &String> = HashMap::new();
        let mut vehicle_updates = Vec::new();
        for entity in msg.entity {
            // get stopped vehicles
            if let Some(vehicle_pos) = entity.vehicle {
                if vehicle_pos.stop_id.is_some() && vehicle_pos.trip.is_some() {
                    if let VehicleStopStatus::StoppedAt = vehicle_pos.current_status() {
                        let trip = vehicle_pos.trip.as_ref().unwrap();
                        let stop_id = vehicle_pos.stop_id().to_owned();
                        // some stops are not public stations and are not part of the static schedule, e.g. R60S, R60N
                        if let Some(stop) = self.stops.get(&stop_id) {
                            let static_stop_id = if let Some(p_stop_id) = &stop.parent {
                                p_stop_id
                            } else {
                                &stop.id
                            };
                            vehicle_updates.push(FeedEntity {
                                trip_id: trip.trip_id().to_owned(),
                                timestamp: vehicle_pos.timestamp(),
                                route_id: trip.route_id().to_owned(),
                                stop_id: static_stop_id,
                                color: None,
                            });
                        }
                    }
                }
            }
            // get the latest stop_time_update for each trip, which contains the next stop being approached or stopped at
            if let Some(trip_update) = entity.trip_update {
                let trip_id = trip_update.trip.trip_id();
                if let Some(stop_update) = trip_update.stop_time_update.first() {
                    let stop_id = stop_update.stop_id();
                    if let Some(stop) = self.stops.get(stop_id) {
                        let static_stop_id = if let Some(p_stop_id) = &stop.parent {
                            p_stop_id
                        } else {
                            &stop.id
                        };

                        latest_trip_stop.insert(trip_id.into(), static_stop_id);
                    }
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
        let current_trips: Vec<_> = self.active_stops_current.keys().map(|k| k.to_owned()).collect();
        for prev in current_trips {
            if current_stopped.contains_key(&prev) == false {
                self.active_stops_current.remove(&prev);
                self.queue
                    .push_back(FeedOp::Remove(prev.to_owned()));
            }
        }

        // queue add new stops to state
        for entity in current_stopped.into_values() {
            if self.active_stops_current.contains_key(&entity.trip_id) == false {
                self.active_stops_current.insert(entity.trip_id.clone(), true);
                self.queue.push_back(FeedOp::Add(entity));
            }
        }
    }
}
