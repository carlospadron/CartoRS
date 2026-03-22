use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::xml::xml_escape;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Service not enabled: {0}")]
    ServiceNotEnabled(String),
    #[error("Missing required parameter: {0}")]
    MissingParameter(String),
    #[error("Invalid parameter value for '{0}': {1}")]
    InvalidParameter(String, String),
    #[error("Layer not found: {0}")]
    LayerNotFound(String),
    #[error("Datasource error: {0}")]
    DatasourceError(String),
    #[error("Rendering error: {0}")]
    RenderingError(String),
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            AppError::ServiceNotEnabled(_) => (StatusCode::FORBIDDEN, self.to_string()),
            AppError::MissingParameter(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::InvalidParameter(_, _) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::LayerNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::DatasourceError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
            }
            AppError::RenderingError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
            }
            AppError::UnsupportedOperation(_) => (StatusCode::BAD_REQUEST, self.to_string()),
        };

        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ServiceExceptionReport version="1.3.0">
  <ServiceException code="{}">{}</ServiceException>
</ServiceExceptionReport>"#,
            status.as_u16(),
            xml_escape(&body)
        );

        (
            status,
            [("Content-Type", "application/xml")],
            xml,
        )
            .into_response()
    }
}
