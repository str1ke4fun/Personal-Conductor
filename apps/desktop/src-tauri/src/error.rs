use serde::ser::SerializeStruct;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Internal(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AppError {
    fn variant_name(&self) -> &'static str {
        match self {
            AppError::NotFound(_) => "NotFound",
            AppError::Validation(_) => "Validation",
            AppError::Internal(_) => "Internal",
            AppError::Other(_) => "Other",
        }
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Internal(s)
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("type", self.variant_name())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_serialization() {
        let err = AppError::NotFound("item 42 not found".to_string());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["type"], "NotFound");
        assert_eq!(json["message"], "item 42 not found");
    }

    #[test]
    fn validation_serialization() {
        let err = AppError::Validation("field 'name' is required".to_string());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["type"], "Validation");
        assert_eq!(json["message"], "field 'name' is required");
    }

    #[test]
    fn internal_serialization() {
        let err = AppError::Internal("database connection failed".to_string());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["type"], "Internal");
        assert_eq!(json["message"], "database connection failed");
    }

    #[test]
    fn other_serialization() {
        let inner = anyhow::anyhow!("underlying cause");
        let err = AppError::Other(inner);
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["type"], "Other");
        assert_eq!(json["message"], "underlying cause");
    }

    #[test]
    fn from_string_conversion() {
        let err: AppError = "something went wrong".to_string().into();
        assert!(matches!(err, AppError::Internal(ref msg) if msg == "something went wrong"));
    }

    #[test]
    fn from_anyhow_conversion() {
        let inner = anyhow::anyhow!("root cause");
        let err: AppError = inner.into();
        assert!(matches!(err, AppError::Other(_)));
        assert_eq!(err.to_string(), "root cause");
    }
}
