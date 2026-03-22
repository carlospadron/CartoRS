use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;

use crate::error::AppError;
use crate::server::AppState;
use crate::xml::xml_escape;

/// Handle WFS requests (dispatches based on REQUEST parameter).
pub async fn handle_wfs(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    if !state.config.services.wfs.enabled {
        return Err(AppError::ServiceNotEnabled("WFS".to_string()));
    }

    let request = params
        .get("REQUEST")
        .or_else(|| params.get("request"))
        .ok_or_else(|| AppError::MissingParameter("REQUEST".to_string()))?;

    match request.to_uppercase().as_str() {
        "GETCAPABILITIES" => get_capabilities(state).await,
        "GETFEATURE" => get_feature(state, params).await,
        other => Err(AppError::UnsupportedOperation(format!(
            "WFS operation '{}' is not supported",
            other
        ))),
    }
}

async fn get_capabilities(state: AppState) -> Result<Response, AppError> {
    let config = &state.config;
    let feature_types_xml: String = config
        .layers
        .iter()
        .map(|layer| {
            format!(
                r#"    <FeatureType>
      <Name>{name}</Name>
      <Title>{title}</Title>
      <DefaultCRS>{srs}</DefaultCRS>
      <ows:WGS84BoundingBox>
        <ows:LowerCorner>{minx} {miny}</ows:LowerCorner>
        <ows:UpperCorner>{maxx} {maxy}</ows:UpperCorner>
      </ows:WGS84BoundingBox>
    </FeatureType>"#,
                name = xml_escape(&layer.name),
                title = xml_escape(&layer.title),
                srs = xml_escape(&layer.srs),
                minx = layer.bbox[0],
                miny = layer.bbox[1],
                maxx = layer.bbox[2],
                maxy = layer.bbox[3],
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<wfs:WFS_Capabilities version="2.0.0"
  xmlns:wfs="http://www.opengis.net/wfs/2.0"
  xmlns:ows="http://www.opengis.net/ows/1.1"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
  xsi:schemaLocation="http://www.opengis.net/wfs/2.0 http://schemas.opengis.net/wfs/2.0/wfs.xsd">
  <ows:ServiceIdentification>
    <ows:Title>{title}</ows:Title>
    <ows:ServiceType>WFS</ows:ServiceType>
    <ows:ServiceTypeVersion>2.0.0</ows:ServiceTypeVersion>
  </ows:ServiceIdentification>
  <ows:OperationsMetadata>
    <ows:Operation name="GetCapabilities"/>
    <ows:Operation name="GetFeature"/>
  </ows:OperationsMetadata>
  <FeatureTypeList>
{feature_types}
  </FeatureTypeList>
</wfs:WFS_Capabilities>"#,
        title = xml_escape(&config.services.wfs.title),
        feature_types = feature_types_xml,
    );

    Ok((
        [(header::CONTENT_TYPE, "application/xml")],
        xml,
    )
        .into_response())
}

async fn get_feature(
    state: AppState,
    params: HashMap<String, String>,
) -> Result<Response, AppError> {
    let get_param = |name: &str| -> Option<String> {
        params
            .get(name)
            .or_else(|| params.get(&name.to_uppercase()))
            .or_else(|| params.get(&name.to_lowercase()))
            .cloned()
    };

    // TYPENAMES (WFS 2.0) or TYPENAME (WFS 1.1)
    let type_names = get_param("TYPENAMES")
        .or_else(|| get_param("TYPENAME"))
        .ok_or_else(|| AppError::MissingParameter("TYPENAMES".to_string()))?;

    let max_features = get_param("COUNT")
        .or_else(|| get_param("MAXFEATURES"))
        .and_then(|v| v.parse::<usize>().ok());

    // Parse optional BBOX
    let bbox = get_param("BBOX").and_then(|bbox_str| {
        let parts: Vec<f64> = bbox_str
            .split(',')
            .filter_map(|s| s.trim().parse::<f64>().ok())
            .collect();
        if parts.len() >= 4 {
            Some([parts[0], parts[1], parts[2], parts[3]])
        } else {
            None
        }
    });

    // Collect features from all requested type names
    let layer_names: Vec<&str> = type_names.split(',').map(|s| s.trim()).collect();
    let mut all_features = Vec::new();

    for layer_name in &layer_names {
        let layer_config = state
            .config
            .layers
            .iter()
            .find(|l| l.name == *layer_name)
            .ok_or_else(|| AppError::LayerNotFound(layer_name.to_string()))?;

        let datasource = state
            .datasources
            .get(&layer_config.datasource)
            .ok_or_else(|| {
                AppError::DatasourceError(format!(
                    "Datasource '{}' not initialized",
                    layer_config.datasource
                ))
            })?;

        let features = datasource
            .get_features(layer_config, bbox, max_features)
            .map_err(|e| AppError::DatasourceError(e.to_string()))?;

        all_features.extend(features);
    }

    // Build GeoJSON FeatureCollection
    let geojson = serde_json::json!({
        "type": "FeatureCollection",
        "features": all_features.iter().map(|f| f.to_geojson()).collect::<Vec<_>>(),
        "numberReturned": all_features.len(),
    });

    Ok((
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&geojson)
            .map_err(|e| AppError::DatasourceError(format!("JSON serialization error: {}", e)))?,
    )
        .into_response())
}


#[cfg(test)]
mod tests {
    use crate::xml::xml_escape;

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("test & value"), "test &amp; value");
    }
}
