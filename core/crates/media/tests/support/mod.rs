//! Builds the test images in memory: nothing binary is kept in the repository.
//! Each test binary uses a different subset of these helpers.
#![allow(dead_code)]

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

pub const MIB: usize = 1024 * 1024;

/// Many distinct colors, so the lossy path is taken for JPEG and lossy-decodable sources.
pub fn gradient(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageRgba8(RgbaImage::from_fn(width, height, |x, y| {
        Rgba([x as u8, y as u8, (x ^ y) as u8, 255])
    }))
}

pub fn flat(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(
        width,
        height,
        Rgba([30, 80, 200, 255]),
    ))
}

pub fn encode(image: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, format)
        .expect("test image encodes");
    out.into_inner()
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    let body = [kind.as_slice(), data].concat();
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc32(&body).to_be_bytes());
    chunk
}

/// A PNG that declares `width`×`height` but carries no pixels: the decoder must refuse it from
/// the header alone. The IDAT is an empty zlib stream so the header parse reaches the image data.
pub fn png_declaring(width: u32, height: u32) -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let empty_zlib = [0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01];
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(png_chunk(b"IHDR", &ihdr));
    png.extend(png_chunk(b"IDAT", &empty_zlib));
    png.extend(png_chunk(b"IEND", &[]));
    png
}

/// A JPEG of `width`×`height` with an APP1 EXIF block that holds an orientation (6, rotate 90°)
/// and a GPS pointer. Both are real TIFF entries, so the decoder reads them.
pub fn jpeg_with_gps_exif(width: u32, height: u32) -> Vec<u8> {
    let jpeg = encode(&flat(width, height), ImageFormat::Jpeg);
    let mut tiff = b"II*\0".to_vec();
    tiff.extend_from_slice(&8u32.to_le_bytes());
    let gps_offset: u32 = 8 + 2 + 2 * 12 + 4;
    tiff.extend_from_slice(&2u16.to_le_bytes());
    tiff.extend(ifd_entry(0x0112, 3, 1, 6u32.to_le_bytes()));
    tiff.extend(ifd_entry(0x8825, 4, 1, gps_offset.to_le_bytes()));
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend(ifd_entry(0x0001, 2, 2, [b'N', 0, 0, 0]));
    tiff.extend_from_slice(&0u32.to_le_bytes());

    let mut payload = b"Exif\0\0".to_vec();
    payload.extend(tiff);
    let segment_length = (payload.len() + 2) as u16;
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&segment_length.to_be_bytes());
    out.extend(payload);
    out.extend_from_slice(&jpeg[2..]);
    out
}

/// One 12-byte TIFF IFD entry, value stored inline.
fn ifd_entry(tag: u16, kind: u16, count: u32, value: [u8; 4]) -> Vec<u8> {
    let mut entry = tag.to_le_bytes().to_vec();
    entry.extend_from_slice(&kind.to_le_bytes());
    entry.extend_from_slice(&count.to_le_bytes());
    entry.extend_from_slice(&value);
    entry
}
