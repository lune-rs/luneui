//! Offscreen render targets and GPU readback to PNG: the frame-dump building blocks. Readback
//! copies a texture back through a mapped buffer (no window or screen capture), so dumps work
//! on a hidden window and are byte-comparable between runs. Deep formats read back quantised to
//! 8 bits by `read_rgba`, or at full precision by `read_rgba_f32`.

use std::path::Path;

/// An offscreen colour target of `format` and `size` physical pixels that can be drawn into
/// and read back.
pub fn target(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    size: (u32, u32),
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("frame dump"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Copies `texture` back to the CPU as tightly packed RGBA8, keeping alpha. The deep formats
/// (`Rgb10a2Unorm`, `Rgba16Unorm`, `Rgba16Float`) are quantised to 8 bits per channel, halves
/// up; [`read_rgba_f32`] keeps their full precision.
pub fn read_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Result<Vec<u8>, String> {
    with_readback(device, queue, texture, unpad_to_rgba)
}

/// Copies `texture` back to the CPU as tightly packed RGBA, four `f32` per pixel. A unorm
/// channel is its integer divided by its maximum (0..=1); a float channel is the stored value.
pub fn read_rgba_f32(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Result<Vec<f32>, String> {
    with_readback(device, queue, texture, unpad_to_f32)
}

/// Copies `texture` into a mapped buffer and hands its rows to `read` with the texture's width,
/// height, padded row length in bytes and format.
fn with_readback<T>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    read: impl FnOnce(&[u8], u32, u32, u32, wgpu::TextureFormat) -> Result<T, String>,
) -> Result<T, String> {
    let (w, h, format) = (texture.width(), texture.height(), texture.format());
    let bytes = format
        .block_copy_size(None)
        .ok_or_else(|| format!("unsupported dump format {format:?}"))?;
    let row = (w * bytes).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("frame dump readback"),
        size: u64::from(row) * u64::from(h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(h),
            },
        },
        texture.size(),
    );
    queue.submit(Some(enc.finish()));
    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| format!("poll: {e:?}"))?;
    let data = slice
        .get_mapped_range()
        .map_err(|e| format!("map: {e:?}"))?;
    read(&data, w, h, row, format)
}

/// The byte layout of one readback pixel: which formats `unpad` reads and what each channel is.
#[derive(Clone, Copy)]
enum Kind {
    /// `Bgra8Unorm` and `Bgra8UnormSrgb`: swizzled to RGBA.
    Bgra8,
    /// `Rgba8Unorm` and `Rgba8UnormSrgb`: already RGBA.
    Rgba8,
    /// `Rgb10a2Unorm`: one little-endian `u32`, r in bits 0..10, g 10..20, b 20..30, a 30..32.
    Rgb10a2,
    /// `Rgba16Unorm`: four little-endian `u16`.
    Rgba16,
    /// `Rgba16Float`: four little-endian IEEE 754 half floats.
    Rgba16Float,
}

impl Kind {
    fn of(format: wgpu::TextureFormat) -> Result<Self, String> {
        use wgpu::TextureFormat as F;
        match format {
            F::Bgra8Unorm | F::Bgra8UnormSrgb => Ok(Self::Bgra8),
            F::Rgba8Unorm | F::Rgba8UnormSrgb => Ok(Self::Rgba8),
            F::Rgb10a2Unorm => Ok(Self::Rgb10a2),
            F::Rgba16Unorm => Ok(Self::Rgba16),
            F::Rgba16Float => Ok(Self::Rgba16Float),
            other => Err(format!("unsupported dump format {other:?}")),
        }
    }

    /// Bytes per pixel.
    fn bytes(self) -> usize {
        match self {
            Self::Bgra8 | Self::Rgba8 | Self::Rgb10a2 => 4,
            Self::Rgba16 | Self::Rgba16Float => 8,
        }
    }

