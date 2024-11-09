use geo::{Coord, HaversineBearing, HaversineDistance, Point, Polygon, Rect, LineString};

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

pub fn coord_to_xy(coord: Coord<f32>, centroid: &Point<f32>) -> Coord<f32> {
    let point: Point<f32> = coord.into();
    let distance = centroid.haversine_distance(&point);
    let bearing = centroid.haversine_bearing(point).to_radians();
    let x = distance * bearing.cos();
    let y = distance * bearing.sin();
    Coord { x, y }
}

pub fn circle(coord: Coord, radius: f64) -> Polygon {
    let mut line_coords: Vec<Coord> = Vec::new();

    for deg in (1..=360).step_by(5) {
        let rad = (deg as f64).to_radians();
        let x = coord.x + (radius * rad.cos());
        let y = coord.y + (radius * rad.sin());
        line_coords.push(geo::coord!{ x: x, y: y });
    };
    let first = line_coords.first().unwrap();
    line_coords.push(first.clone());

    Polygon::new(LineString::new(line_coords), vec![])
}
