pub mod geopackage;
pub mod geometry;

use crate::config::LayerConfig;
use geometry::{Feature, Geometry};

/// Bounding box: [minx, miny, maxx, maxy]
pub type Bbox = [f64; 4];

/// Trait for spatial data sources.
pub trait DataSource: Send + Sync {
    /// Query features from a layer, optionally filtered by bounding box.
    fn get_features(
        &self,
        layer: &LayerConfig,
        bbox: Option<Bbox>,
        max_features: Option<usize>,
    ) -> Result<Vec<Feature>, DatasourceError>;
}

#[derive(Debug, thiserror::Error)]
pub enum DatasourceError {
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Geometry parse error: {0}")]
    GeometryParseError(String),
    #[error("Table not found: {0}")]
    TableNotFound(String),
}

/// Check if two bounding boxes intersect.
pub fn bbox_intersects(a: &Bbox, b: &Bbox) -> bool {
    a[0] <= b[2] && a[2] >= b[0] && a[1] <= b[3] && a[3] >= b[1]
}

/// Compute the bounding box of a geometry.
pub fn geometry_bbox(geom: &Geometry) -> Option<Bbox> {
    let coords = geom.all_coordinates();
    if coords.is_empty() {
        return None;
    }
    let mut bbox = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for (x, y) in coords {
        bbox[0] = bbox[0].min(x);
        bbox[1] = bbox[1].min(y);
        bbox[2] = bbox[2].max(x);
        bbox[3] = bbox[3].max(y);
    }
    Some(bbox)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bbox_intersects() {
        assert!(bbox_intersects(
            &[0.0, 0.0, 10.0, 10.0],
            &[5.0, 5.0, 15.0, 15.0]
        ));
        assert!(!bbox_intersects(
            &[0.0, 0.0, 5.0, 5.0],
            &[6.0, 6.0, 10.0, 10.0]
        ));
        assert!(bbox_intersects(
            &[0.0, 0.0, 10.0, 10.0],
            &[0.0, 0.0, 10.0, 10.0]
        ));
    }

    #[test]
    fn test_geometry_bbox() {
        let geom = Geometry::Point(5.0, 10.0);
        let bbox = geometry_bbox(&geom).unwrap();
        assert_eq!(bbox, [5.0, 10.0, 5.0, 10.0]);

        let geom = Geometry::LineString(vec![(0.0, 0.0), (10.0, 5.0), (3.0, 8.0)]);
        let bbox = geometry_bbox(&geom).unwrap();
        assert_eq!(bbox, [0.0, 0.0, 10.0, 8.0]);
    }
}
