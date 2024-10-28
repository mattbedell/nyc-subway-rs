use geo::{Coord, Rect, Point, HaversineBearing, HaversineDistance};

pub fn combine_bounding_rect(acc: Rect, rect: Rect) -> Rect {
    let Coord { x: min_x, y: min_y } = acc.min();
    let Coord {
        x: omin_x,
        y: omin_y,
    } = rect.min();
    let Coord { x: max_x, y: max_y } = acc.max();
    let Coord {
        x: omax_x,
        y: omax_y,
    } = rect.max();
    let nmin = Coord {
        x: min_x.min(omin_x),
        y: min_y.min(omin_y),
    };
    let nmax = Coord {
        x: max_x.max(omax_x),
        y: max_y.max(omax_y),
    };
    Rect::new(nmin, nmax)
}

pub fn coord_to_xy(coord: Coord, centroid: Point) -> Coord {
    let point: Point = coord.into();
    let distance = centroid.haversine_distance(&point);
    let bearing = centroid.haversine_bearing(point).to_radians();
    let x = distance * bearing.cos();
    let y = distance * bearing.sin();
    Coord { x, y }
}
