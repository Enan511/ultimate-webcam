use bytemuck::{Pod, Zeroable};
use std::sync::Arc;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug, serde::Deserialize, serde::Serialize)]
pub struct FilterUniforms {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub warmth: f32,
    pub sharpness: f32,
    pub vignette: f32,
    pub lut_preset: u32,
    pub mirror_h: u32,
    pub flip_v: u32,
    pub rotation: u32,
    pub zoom_crop: f32,
    pub width: f32,
    pub height: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

impl FilterUniforms {
    pub fn is_neutral(&self) -> bool {
        self.brightness == 0.0
            && self.contrast == 1.0
            && self.saturation == 1.0
            && self.warmth == 0.0
            && self.sharpness == 0.0
            && self.vignette == 0.0
            && self.lut_preset == 0
            && self.mirror_h == 0
            && self.flip_v == 0
            && self.rotation == 0
            && self.zoom_crop == 1.0
    }
}

impl Default for FilterUniforms {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            warmth: 0.0,
            sharpness: 0.0,
            vignette: 0.0,
            lut_preset: 0,
            mirror_h: 0,
            flip_v: 0,
            rotation: 0,
            zoom_crop: 1.0,
            width: 1920.0,
            height: 1080.0,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        }
    }
}

pub struct ProcessedFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct WgpuPipeline {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    current_uniforms: FilterUniforms,

    // Cached textures for zero reallocation
    cached_width: u32,
    cached_height: u32,
    cached_portrait: bool,
    input_texture: Option<wgpu::Texture>,
    output_texture: Option<wgpu::Texture>,
    staging_buffer: Option<wgpu::Buffer>,
}

impl WgpuPipeline {
    pub fn new() -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| anyhow::anyhow!("Failed to find suitable GPU adapter for DirectX 12"))?;

        log::info!(
            "Initialized DirectX 12 adapter: {:?}",
            adapter.get_info().name
        );

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("DirectX 12 WGPU Headless Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
            },
            None,
        ))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("filters.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/filters.wgsl").into()),
        });

        let initial_uniforms = FilterUniforms::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Filter Uniform Buffer"),
            contents: bytemuck::bytes_of(&initial_uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Filter Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Filter Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Filter Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Bilinear Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            sampler,
            uniform_buffer,
            current_uniforms: initial_uniforms,
            cached_width: 0,
            cached_height: 0,
            cached_portrait: false,
            input_texture: None,
            output_texture: None,
            staging_buffer: None,
        })
    }

    pub fn update_uniforms(&mut self, mut uniforms: FilterUniforms) {
        uniforms.width = self.cached_width as f32;
        uniforms.height = self.cached_height as f32;
        self.current_uniforms = uniforms;
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    pub fn process_frame(
        &mut self,
        rgba_data: &[u8],
        width: u32,
        height: u32,
    ) -> anyhow::Result<ProcessedFrame> {
        let portrait =
            self.current_uniforms.rotation == 90 || self.current_uniforms.rotation == 270;
        let (out_width, out_height) = if portrait {
            (height, width)
        } else {
            (width, height)
        };
        if self.cached_width != width
            || self.cached_height != height
            || self.cached_portrait != portrait
        {
            self.cached_portrait = portrait;
            self.cached_width = width;
            self.cached_height = height;

            // Re-create textures & staging buffer
            let texture_size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };

            let in_tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Input RGBA Texture"),
                size: texture_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            let out_tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Output Filtered Texture"),
                size: wgpu::Extent3d {
                    width: out_width,
                    height: out_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });

            let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT; // 256 bytes alignment
            let bytes_per_row = (out_width * 4 + align - 1) & !(align - 1);
            let staging_size = (bytes_per_row * out_height) as u64;

            let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Staging Readback Buffer"),
                size: staging_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });

            self.input_texture = Some(in_tex);
            self.output_texture = Some(out_tex);
            self.staging_buffer = Some(staging);

            // Update uniform resolution
            let mut u = self.current_uniforms;
            u.width = width as f32;
            u.height = height as f32;
            self.update_uniforms(u);
        }

        let in_tex = self.input_texture.as_ref().unwrap();
        let out_tex = self.output_texture.as_ref().unwrap();
        let staging = self.staging_buffer.as_ref().unwrap();

        // 1. Upload incoming raw RGBA frame to input texture
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: in_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba_data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let in_view = in_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let out_view = out_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Filter Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&in_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        // 2. Execute GPU Render Pass
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Filter Command Encoder"),
            });

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Filter Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &out_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw(0..3, 0..1); // 1 fullscreen triangle
        }

        // 3. Copy filtered output texture to staging buffer
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let bytes_per_row = (out_width * 4 + align - 1) & !(align - 1);

        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: out_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: staging,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(out_height),
                },
            },
            wgpu::Extent3d {
                width: out_width,
                height: out_height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        // 4. Map and read back staging buffer
        let buffer_slice = staging.slice(..);
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        loop {
            self.device.poll(wgpu::Maintain::Poll);
            match rx.try_recv() {
                Ok(result) => {
                    result?;
                    break;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    anyhow::bail!("GPU readback channel closed")
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
            }
            if std::time::Instant::now() >= deadline {
                anyhow::bail!("GPU readback timed out; using the original camera image");
            }
            std::thread::sleep(std::time::Duration::from_micros(100));
        }

        let mapped = buffer_slice.get_mapped_range();
        let mut output_bytes = vec![0u8; (out_width * out_height * 4) as usize];

        // Strip row alignment padding
        for row in 0..out_height {
            let src_start = (row * bytes_per_row) as usize;
            let src_end = src_start + (out_width * 4) as usize;
            let dst_start = (row * out_width * 4) as usize;
            let dst_end = dst_start + (out_width * 4) as usize;
            output_bytes[dst_start..dst_end].copy_from_slice(&mapped[src_start..src_end]);
        }

        drop(mapped);
        staging.unmap();

        Ok(ProcessedFrame {
            rgba: output_bytes,
            width: out_width,
            height: out_height,
        })
    }
}