    /// The four channels of one pixel (`px` is `bytes()` long), in RGBA order.
    fn texel(self, px: &[u8]) -> [Channel; 4] {
        let byte = |i: usize| Channel::Unorm(u32::from(px[i]), 255);
        let half = |i: usize| u16::from_le_bytes([px[2 * i], px[2 * i + 1]]);
        let unorm16 = |i: usize| Channel::Unorm(u32::from(half(i)), 65535);
        let float16 = |i: usize| Channel::Float(f16_to_f32(half(i)));
        match self {
            Self::Bgra8 => [byte(2), byte(1), byte(0), byte(3)],
            Self::Rgba8 => [byte(0), byte(1), byte(2), byte(3)],
            Self::Rgb10a2 => {
                let v = u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
                [
                    Channel::Unorm(v & 0x3ff, 1023),
                    Channel::Unorm((v >> 10) & 0x3ff, 1023),
                    Channel::Unorm((v >> 20) & 0x3ff, 1023),
                    Channel::Unorm(v >> 30, 3),
                ]
            }
            Self::Rgba16 => [unorm16(0), unorm16(1), unorm16(2), unorm16(3)],
            Self::Rgba16Float => [float16(0), float16(1), float16(2), float16(3)],
        }
    }
}

/// One stored channel: a unorm integer out of its maximum, or a float.
#[derive(Clone, Copy)]
enum Channel {
    Unorm(u32, u32),
    Float(f32),
}

impl Channel {
    /// Quantised to 8 bits, halves up. A float is clamped to 0..=1 first, and NaN is 0.
    fn to_u8(self) -> u8 {
        match self {
            Self::Unorm(v, max) => ((2 * v * 255 + max) / (2 * max)) as u8,
            Self::Float(x) if x.is_nan() => 0,
            Self::Float(x) => (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        }
    }

    fn to_f32(self) -> f32 {
        match self {
            Self::Unorm(v, max) => v as f32 / max as f32,
            Self::Float(x) => x,
        }
    }
}

/// Decodes an IEEE 754 binary16 bit pattern (sign, 5-bit exponent, 10-bit fraction).
fn f16_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = f32::from(bits & 0x3ff);
    let magnitude = match exponent {
        // Zero and subnormals: fraction * 2^-10 * 2^-14.
        0 => fraction * 2f32.powi(-24),
        0x1f if fraction == 0.0 => f32::INFINITY,
        0x1f => f32::NAN,
        e => (1.0 + fraction / 1024.0) * 2f32.powi(e - 15),
    };
    sign * magnitude
}

/// Checks the readback's format and that its buffer holds `h` rows of `row` bytes, each with
/// `w` pixels. Returns the pixel kind and the sizes as `usize`.
fn checked_layout(
    data: &[u8],
    w: u32,
    h: u32,
    row: u32,
    format: wgpu::TextureFormat,
) -> Result<(Kind, usize, usize, usize), String> {
    let kind = Kind::of(format)?;
    let (w, h, row) = (w as usize, h as usize, row as usize);
    if row < w * kind.bytes() || data.len() < row * h {
        return Err("readback buffer too small".to_owned());
    }
    Ok((kind, w, h, row))
}

/// Removes the row padding of a readback and converts to RGBA8, keeping alpha. Deep formats
/// are quantised (see [`Channel::to_u8`]).
fn unpad_to_rgba(
    data: &[u8],
    w: u32,
    h: u32,
    row: u32,
    format: wgpu::TextureFormat,
) -> Result<Vec<u8>, String> {
    let (kind, w, h, row) = checked_layout(data, w, h, row, format)?;
    let bytes = kind.bytes();
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for px in data[y * row..y * row + w * bytes].chunks_exact(bytes) {
            rgba.extend(kind.texel(px).map(Channel::to_u8));
        }
    }
    Ok(rgba)
}

/// Removes the row padding of a readback and converts to RGBA, four `f32` per pixel.
fn unpad_to_f32(
    data: &[u8],
    w: u32,
    h: u32,
    row: u32,
    format: wgpu::TextureFormat,
) -> Result<Vec<f32>, String> {
    let (kind, w, h, row) = checked_layout(data, w, h, row, format)?;
    let bytes = kind.bytes();
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for px in data[y * row..y * row + w * bytes].chunks_exact(bytes) {
            rgba.extend(kind.texel(px).map(Channel::to_f32));
        }
    }
    Ok(rgba)
}

