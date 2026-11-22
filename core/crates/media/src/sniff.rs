use crate::error::MediaError;

const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const JPEG_SOI: &[u8] = &[0xFF, 0xD8, 0xFF];
const VP8X_ANIMATION_FLAG: u8 = 0x02;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Png,
    Jpeg,
    WebP,
}

/// Detects the type from the real header. Client names and MIME types are never consulted.
pub(crate) fn sniff(bytes: &[u8]) -> Result<Kind, MediaError> {
    if bytes.starts_with(PNG_SIGNATURE) {
        return if png_is_animated(bytes) {
            Err(MediaError::UnsupportedType)
        } else {
            Ok(Kind::Png)
        };
    }
    if bytes.starts_with(JPEG_SOI) {
        return Ok(Kind::Jpeg);
    }
    if bytes.get(0..4) == Some(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return if webp_is_animated(bytes) {
            Err(MediaError::UnsupportedType)
        } else {
            Ok(Kind::WebP)
        };
    }
    Err(MediaError::UnsupportedType)
}

/// An APNG carries an `acTL` chunk before its first `IDAT`.
fn png_is_animated(bytes: &[u8]) -> bool {
    let mut offset = PNG_SIGNATURE.len();
    while let Some(header) = bytes.get(offset..offset + 8) {
        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        match &header[4..8] {
            b"acTL" => return true,
            b"IDAT" | b"IEND" => return false,
            _ => offset = offset.saturating_add(length).saturating_add(12),
        }
    }
    false
}

fn webp_is_animated(bytes: &[u8]) -> bool {
    bytes.get(12..16) == Some(b"VP8X")
        && bytes
            .get(20)
            .is_some_and(|flags| flags & VP8X_ANIMATION_FLAG != 0)
}

#[cfg(test)]
mod tests {
    use super::{sniff, Kind};
    use crate::error::MediaError;

    #[test]
    fn detects_the_real_header_only() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n rest"), Ok(Kind::Png));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Ok(Kind::Jpeg));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Ok(Kind::WebP));
        assert_eq!(sniff(b"GIF89a"), Err(MediaError::UnsupportedType));
        assert_eq!(sniff(b"<svg xmlns"), Err(MediaError::UnsupportedType));
        assert_eq!(sniff(b"%PDF-1.7"), Err(MediaError::UnsupportedType));
        assert_eq!(sniff(b"\x89PNG"), Err(MediaError::UnsupportedType));
    }

    #[test]
    fn refuses_animated_webp() {
        let mut animated = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0".to_vec();
        animated.extend_from_slice(&[0x02, 0, 0, 0]);
        assert_eq!(sniff(&animated), Err(MediaError::UnsupportedType));
    }
}
