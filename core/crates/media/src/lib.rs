//! Validates uploaded images by their real header and re-encodes them to WebP, pixels only.

mod decode;
mod encode;
mod error;
mod isolated;
mod limits;
mod process;
mod sniff;
mod wire;
pub mod worker;

pub use error::MediaError;
pub use isolated::process_image_isolated;
pub use limits::Limits;
pub use process::{process_image, ProcessedImage, WEBP_MIME};