/// Writes `rgba` (8 bits per channel, `size` wide and high) to `path` as a PNG. Errors as a message
/// naming what failed.
pub fn write_png(path: &Path, size: (u32, u32), rgba: &[u8]) -> Result<(), String> {
    let file =
        std::fs::File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), size.0, size.1);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpad_swizzles_bgra_and_keeps_alpha() {
        // 2x2 BGRA, rows padded to 256 bytes with 0xEE filler.
        let mut data = vec![0xEE; 512];
        data[..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        data[256..264].copy_from_slice(&[9, 10, 11, 12, 13, 14, 15, 16]);
        let out = unpad_to_rgba(&data, 2, 2, 256, wgpu::TextureFormat::Bgra8Unorm)
            .expect("valid readback");
        assert_eq!(out, [3, 2, 1, 4, 7, 6, 5, 8, 11, 10, 9, 12, 15, 14, 13, 16]);
        let srgb = unpad_to_rgba(&data, 2, 2, 256, wgpu::TextureFormat::Bgra8UnormSrgb)
            .expect("valid readback");
        assert_eq!(srgb, out);
    }

    #[test]
    fn unpad_passes_rgba_through() {
        let mut data = vec![0xEE; 512];
        data[..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        data[256..264].copy_from_slice(&[9, 10, 11, 12, 13, 14, 15, 16]);
        let out = unpad_to_rgba(&data, 2, 2, 256, wgpu::TextureFormat::Rgba8Unorm)
            .expect("valid readback");
        assert_eq!(out, (1..=16).collect::<Vec<u8>>());
    }

    #[test]
    fn unpad_rejects_other_formats_and_short_buffers() {
        assert!(unpad_to_rgba(&[0; 512], 2, 2, 256, wgpu::TextureFormat::R8Unorm).is_err());
        assert!(unpad_to_rgba(&[0; 100], 2, 2, 256, wgpu::TextureFormat::Bgra8Unorm).is_err());
        assert!(unpad_to_f32(&[0; 512], 2, 2, 256, wgpu::TextureFormat::R8Unorm).is_err());
    }

    #[test]
    fn rgb10a2_reads_the_packing_and_quantises_alpha() {
        // Four pixels, one per alpha level 0..=3; r = 1023, g = 512, b = 0 in each.
        let mut data = vec![0xEE; 256];
        for a in 0..4u32 {
            let px = 1023 | (512 << 10) | (a << 30);
            let at = a as usize * 4;
            data[at..at + 4].copy_from_slice(&px.to_le_bytes());
        }
        let fmt = wgpu::TextureFormat::Rgb10a2Unorm;
        let out = unpad_to_rgba(&data, 4, 1, 256, fmt).expect("valid readback");
        assert_eq!(
            out,
            [
                255, 128, 0, 0, 255, 128, 0, 85, 255, 128, 0, 170, 255, 128, 0, 255
            ]
        );
        let f = unpad_to_f32(&data, 4, 1, 256, fmt).expect("valid readback");
        assert_eq!(f[0..4], [1.0, 512.0 / 1023.0, 0.0, 0.0]);
        assert_eq!(f[12..16], [1.0, 512.0 / 1023.0, 0.0, 1.0]);
        assert_eq!(f[7], 1.0 / 3.0, "alpha level 1 as a fraction");
    }

    #[test]
    fn unorm16_quantises_halves_up() {
        let mut data = vec![0xEE; 256];
        for (i, r) in [0u16, 32767, 32768, 65535].into_iter().enumerate() {
            let mut px = [0u8; 8];
            px[0..2].copy_from_slice(&r.to_le_bytes());
            px[6..8].copy_from_slice(&u16::MAX.to_le_bytes());
            data[i * 8..i * 8 + 8].copy_from_slice(&px);
        }
        let fmt = wgpu::TextureFormat::Rgba16Unorm;
        let out = unpad_to_rgba(&data, 4, 1, 256, fmt).expect("valid readback");
        let reds: Vec<u8> = out.chunks_exact(4).map(|px| px[0]).collect();
        assert_eq!(reds, [0, 127, 128, 255]);
        assert_eq!(out[3], 255, "alpha 65535 is 255");
        let f = unpad_to_f32(&data, 4, 1, 256, fmt).expect("valid readback");
        assert_eq!(f[8], 32768.0 / 65535.0);
    }

    #[test]
    fn float16_quantises_and_decodes() {
        // r holds each half; g and b are 0, a is 1.0.
        let halves: [u16; 6] = [0x3C00, 0x3800, 0x0001, 0x7C00, 0xFE00, 0xBC00];
        let mut data = vec![0xEE; 256];
        for (i, r) in halves.into_iter().enumerate() {
            let mut px = [0u8; 8];
            px[0..2].copy_from_slice(&r.to_le_bytes());
            px[6..8].copy_from_slice(&0x3C00u16.to_le_bytes());
            data[i * 8..i * 8 + 8].copy_from_slice(&px);
        }
        let fmt = wgpu::TextureFormat::Rgba16Float;
        let out = unpad_to_rgba(&data, 6, 1, 256, fmt).expect("valid readback");
        let reds: Vec<u8> = out.chunks_exact(4).map(|px| px[0]).collect();
        // 1.0, 0.5, subnormal, +inf, NaN, -1.0
        assert_eq!(reds, [255, 128, 0, 255, 0, 0]);
        assert!(out.chunks_exact(4).all(|px| px[3] == 255));
        let f = unpad_to_f32(&data, 6, 1, 256, fmt).expect("valid readback");
        assert_eq!(f[0], 1.0);
        assert_eq!(f[4], 0.5);
        assert_eq!(f[8], 2f32.powi(-24), "smallest subnormal");
        assert!(f[12].is_infinite() && f[12] > 0.0);
        assert!(f[16].is_nan());
        assert_eq!(f[20], -1.0);
    }

    #[test]
    fn f16_decodes_zero_signs_and_normals() {
        assert_eq!(f16_to_f32(0x0000), 0.0);
        assert!(f16_to_f32(0x8000).is_sign_negative());
        assert_eq!(f16_to_f32(0x4000), 2.0);
        assert_eq!(f16_to_f32(0xC000), -2.0);
        assert_eq!(f16_to_f32(0x7BFF), 65504.0, "largest finite half");
    }

    #[test]
    fn f32_readback_swizzles_bgra() {
        let mut data = vec![0xEE; 512];
        data[..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let out = unpad_to_f32(&data, 2, 1, 256, wgpu::TextureFormat::Bgra8Unorm)
            .expect("valid readback");
        assert_eq!(
            out[..4],
            [3.0 / 255.0, 2.0 / 255.0, 1.0 / 255.0, 4.0 / 255.0]
        );
        assert_eq!(
            out[4..8],
            [7.0 / 255.0, 6.0 / 255.0, 5.0 / 255.0, 8.0 / 255.0]
        );
    }

    #[test]
    fn deep_formats_reject_short_buffers() {
        let fmt = wgpu::TextureFormat::Rgba16Float;
        assert!(unpad_to_rgba(&[0; 300], 2, 2, 256, fmt).is_err());
        assert!(unpad_to_f32(&[0; 300], 2, 2, 256, fmt).is_err());
        // A row too short for two 8-byte pixels.
        assert!(unpad_to_f32(&[0; 512], 2, 2, 8, fmt).is_err());
    }

    #[test]
    fn write_png_round_trips_the_pixels_and_names_a_failure() {
        let rgba: Vec<u8> = (0..16).map(|i| i * 16).collect();
        let path = std::env::temp_dir().join(format!("luneui-dump-{}.png", std::process::id()));
        assert!(!path.exists(), "the scratch name is ours alone");
        write_png(&path, (2, 2), &rgba).expect("writes");
        let decoder = png::Decoder::new(std::io::BufReader::new(
            std::fs::File::open(&path).expect("opens"),
        ));
        let mut reader = decoder.read_info().expect("a PNG");
        let mut back = vec![0; reader.output_buffer_size().expect("sized")];
        let info = reader.next_frame(&mut back).expect("decodes");
        std::fs::remove_file(&path).expect("removes its own file");
        assert_eq!((info.width, info.height), (2, 2));
        assert_eq!(back[..16], rgba[..]);
        // A path that cannot be created is an error naming it, not a panic.
        let bad = path.join("no-such-dir").join("x.png");
        assert!(write_png(&bad, (2, 2), &rgba).is_err_and(|e| e.contains("create")));
    }
}
