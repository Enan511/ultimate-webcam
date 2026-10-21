use openh264::decoder::Decoder;
use openh264::formats::YUVSource;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct HardwareDecoder {
    decoder: Decoder,
    current_width: AtomicU32,
    current_height: AtomicU32,
}

unsafe impl Send for HardwareDecoder {}

impl HardwareDecoder {
    pub fn new() -> anyhow::Result<Self> {
        let decoder = Decoder::new()
            .map_err(|e| anyhow::anyhow!("Failed to initialize OpenH264 decoder: {:?}", e))?;

        Ok(Self {
            decoder,
            current_width: AtomicU32::new(0),
            current_height: AtomicU32::new(0),
        })
    }

    pub fn decode_packet<F>(&mut self, packet_data: &[u8], on_frame: F) -> anyhow::Result<()>
    where
        F: FnMut(Vec<u8>, u32, u32),
    {
        self.decode_packet_with_output(packet_data, true, on_frame)
    }

    pub fn decode_packet_with_output<F>(
        &mut self,
        packet_data: &[u8],
        produce_image: bool,
        mut on_frame: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(Vec<u8>, u32, u32),
    {
        if let Some(decoded) = self
            .decoder
            .decode(packet_data)
            .map_err(|e| anyhow::anyhow!("H.264 decode failed: {e:?}"))?
        {
            let (width, height) = decoded.dimensions();
            if produce_image && width > 0 && height > 0 {
                let w = width as u32;
                let h = height as u32;

                self.current_width.store(w, Ordering::Relaxed);
                self.current_height.store(h, Ordering::Relaxed);

                let mut rgba = vec![0u8; width * height * 4];
                write_rgba(&decoded, &mut rgba);
                on_frame(rgba, w, h);
            }
        }
        Ok(())
    }

    pub fn get_dimensions(&self) -> (u32, u32) {
        (
            self.current_width.load(Ordering::Relaxed),
            self.current_height.load(Ordering::Relaxed),
        )
    }
}

/// Convert directly into the virtual camera's RGBA layout. Eight pixels are
/// packed into one vector store; no intermediate RGB image or expansion pass.
pub fn write_rgba(source: &impl YUVSource, target: &mut [u8]) {
    let (w, h) = source.dimensions();
    let (sy, su, sv) = source.strides();
    assert!(w % 2 == 0 && h % 2 == 0, "I420 dimensions must be even");
    assert!(target.len() >= w * h * 4 && sy >= w && su >= w / 2 && sv >= w / 2);
    assert!(
        source.y().len() >= sy * h
            && source.u().len() >= su * (h / 2)
            && source.v().len() >= sv * (h / 2)
    );
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx2") {
        // Runtime feature detection and the plane/stride bounds above make
        // each unaligned vector load/store valid, including padded rows.
        unsafe {
            rgba_avx2(source, target);
        }
        return;
    }
    rgba_scalar(source, target);
}
fn rgba_scalar(source: &impl YUVSource, target: &mut [u8]) {
    let (w, h) = source.dimensions();
    let (sy, su, sv) = source.strides();
    for y in 0..h {
        for x in 0..w {
            let l = source.y()[y * sy + x] as f32;
            let u = source.u()[y / 2 * su + x / 2] as f32 - 128.;
            let v = source.v()[y / 2 * sv + x / 2] as f32 - 128.;
            target[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&[
                (l + 1.402 * v) as u8,
                (l - 0.344 * u - 0.714 * v) as u8,
                (l + 1.772 * u) as u8,
                255,
            ]);
        }
    }
}
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn rgba_avx2(source: &impl YUVSource, target: &mut [u8]) {
    use std::arch::x86_64::*;
    let (w, h) = source.dimensions();
    let (sy, su, sv) = source.strides();
    let zero = _mm256_setzero_ps();
    let max = _mm256_set1_ps(255.);
    let center = _mm256_set1_ps(128.);
    for y in 0..h {
        let mut x = 0;
        while x + 8 <= w {
            let l = _mm256_cvtepi32_ps(_mm256_cvtepu8_epi32(_mm_loadl_epi64(
                source.y().as_ptr().add(y * sy + x) as *const __m128i,
            )));
            let uv = |p: *const u8| {
                let bytes = _mm_cvtsi32_si128(std::ptr::read_unaligned(p as *const i32));
                _mm256_sub_ps(
                    _mm256_cvtepi32_ps(_mm256_cvtepu8_epi32(_mm_unpacklo_epi8(bytes, bytes))),
                    center,
                )
            };
            let u = uv(source.u().as_ptr().add(y / 2 * su + x / 2));
            let v = uv(source.v().as_ptr().add(y / 2 * sv + x / 2));
            let clamp = |v| _mm256_cvttps_epi32(_mm256_max_ps(zero, _mm256_min_ps(max, v)));
            let r = clamp(_mm256_add_ps(l, _mm256_mul_ps(v, _mm256_set1_ps(1.402))));
            let g = clamp(_mm256_sub_ps(
                _mm256_sub_ps(l, _mm256_mul_ps(u, _mm256_set1_ps(0.344))),
                _mm256_mul_ps(v, _mm256_set1_ps(0.714)),
            ));
            let b = clamp(_mm256_add_ps(l, _mm256_mul_ps(u, _mm256_set1_ps(1.772))));
            let pixels = _mm256_or_si256(
                _mm256_set1_epi32(0xff000000u32 as i32),
                _mm256_or_si256(
                    r,
                    _mm256_or_si256(_mm256_slli_epi32::<8>(g), _mm256_slli_epi32::<16>(b)),
                ),
            );
            _mm256_storeu_si256(
                target.as_mut_ptr().add((y * w + x) * 4) as *mut __m256i,
                pixels,
            );
            x += 8;
        }
        for x in x..w {
            let l = source.y()[y * sy + x] as f32;
            let u = source.u()[y / 2 * su + x / 2] as f32 - 128.;
            let v = source.v()[y / 2 * sv + x / 2] as f32 - 128.;
            target[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&[
                (l + 1.402 * v) as u8,
                (l - 0.344 * u - 0.714 * v) as u8,
                (l + 1.772 * u) as u8,
                255,
            ]);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vector_conversion_handles_padding_tails_and_saturation() {
        for w in [2, 6, 8, 10, 30, 640] {
            let h = 20;
            let sy = w + 16;
            let sc = w / 2 + 8;
            let mut seed = 7u32;
            let mut random = |len| {
                (0..len)
                    .map(|_| {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        (seed >> 24) as u8
                    })
                    .collect::<Vec<_>>()
            };
            let y = random(sy * h);
            let u = random(sc * h / 2);
            let v = random(sc * h / 2);
            let frame = openh264::formats::YUVSlices::new((&y, &u, &v), (w, h), (sy, sc, sc));
            let mut expected = vec![0; w * h * 4];
            let mut actual = expected.clone();
            rgba_scalar(&frame, &mut expected);
            write_rgba(&frame, &mut actual);
            assert!(expected
                .iter()
                .zip(&actual)
                .all(|(a, b)| a.abs_diff(*b) <= 1));
            assert!(actual.iter().skip(3).step_by(4).all(|a| *a == 255));
        }
    }
}
