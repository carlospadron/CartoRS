use rusqlite::Connection;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::Path;

use crate::config::LayerConfig;
use super::geometry::{parse_gpkg_binary, Feature};
use super::{bbox_intersects, geometry_bbox, Bbox, DataSource, DatasourceError};

pub struct GeoPackageSource {
    path: String,
}

impl GeoPackageSource {
    pub fn new(path: &str) -> Result<Self, DatasourceError> {
        // Verify the file exists
        if !Path::new(path).exists() {
            return Err(DatasourceError::DatabaseError(format!(
                "GeoPackage file not found: {}",
                path
            )));
        }
        Ok(Self {
            path: path.to_string(),
        })
    }

    fn open_connection(&self) -> Result<Connection, DatasourceError> {
        Connection::open(&self.path)
            .map_err(|e| DatasourceError::DatabaseError(format!("Failed to open GeoPackage: {}", e)))
    }
}

impl DataSource for GeoPackageSource {
    fn get_features(
        &self,
        layer: &LayerConfig,
        bbox: Option<Bbox>,
        max_features: Option<usize>,
    ) -> Result<Vec<Feature>, DatasourceError> {
        let conn = self.open_connection()?;

        // Verify table exists
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                [&layer.table],
                |row| row.get(0),
            )
            .map_err(|e| DatasourceError::DatabaseError(e.to_string()))?;

        if !table_exists {
            return Err(DatasourceError::TableNotFound(layer.table.clone()));
        }

        // Get column names for the table (excluding geometry column)
        let column_names = get_column_names(&conn, &layer.table, &layer.geometry_column)?;

        // Build query - LIMIT is safe to interpolate directly since max_features is a usize
        let limit_clause = max_features
            .map(|n| format!(" LIMIT {}", n))
            .unwrap_or_default();
        let query = format!(
            "SELECT {}, {} FROM {}{}",
            column_names.join(", "),
            layer.geometry_column,
            layer.table,
            limit_clause
        );

        let mut stmt = conn
            .prepare(&query)
            .map_err(|e| DatasourceError::DatabaseError(e.to_string()))?;

        let geom_idx = column_names.len();
        let rows = stmt
            .query_map([], |row| {
                // Read properties
                let mut properties = HashMap::new();
                for (i, col_name) in column_names.iter().enumerate() {
                    let value: JsonValue = match row.get_ref(i) {
                        Ok(rusqlite::types::ValueRef::Null) => JsonValue::Null,
                        Ok(rusqlite::types::ValueRef::Integer(v)) => JsonValue::from(v),
                        Ok(rusqlite::types::ValueRef::Real(v)) => {
                            serde_json::Number::from_f64(v)
                                .map(JsonValue::Number)
                                .unwrap_or(JsonValue::Null)
                        }
                        Ok(rusqlite::types::ValueRef::Text(v)) => {
                            JsonValue::String(String::from_utf8_lossy(v).to_string())
                        }
                        Ok(rusqlite::types::ValueRef::Blob(_)) => {
                            JsonValue::String("[binary]".to_string())
                        }
                        Err(_) => JsonValue::Null,
                    };
                    properties.insert(col_name.clone(), value);
                }

                // Read geometry blob
                let geom_blob: Vec<u8> = row.get(geom_idx)?;
                Ok((properties, geom_blob))
            })
            .map_err(|e| DatasourceError::DatabaseError(e.to_string()))?;

        let mut features = Vec::new();
        for row_result in rows {
            let (properties, geom_blob) = row_result
                .map_err(|e| DatasourceError::DatabaseError(e.to_string()))?;

            let geometry = parse_gpkg_binary(&geom_blob)
                .map_err(|e| DatasourceError::GeometryParseError(e))?;

            // Apply bbox filter if specified
            if let Some(ref filter_bbox) = bbox {
                if let Some(geom_bbox) = geometry_bbox(&geometry) {
                    if !bbox_intersects(filter_bbox, &geom_bbox) {
                        continue;
                    }
                }
            }

            features.push(Feature {
                geometry,
                properties,
            });
        }

        Ok(features)
    }
}

