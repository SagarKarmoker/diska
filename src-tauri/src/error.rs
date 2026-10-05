use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("io error: {0}")]
    PlainIo(#[from] std::io::Error),

    #[error("path is outside the allowed roots: {0}")]
    OutsideAllowedRoots(PathBuf),

    #[error("refusing to delete a protected path: {0}")]
    ProtectedPath(PathBuf),

    #[error("refusing to delete outside the user profile without elevation: {0}")]
    NeedsElevation(PathBuf),

    #[error("path does not exist: {0}")]
    NotFound(PathBuf),

    #[error("scan was cancelled")]
    Cancelled,

    #[error("invalid request: {0}")]
    Invalid(String),
}

impl AppError {
    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Io { .. } | Self::PlainIo(_) => "io",
            Self::OutsideAllowedRoots(_) => "outsideAllowedRoots",
            Self::ProtectedPath(_) => "protectedPath",
            Self::NeedsElevation(_) => "needsElevation",
            Self::NotFound(_) => "notFound",
            Self::Cancelled => "cancelled",
            Self::Invalid(_) => "invalid",
        }
    }
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
