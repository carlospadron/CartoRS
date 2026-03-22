use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;

use crate::error::AppError;
use crate::rendering::render_map;
use crate::server::AppState;

/// Handle WMS requests (dispatches based on REQUEST parameter).
pub async fn handle_wms(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    if !state.config.services.wms.enabled {
        return Err(AppError::ServiceNotEnabled("WMS".to_string()));
    }

    let request = params
        .get("REQUEST")
        .or_else(|| params.get("request"))
        .ok_or_else(|| AppError::MissingParameter("REQUEST".to_string()))?;

    match request.to_uppercase().as_str() {
        "GETCAPABILITIES" => get_capabilities(state).await,
        "GETMAP" => get_map(state, params).await,
        other => Err(AppError::UnsupportedOperation(format!(
            "WMS operation '{}' is not supported",
            other
        ))),
    }
}

async fn get_capabilities(state: AppState) -> Result<Response, AppError> {
    let config = &state.config;
    let layers_xml: String = config
        .layers
        .iter()
        .map(|layer| {
            format!(
                r#"      <Layer queryable="1">
        <Name>{name}</Name>
        <Title>{title}</Title>
        <CRS>{srs}</CRS>
        <EX_GeographicBoundingBox>
          <westBoundLongitude>{minx}</westBoundLongitude>
          <eastBoundLongitude>{maxx}</eastBoundLongitude>
          <southBoundLatitude>{miny}</southBoundLatitude>
          <northBoundLatitude>{maxy}</northBoundLatitude>
        </EX_GeographicBoundingBox>
        <BoundingBox CRS="{srs}" minx="{minx}" miny="{miny}" maxx="{maxx}" maxy="{maxy}"/>
      </Layer>"#,
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
<WMS_Capabilities version="1.3.0" xmlns="http://www.opengis.net/wms"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
  xsi:schemaLocation="http://www.opengis.net/wms http://schemas.opengis.net/wms/1.3.0/capabilities_1_3_0.xsd">
  <Service>
    <Name>WMS</Name>
    <Title>{title}</Title>
  </Service>
  <Capability>
    <Request>
      <GetCapabilities>
        <Format>text/xml</Format>
      </GetCapabilities>
      <GetMap>
        <Format>image/png</Format>
      </GetMap>
    </Request>
    <Exception>
      <Format>XML</Format>
    </Exception>
    <Layer>
      <Title>{title}</Title>
{layers}
    </Layer>
  </Capability>
</WMS_Capabilities>"#,
        title = xml_escape(&config.services.wms.title),
        layers = layers_xml,
    );

    Ok((
        [(header::CONTENT_TYPE, "application/xml")],
        xml,
    )
        .into_response())
}

async fn get_map(
    state: AppState,
    params: HashMap<String, String>,
) -> Result<Response, AppError> {
    // Parse required parameters (case-insensitive lookup)
    let get_param = |name: &str| -> Result<String, AppError> {
        params
            .get(name)
            .or_else(|| params.get(&name.to_uppercase()))
            .or_else(|| params.get(&name.to_lowercase()))
            .cloned()
            .ok_or_else(|| AppError::MissingParameter(name.to_string()))
    };

    let layers_param = get_param("LAYERS")?;
    let width: u32 = get_param("WIDTH")?
        .parse()
        .map_err(|_| AppError::InvalidParameter("WIDTH".to_string(), "must be a positive integer".to_string()))?;
    let height: u32 = get_param("HEIGHT")?
        .parse()
        .map_err(|_| AppError::InvalidParameter("HEIGHT".to_string(), "must be a positive integer".to_string()))?;
    let bbox_str = get_param("BBOX")?;

    // Validate dimensions
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return Err(AppError::InvalidParameter(
            "WIDTH/HEIGHT".to_string(),
            "must be between 1 and 4096".to_string(),
        ));
    }

    // Parse BBOX
    let bbox_parts: Vec<f64> = bbox_str
        .split(',')
        .map(|s| s.trim().parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            AppError::InvalidParameter(
                "BBOX".to_string(),
                "must be four comma-separated numbers".to_string(),
            )
        })?;

    if bbox_parts.len() != 4 {
        return Err(AppError::InvalidParameter(
            "BBOX".to_string(),
            "must be four comma-separated numbers".to_string(),
        ));
    }
    let bbox = [bbox_parts[0], bbox_parts[1], bbox_parts[2], bbox_parts[3]];

    // Collect features from all requested layers
    let layer_names: Vec<&str> = layers_param.split(',').map(|s| s.trim()).collect();
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
            .get_features(layer_config, Some(bbox), None)
            .map_err(|e| AppError::DatasourceError(e.to_string()))?;

        all_features.extend(features);
    }

    // Render the map
    let png_data = render_map(&all_features, &bbox, width, height)
        .map_err(|e| AppError::RenderingError(e))?;

    Ok((
        [(header::CONTENT_TYPE, "image/png")],
        png_data,
    )
        .into_response())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("hello & world"), "hello &amp; world");
        assert_eq!(xml_escape("<tag>"), "&lt;tag&gt;");
    }
}