fn get_column_names(
    conn: &Connection,
    table: &str,
    geometry_column: &str,
) -> Result<Vec<String>, DatasourceError> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .map_err(|e| DatasourceError::DatabaseError(e.to_string()))?;

    let names: Vec<String> = stmt
        .query_map([], |row| {
            let name: String = row.get(1)?;
            Ok(name)
        })
        .map_err(|e| DatasourceError::DatabaseError(e.to_string()))?
        .filter_map(|r| r.ok())
        .filter(|name| name != geometry_column)
        .collect();

    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_gpkg(path: &str) {
        let conn = Connection::open(path).unwrap();

        // Create GeoPackage metadata tables
        conn.execute_batch(
            "CREATE TABLE gpkg_contents (
                table_name TEXT NOT NULL PRIMARY KEY,
                data_type TEXT NOT NULL,
                identifier TEXT,
                description TEXT DEFAULT '',
                last_change TEXT,
                min_x DOUBLE,
                min_y DOUBLE,
                max_x DOUBLE,
                max_y DOUBLE,
                srs_id INTEGER
            );
            CREATE TABLE gpkg_geometry_columns (
                table_name TEXT NOT NULL,
                column_name TEXT NOT NULL,
                geometry_type_name TEXT NOT NULL,
                srs_id INTEGER NOT NULL,
                z TINYINT NOT NULL,
                m TINYINT NOT NULL
            );"
        ).unwrap();

        // Create a test feature table
        conn.execute_batch(
            "CREATE TABLE test_points (
                fid INTEGER PRIMARY KEY,
                name TEXT,
                value REAL,
                geom BLOB
            );"
        ).unwrap();

        // Insert test data with WKB point geometries wrapped in GPB
        let points = vec![
            ("Point A", 1.0, 5.0, 10.0),
            ("Point B", 2.0, 15.0, 20.0),
            ("Point C", 3.0, 25.0, 30.0),
        ];

        for (name, value, x, y) in points {
            let gpb = make_test_gpb_point(x, y);
            conn.execute(
                "INSERT INTO test_points (name, value, geom) VALUES (?1, ?2, ?3)",
                rusqlite::params![name, value, gpb],
            ).unwrap();
        }

        // Register in gpkg_contents
        conn.execute(
            "INSERT INTO gpkg_contents (table_name, data_type, srs_id) VALUES ('test_points', 'features', 4326)",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO gpkg_geometry_columns (table_name, column_name, geometry_type_name, srs_id, z, m) VALUES ('test_points', 'geom', 'POINT', 4326, 0, 0)",
            [],
        ).unwrap();
    }

    fn make_test_gpb_point(x: f64, y: f64) -> Vec<u8> {
        let mut data = Vec::new();
        // GPB header
        data.push(0x47); // 'G'
        data.push(0x50); // 'P'
        data.push(0);    // version
        data.push(0);    // flags: little endian, no envelope
        data.extend_from_slice(&4326u32.to_le_bytes()); // SRS ID
        // WKB Point
        data.push(1); // little endian
        data.extend_from_slice(&1u32.to_le_bytes()); // Point type
        data.extend_from_slice(&x.to_le_bytes());
        data.extend_from_slice(&y.to_le_bytes());
        data
    }

    #[test]
    fn test_geopackage_read_features() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.gpkg");
        let path_str = path.to_str().unwrap();
        create_test_gpkg(path_str);

        let source = GeoPackageSource::new(path_str).unwrap();
        let layer = LayerConfig {
            name: "test".to_string(),
            title: "Test".to_string(),
            datasource: "testds".to_string(),
            table: "test_points".to_string(),
            srs: "EPSG:4326".to_string(),
            geometry_column: "geom".to_string(),
            bbox: [-180.0, -90.0, 180.0, 90.0],
        };

        let features = source.get_features(&layer, None, None).unwrap();
        assert_eq!(features.len(), 3);
        assert_eq!(features[0].properties["name"], "Point A");
    }

    #[test]
    fn test_geopackage_bbox_filter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.gpkg");
        let path_str = path.to_str().unwrap();
        create_test_gpkg(path_str);

        let source = GeoPackageSource::new(path_str).unwrap();
        let layer = LayerConfig {
            name: "test".to_string(),
            title: "Test".to_string(),
            datasource: "testds".to_string(),
            table: "test_points".to_string(),
            srs: "EPSG:4326".to_string(),
            geometry_column: "geom".to_string(),
            bbox: [-180.0, -90.0, 180.0, 90.0],
        };

        // Filter to only include Point A (5.0, 10.0) and Point B (15.0, 20.0)
        let bbox = [0.0, 0.0, 20.0, 25.0];
        let features = source.get_features(&layer, Some(bbox), None).unwrap();
        assert_eq!(features.len(), 2);
    }

    #[test]
    fn test_geopackage_max_features() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.gpkg");
        let path_str = path.to_str().unwrap();
        create_test_gpkg(path_str);

        let source = GeoPackageSource::new(path_str).unwrap();
        let layer = LayerConfig {
            name: "test".to_string(),
            title: "Test".to_string(),
            datasource: "testds".to_string(),
            table: "test_points".to_string(),
            srs: "EPSG:4326".to_string(),
            geometry_column: "geom".to_string(),
            bbox: [-180.0, -90.0, 180.0, 90.0],
        };

        let features = source.get_features(&layer, None, Some(2)).unwrap();
        assert_eq!(features.len(), 2);
    }

    #[test]
    fn test_geopackage_table_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.gpkg");
        let path_str = path.to_str().unwrap();
        create_test_gpkg(path_str);

        let source = GeoPackageSource::new(path_str).unwrap();
        let layer = LayerConfig {
            name: "test".to_string(),
            title: "Test".to_string(),
            datasource: "testds".to_string(),
            table: "nonexistent".to_string(),
            srs: "EPSG:4326".to_string(),
            geometry_column: "geom".to_string(),
            bbox: [-180.0, -90.0, 180.0, 90.0],
        };

        let result = source.get_features(&layer, None, None);
        assert!(result.is_err());
    }
}
