use pc_webcam_studio_lib::gpu::wgpu_pipeline::{FilterUniforms, WgpuPipeline};
fn main() -> anyhow::Result<()> {
    let (w, h) = (128u32, 72u32);
    let mut input = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            input.extend_from_slice(&[
                (x * 255 / (w - 1)) as u8,
                (y * 255 / (h - 1)) as u8,
                127,
                255,
            ]);
        }
    }
    let mut gpu = WgpuPipeline::new()?;
    let frame = gpu.process_frame(&input, w, h)?;
    anyhow::ensure!(frame.width == w && frame.height == h);
    anyhow::ensure!(
        frame
            .rgba
            .iter()
            .zip(&input)
            .all(|(a, b)| (*a as i16 - *b as i16).abs() <= 2),
        "Identity filter changed image"
    );
    let mut u = FilterUniforms::default();
    u.mirror_h = 1;
    gpu.update_uniforms(u);
    let mirror = gpu.process_frame(&input, w, h)?;
    anyhow::ensure!(
        mirror.rgba[0] > 250 && mirror.rgba[1] < 3,
        "Mirror transform failed"
    );
    u.mirror_h = 0;
    u.rotation = 90;
    gpu.update_uniforms(u);
    let rotated = gpu.process_frame(&input, w, h)?;
    anyhow::ensure!(
        rotated.width == h && rotated.height == w,
        "Portrait dimensions are incorrect"
    );
    anyhow::ensure!(
        rotated.rgba[0] < 3 && rotated.rgba[1] > 250,
        "Rotation transform failed"
    );
    println!("PASS: DirectX 12 identity pixels, horizontal mirror, and portrait rotation");
    Ok(())
}
