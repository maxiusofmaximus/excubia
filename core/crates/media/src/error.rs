use image::error::LimitErrorKind;
use image::ImageError;
use thiserror::Error;

/// Why an image was refused. `code` is the stable string the API exposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MediaError {
    #[error("the file is not a PNG, JPEG or static WebP image")]
    UnsupportedType,
    #[error("the image data is corrupt or cannot be decoded")]
    InvalidImage,
    #[error("the image has too many pixels")]
    ImageTooLarge,
    #[error("the image is too wide or too tall")]
    DimensionsTooLarge,
    #[error("the input is larger than the allowed size")]
    InputTooLarge,
    #[error("processing the image took too long")]
    Timeout,
    #[error("the image worker failed")]
    WorkerFailed,
}

impl MediaError {
    pub fn code(self) -> &'static str {
        match self {
            Self::UnsupportedType => "unsupported_type",
            Self::InvalidImage => "invalid_image",
            Self::ImageTooLarge => "image_too_large",
            Self::DimensionsTooLarge => "dimensions_too_large",
            Self::InputTooLarge => "input_too_large",
            Self::Timeout => "timeout",
            Self::WorkerFailed => "worker_failed",
        }
    }

    /// Maps a code from the worker's reply back to the error. Unknown codes are a worker fault.
    pub(crate) fn from_code(code: &str) -> Self {
        match code {
            "unsupported_type" => Self::UnsupportedType,
            "invalid_image" => Self::InvalidImage,
            "image_too_large" => Self::ImageTooLarge,
            "dimensions_too_large" => Self::DimensionsTooLarge,
            "input_too_large" => Self::InputTooLarge,
            _ => Self::WorkerFailed,
        }
    }

    /// Decoder failures: the `image` crate reports its own limits as `Limits`.
    pub(crate) fn from_image(error: &ImageError) -> Self {
        match error {
            ImageError::Limits(limit) => match limit.kind() {
                LimitErrorKind::DimensionError => Self::DimensionsTooLarge,
                _ => Self::ImageTooLarge,
            },
            _ => Self::InvalidImage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MediaError;

    #[test]
    fn codes_round_trip_through_the_worker_reply() {
        for error in [
            MediaError::UnsupportedType,
            MediaError::InvalidImage,
            MediaError::ImageTooLarge,
            MediaError::DimensionsTooLarge,
            MediaError::InputTooLarge,
        ] {
            assert_eq!(MediaError::from_code(error.code()), error);
        }
        assert_eq!(MediaError::from_code("garbage"), MediaError::WorkerFailed);
    }
}
