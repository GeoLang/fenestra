//! The two coordinate reference systems Fenestra serves, and the web mercator
//! projection shared by the capabilities documents and the render path.

/// Geographic CRS of every feature the sources return.
pub const EPSG_4326: &str = "EPSG:4326";
/// Projected CRS of the WMTS tile grid and the WMS/WFS alternative.
pub const EPSG_3857: &str = "EPSG:3857";
/// URN form of [`EPSG_4326`], the form WFS 2.0 and WMTS use.
pub const EPSG_4326_URN: &str = "urn:ogc:def:crs:EPSG::4326";
/// URN form of [`EPSG_3857`].
pub const EPSG_3857_URN: &str = "urn:ogc:def:crs:EPSG::3857";

const EARTH_RADIUS: f64 = 6_378_137.0;

/// Latitude beyond which the mercator projection runs off to infinity.
const MERCATOR_LATITUDE_LIMIT: f64 = 89.99;

pub fn lonlat_to_web_mercator(lon: f64, lat: f64) -> [f64; 2] {
    let x = EARTH_RADIUS * lon.to_radians();
    let lat = lat.clamp(-MERCATOR_LATITUDE_LIMIT, MERCATOR_LATITUDE_LIMIT);
    let y = EARTH_RADIUS
        * (std::f64::consts::FRAC_PI_4 + lat.to_radians() / 2.0)
            .tan()
            .ln();
    [x, y]
}

pub fn web_mercator_to_lonlat(x: f64, y: f64) -> [f64; 2] {
    let lon = (x / EARTH_RADIUS).to_degrees();
    let lat = (2.0 * (y / EARTH_RADIUS).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
    [lon, lat]
}

/// Project a `[min_x, min_y, max_x, max_y]` lon/lat box into web mercator.
pub fn bbox_to_web_mercator(bbox: [f64; 4]) -> [f64; 4] {
    let min = lonlat_to_web_mercator(bbox[0], bbox[1]);
    let max = lonlat_to_web_mercator(bbox[2], bbox[3]);
    [min[0], min[1], max[0], max[1]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_mercator_round_trips() {
        let [x, y] = lonlat_to_web_mercator(7.4278, 43.7392);
        let [lon, lat] = web_mercator_to_lonlat(x, y);
        assert!((lon - 7.4278).abs() < 1e-9, "{lon}");
        assert!((lat - 43.7392).abs() < 1e-9, "{lat}");
    }

    #[test]
    fn world_bbox_reaches_the_mercator_extent() {
        let [min_x, _, max_x, _] = bbox_to_web_mercator([-180.0, -85.0, 180.0, 85.0]);
        assert!((min_x + 20_037_508.34).abs() < 1.0, "{min_x}");
        assert!((max_x - 20_037_508.34).abs() < 1.0, "{max_x}");
    }
}
