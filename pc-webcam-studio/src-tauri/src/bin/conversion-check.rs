use openh264::{decoder::Decoder, formats::YUVSource};
use pc_webcam_studio_lib::stream::hw_decoder::HardwareDecoder;
fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("Pass an H264 keyframe fixture"))?;
    let packet = std::fs::read(path)?;
    let mut reference_decoder = Decoder::new()?;
    let frame = reference_decoder
        .decode(&packet)?
        .ok_or_else(|| anyhow::anyhow!("Fixture must contain a decoded image"))?;
    let (w, h) = frame.dimensions();
    let mut reference = vec![0; w * h * 4];
    frame.write_rgba8(&mut reference);
    let mut tested = 0;
    HardwareDecoder::new()?.decode_packet(&packet, |rgba, width, height| {
        assert_eq!((width, height), (w as u32, h as u32));
        assert!(rgba.iter().skip(3).step_by(4).all(|a| *a == 255));
        let max_error = rgba
            .iter()
            .zip(&reference)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max_error <= 1, "RGB channel error {max_error}");
        tested += 1;
    })?;
    anyhow::ensure!(tested == 1);
    let mut rgb = vec![0; w * h * 3];
    let begin = std::time::Instant::now();
    for _ in 0..100 {
        frame.write_rgba8(std::hint::black_box(&mut reference));
    }
    let scalar = begin.elapsed().as_secs_f64() * 10.0;
    let begin = std::time::Instant::now();
    for _ in 0..100 {
        frame.write_rgb8(std::hint::black_box(&mut rgb));
        for (src, dst) in rgb.chunks_exact(3).zip(reference.chunks_exact_mut(4)) {
            dst.copy_from_slice(&[src[0], src[1], src[2], 255]);
        }
        std::hint::black_box(&reference);
    }
    let vector = begin.elapsed().as_secs_f64() * 10.0;
    println!("PASS: {w}x{h} decoded pixels match scalar output within 1 channel value; alpha opaque. Scalar {scalar:.3} ms, vector RGB+alpha {vector:.3} ms.");
    let begin = std::time::Instant::now();
    for _ in 0..100 {
        pc_webcam_studio_lib::stream::hw_decoder::write_rgba(&frame, &mut reference);
        std::hint::black_box(&reference);
    }
    let direct = begin.elapsed().as_secs_f64() * 10.;
    println!(
        "Direct packed SIMD RGBA: {direct:.3} ms; {:.2}x faster than RGB+alpha",
        vector / direct
    );
    Ok(())
}
