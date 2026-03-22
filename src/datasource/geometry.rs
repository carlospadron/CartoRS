use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Simple geometry types for spatial data.
#[derive(Debug, Clone)]
pub enum Geometry {
    Point(f64, f64),
    LineString(Vec<(f64, f64)>),
    Polygon(Vec<Vec<(f64, f64)>>),
    MultiPoint(Vec<(f64, f64)>),
    MultiLineString(Vec<Vec<(f64, f64)>>),
    MultiPolygon(Vec<Vec<Vec<(f64, f64)>>>),
}

impl Geometry {
    /// Return all coordinates in this geometry as a flat list.
    pub fn all_coordinates(&self) -> Vec<(f64, f64)> {
        match self {
            Geometry::Point(x, y) => vec![(*x, *y)],
            Geometry::LineString(coords) => coords.clone(),
            Geometry::Polygon(rings) => rings.iter().flat_map(|r| r.iter().copied()).collect(),
            Geometry::MultiPoint(pts) => pts.clone(),
            Geometry::MultiLineString(lines) => {
                lines.iter().flat_map(|l| l.iter().copied()).collect()
            }
            Geometry::MultiPolygon(polys) => polys
                .iter()
                .flat_map(|p| p.iter().flat_map(|r| r.iter().copied()))
                .collect(),
        }
    }

    /// Convert geometry to GeoJSON value.
    pub fn to_geojson(&self) -> JsonValue {
        match self {
            Geometry::Point(x, y) => serde_json::json!({
                "type": "Point",
                "coordinates": [x, y]
            }),
            Geometry::LineString(coords) => serde_json::json!({
                "type": "LineString",
                "coordinates": coords.iter().map(|(x, y)| vec![*x, *y]).collect::<Vec<_>>()
            }),
            Geometry::Polygon(rings) => serde_json::json!({
                "type": "Polygon",
                "coordinates": rings.iter().map(|ring|
                    ring.iter().map(|(x, y)| vec![*x, *y]).collect::<Vec<_>>()
                ).collect::<Vec<_>>()
            }),
            Geometry::MultiPoint(pts) => serde_json::json!({
                "type": "MultiPoint",
                "coordinates": pts.iter().map(|(x, y)| vec![*x, *y]).collect::<Vec<_>>()
            }),
            Geometry::MultiLineString(lines) => serde_json::json!({
                "type": "MultiLineString",
                "coordinates": lines.iter().map(|line|
                    line.iter().map(|(x, y)| vec![*x, *y]).collect::<Vec<_>>()
                ).collect::<Vec<_>>()
            }),
            Geometry::MultiPolygon(polys) => serde_json::json!({
                "type": "MultiPolygon",
                "coordinates": polys.iter().map(|poly|
                    poly.iter().map(|ring|
                        ring.iter().map(|(x, y)| vec![*x, *y]).collect::<Vec<_>>()
                    ).collect::<Vec<_>>()
                ).collect::<Vec<_>>()
            }),
        }
    }
}

/// A spatial feature with geometry and properties.
#[derive(Debug, Clone)]
pub struct Feature {
    pub geometry: Geometry,
    pub properties: HashMap<String, JsonValue>,
}

impl Feature {
    pub fn to_geojson(&self) -> JsonValue {
        serde_json::json!({
            "type": "Feature",
            "geometry": self.geometry.to_geojson(),
            "properties": self.properties
        })
    }
}

/// Parse WKB (Well-Known Binary) geometry from bytes.
pub fn parse_wkb(data: &[u8]) -> Result<Geometry, String> {
    if data.is_empty() {
        return Err("Empty WKB data".to_string());
    }
    let mut cursor = 0;
    parse_wkb_geometry(data, &mut cursor)
}

/// Parse GeoPackage Binary geometry (GPB format).
pub fn parse_gpkg_binary(data: &[u8]) -> Result<Geometry, String> {
    if data.len() < 8 {
        return Err("GeoPackage binary data too short".to_string());
    }

    // Check magic number "GP"
    if data[0] != 0x47 || data[1] != 0x50 {
        // Not GPB format, try parsing as raw WKB
        return parse_wkb(data);
    }

    let _version = data[2];
    let flags = data[3];

    // Envelope type is bits 1-3 of flags
    let envelope_type = (flags >> 1) & 0x07;
    let envelope_size = match envelope_type {
        0 => 0,        // no envelope
        1 => 32,       // [minx, maxx, miny, maxy]
        2 => 48,       // + [minz, maxz]
        3 => 48,       // + [minm, maxm]
        4 => 64,       // + [minz, maxz, minm, maxm]
        _ => return Err(format!("Unknown GPB envelope type: {}", envelope_type)),
    };

    // SRS ID is 4 bytes after flags
    let wkb_offset = 8 + envelope_size;
    if data.len() < wkb_offset {
        return Err("GeoPackage binary data truncated before WKB".to_string());
    }

    parse_wkb(&data[wkb_offset..])
}

