pub mod gtfs {
    pub mod realtime {
        include!(concat!(env!("OUT_DIR"), "/transit_realtime.rs"));
    }
}