fn parse_wkb_geometry(data: &[u8], cursor: &mut usize) -> Result<Geometry, String> {
    if *cursor >= data.len() {
        return Err("Unexpected end of WKB data".to_string());
    }

    let byte_order = data[*cursor];
    *cursor += 1;
    let is_little_endian = byte_order == 1;

    let geom_type = read_u32(data, cursor, is_little_endian)?;

    // Mask out Z/M/SRID flags to get the base type
    let base_type = geom_type & 0xFF;

    match base_type {
        1 => parse_wkb_point(data, cursor, is_little_endian),
        2 => parse_wkb_linestring(data, cursor, is_little_endian),
        3 => parse_wkb_polygon(data, cursor, is_little_endian),
        4 => parse_wkb_multipoint(data, cursor, is_little_endian),
        5 => parse_wkb_multilinestring(data, cursor, is_little_endian),
        6 => parse_wkb_multipolygon(data, cursor, is_little_endian),
        _ => Err(format!("Unsupported WKB geometry type: {}", geom_type)),
    }
}

fn parse_wkb_point(
    data: &[u8],
    cursor: &mut usize,
    little_endian: bool,
) -> Result<Geometry, String> {
    let x = read_f64(data, cursor, little_endian)?;
    let y = read_f64(data, cursor, little_endian)?;
    Ok(Geometry::Point(x, y))
}

fn parse_wkb_linestring(
    data: &[u8],
    cursor: &mut usize,
    little_endian: bool,
) -> Result<Geometry, String> {
    let num_points = read_u32(data, cursor, little_endian)? as usize;
    let mut coords = Vec::with_capacity(num_points);
    for _ in 0..num_points {
        let x = read_f64(data, cursor, little_endian)?;
        let y = read_f64(data, cursor, little_endian)?;
        coords.push((x, y));
    }
    Ok(Geometry::LineString(coords))
}

fn parse_wkb_polygon(
    data: &[u8],
    cursor: &mut usize,
    little_endian: bool,
) -> Result<Geometry, String> {
    let num_rings = read_u32(data, cursor, little_endian)? as usize;
    let mut rings = Vec::with_capacity(num_rings);
    for _ in 0..num_rings {
        let num_points = read_u32(data, cursor, little_endian)? as usize;
        let mut coords = Vec::with_capacity(num_points);
        for _ in 0..num_points {
            let x = read_f64(data, cursor, little_endian)?;
            let y = read_f64(data, cursor, little_endian)?;
            coords.push((x, y));
        }
        rings.push(coords);
    }
    Ok(Geometry::Polygon(rings))
}

fn parse_wkb_multipoint(
    data: &[u8],
    cursor: &mut usize,
    little_endian: bool,
) -> Result<Geometry, String> {
    let num_geoms = read_u32(data, cursor, little_endian)? as usize;
    let mut points = Vec::with_capacity(num_geoms);
    for _ in 0..num_geoms {
        match parse_wkb_geometry(data, cursor)? {
            Geometry::Point(x, y) => points.push((x, y)),
            _ => return Err("Expected Point in MultiPoint".to_string()),
        }
    }
    Ok(Geometry::MultiPoint(points))
}

fn parse_wkb_multilinestring(
    data: &[u8],
    cursor: &mut usize,
    little_endian: bool,
) -> Result<Geometry, String> {
    let num_geoms = read_u32(data, cursor, little_endian)? as usize;
    let mut lines = Vec::with_capacity(num_geoms);
    for _ in 0..num_geoms {
        match parse_wkb_geometry(data, cursor)? {
            Geometry::LineString(coords) => lines.push(coords),
            _ => return Err("Expected LineString in MultiLineString".to_string()),
        }
    }
    Ok(Geometry::MultiLineString(lines))
}

fn parse_wkb_multipolygon(
    data: &[u8],
    cursor: &mut usize,
    little_endian: bool,
) -> Result<Geometry, String> {
    let num_geoms = read_u32(data, cursor, little_endian)? as usize;
    let mut polys = Vec::with_capacity(num_geoms);
    for _ in 0..num_geoms {
        match parse_wkb_geometry(data, cursor)? {
            Geometry::Polygon(rings) => polys.push(rings),
            _ => return Err("Expected Polygon in MultiPolygon".to_string()),
        }
    }
    Ok(Geometry::MultiPolygon(polys))
}

fn read_u32(data: &[u8], cursor: &mut usize, little_endian: bool) -> Result<u32, String> {
    if *cursor + 4 > data.len() {
        return Err("Unexpected end of data reading u32".to_string());
    }
    let bytes: [u8; 4] = data[*cursor..*cursor + 4]
        .try_into()
        .map_err(|_| "Failed to read u32 bytes".to_string())?;
    *cursor += 4;
    Ok(if little_endian {
        u32::from_le_bytes(bytes)
    } else {
        u32::from_be_bytes(bytes)
    })
}

fn read_f64(data: &[u8], cursor: &mut usize, little_endian: bool) -> Result<f64, String> {
    if *cursor + 8 > data.len() {
        return Err("Unexpected end of data reading f64".to_string());
    }
    let bytes: [u8; 8] = data[*cursor..*cursor + 8]
        .try_into()
        .map_err(|_| "Failed to read f64 bytes".to_string())?;
    *cursor += 8;
    Ok(if little_endian {
        f64::from_le_bytes(bytes)
    } else {
        f64::from_be_bytes(bytes)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_wkb_point(x: f64, y: f64) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(1); // little endian
        data.extend_from_slice(&1u32.to_le_bytes()); // Point type
        data.extend_from_slice(&x.to_le_bytes());
        data.extend_from_slice(&y.to_le_bytes());
        data
    }

    fn make_wkb_linestring(coords: &[(f64, f64)]) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(1); // little endian
        data.extend_from_slice(&2u32.to_le_bytes()); // LineString type
        data.extend_from_slice(&(coords.len() as u32).to_le_bytes());
        for (x, y) in coords {
            data.extend_from_slice(&x.to_le_bytes());
            data.extend_from_slice(&y.to_le_bytes());
        }
        data
    }

    #[test]
    fn test_parse_wkb_point() {
        let wkb = make_wkb_point(1.5, 2.5);
        let geom = parse_wkb(&wkb).unwrap();
        match geom {
            Geometry::Point(x, y) => {
                assert!((x - 1.5).abs() < f64::EPSILON);
                assert!((y - 2.5).abs() < f64::EPSILON);
            }
            _ => panic!("Expected Point"),
        }
    }

    #[test]
    fn test_parse_wkb_linestring() {
        let coords = vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)];
        let wkb = make_wkb_linestring(&coords);
        let geom = parse_wkb(&wkb).unwrap();
        match geom {
            Geometry::LineString(parsed) => {
                assert_eq!(parsed.len(), 3);
                assert!((parsed[0].0).abs() < f64::EPSILON);
                assert!((parsed[1].0 - 1.0).abs() < f64::EPSILON);
            }
            _ => panic!("Expected LineString"),
        }
    }

    #[test]
    fn test_parse_gpkg_binary_point() {
        // Build a GPB with no envelope wrapping a WKB point
        let mut gpb = Vec::new();
        gpb.push(0x47); // 'G'
        gpb.push(0x50); // 'P'
        gpb.push(0);    // version
        gpb.push(0);    // flags: little endian, no envelope
        gpb.extend_from_slice(&4326u32.to_le_bytes()); // SRS ID
        gpb.extend_from_slice(&make_wkb_point(10.0, 20.0));
        let geom = parse_gpkg_binary(&gpb).unwrap();
        match geom {
            Geometry::Point(x, y) => {
                assert!((x - 10.0).abs() < f64::EPSILON);
                assert!((y - 20.0).abs() < f64::EPSILON);
            }
            _ => panic!("Expected Point"),
        }
    }

    #[test]
    fn test_point_to_geojson() {
        let geom = Geometry::Point(1.0, 2.0);
        let json = geom.to_geojson();
        assert_eq!(json["type"], "Point");
        assert_eq!(json["coordinates"][0], 1.0);
        assert_eq!(json["coordinates"][1], 2.0);
    }

    #[test]
    fn test_feature_to_geojson() {
        let mut props = HashMap::new();
        props.insert("name".to_string(), serde_json::json!("Test"));
        let feature = Feature {
            geometry: Geometry::Point(1.0, 2.0),
            properties: props,
        };
        let json = feature.to_geojson();
        assert_eq!(json["type"], "Feature");
        assert_eq!(json["properties"]["name"], "Test");
        assert_eq!(json["geometry"]["type"], "Point");
    }
}
