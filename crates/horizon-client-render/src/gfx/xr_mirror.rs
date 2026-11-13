//! Desktop mirror of the VR frame. The headset still gets the eyes and the panel;
//! this copies one of those images into the winit window so the monitor is not
//! a frozen empty surface. Menus (the room panel) are shown whole; in play, one
//! eye. The copy does not wait on the monitor (`Mailbox` or `Immediate`), so it
//! does not cap the headset at the display's refresh rate.
//!
//! `HORIZON_XR_MIRROR=0` (also `off` or `false`) leaves the window untouched.

use std::sync::Arc;

use winit::window::Window;

const SHADER: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let xy = p[i];
    var out: VsOut;
    out.pos = vec4<f32>(xy, 0.0, 1.0);
    out.uv = vec2<f32>(xy.x * 0.5 + 0.5, 0.5 - xy.y * 0.5);
    return out;
}

struct Fit {
    scale: vec2<f32>,
    offset: vec2<f32>,
}

@group(0) @binding(0) var eye: texture_2d<f32>;
@group(0) @binding(1) var eye_sampler: sampler;
@group(0) @binding(2) var<uniform> fit: Fit;

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv * fit.scale + fit.offset;
    if (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    return textureSample(eye, eye_sampler, uv);
}
"#;

pub struct XrMirror {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
}

/// The mirror is on unless `HORIZON_XR_MIRROR` turns it off.
pub fn wanted() -> bool {
    !matches!(
        std::env::var("HORIZON_XR_MIRROR").as_deref(),
        Ok("0") | Ok("off") | Ok("false")
    )
}

impl XrMirror {
    pub fn new(
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        window: Arc<Window>,
    ) -> Result<Self, String> {
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("window surface: {e}"))?;
        let config = configure(&surface, adapter, &window, device)?;
        if config.present_mode == wgpu::PresentMode::Fifo {
            log::warn!(
                "Horizon VR: the desktop mirror can only present with VSync, which may cap the headset at the monitor's refresh rate"
            );
        } else {
            log::info!(
                "Horizon VR: the desktop window shows the menu, or one eye while playing ({:?})",
                config.present_mode
            );
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Horizon VR mirror"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Horizon VR mirror"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Horizon VR mirror"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Horizon VR mirror"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Horizon VR mirror"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Horizon VR mirror fit"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            instance: instance.clone(),
            adapter: adapter.clone(),
            window,
            surface,
            config,
            pipeline,
            layout,
            sampler,
            uniform,
        })
    }

    /// Draws `source` into the window, letterboxed. A minimized window or a lost
    /// swapchain skips the frame; the headset's frame is unaffected.
    pub fn present(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, source: &wgpu::Texture) {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        if size.width != self.config.width || size.height != self.config.height {
            match configure(&self.surface, &self.adapter, &self.window, device) {
                Ok(config) => self.config = config,
                Err(e) => {
                    log::warn!("Horizon VR: desktop mirror resize failed ({e})");
                    return;
                }
            }
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Lost => {
                match self.recreate(device) {
                    Ok(()) => {}
                    Err(e) => log::warn!("Horizon VR: desktop mirror lost its window ({e})"),
                }
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                if let Ok(config) = configure(&self.surface, &self.adapter, &self.window, device) {
                    self.config = config;
                }
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Validation => {
                log::warn!("Horizon VR: desktop mirror swapchain rejected its configuration");
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let source_view = source.create_view(&Default::default());
        let fit = letterbox(
            [source.width(), source.height()],
            [self.config.width, self.config.height],
        );
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&fit));
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Horizon VR mirror"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Horizon VR mirror"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Horizon VR mirror"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        queue.present(frame);
    }

    fn recreate(&mut self, device: &wgpu::Device) -> Result<(), String> {
        self.surface = self
            .instance
            .create_surface(self.window.clone())
            .map_err(|e| format!("window surface: {e}"))?;
        self.config = configure(&self.surface, &self.adapter, &self.window, device)?;
        Ok(())
    }
}

fn configure(
    surface: &wgpu::Surface<'_>,
    adapter: &wgpu::Adapter,
    window: &Window,
    device: &wgpu::Device,
) -> Result<wgpu::SurfaceConfiguration, String> {
    let size = window.inner_size();
    let width = size.width.max(1);
    let height = size.height.max(1);
    let caps = surface.get_capabilities(adapter);
    // The eye holds sRGB bytes in a UNORM texture. An sRGB swapchain would
    // encode them a second time.
    let format = caps
        .formats
        .iter()
        .copied()
        .find(|format| !format.is_srgb())
        .or_else(|| caps.formats.first().copied())
        .ok_or("the window offers no texture format")?;
    let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
        wgpu::PresentMode::Mailbox
    } else if caps.present_modes.contains(&wgpu::PresentMode::Immediate) {
        wgpu::PresentMode::Immediate
    } else {
        wgpu::PresentMode::Fifo
    };
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode,
        desired_maximum_frame_latency: 2,
        alpha_mode: caps
            .alpha_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::CompositeAlphaMode::Opaque)
            .unwrap_or(wgpu::CompositeAlphaMode::Auto),
        view_formats: vec![],
        color_space: wgpu::SurfaceColorSpace::default(),
    };
    surface.configure(device, &config);
    Ok(config)
}

/// `scale` and `offset` such that `eye_uv = window_uv * scale + offset` shows the
/// whole eye inside the window, with black bars where the aspects differ.
fn letterbox(eye: [u32; 2], window: [u32; 2]) -> [f32; 4] {
    let ew = eye[0] as f32;
    let eh = eye[1] as f32;
    let ww = window[0] as f32;
    let wh = window[1] as f32;
    if ew <= 0.0 || eh <= 0.0 || ww <= 0.0 || wh <= 0.0 {
        return [1.0, 1.0, 0.0, 0.0];
    }
    let eye_aspect = ew / eh;
    let win_aspect = ww / wh;
    if win_aspect > eye_aspect {
        let span = eye_aspect / win_aspect;
        let scale_x = 1.0 / span;
        [scale_x, 1.0, -(1.0 - span) / (2.0 * span), 0.0]
    } else {
        let span = win_aspect / eye_aspect;
        let scale_y = 1.0 / span;
        [1.0, scale_y, 0.0, -(1.0 - span) / (2.0 * span)]
    }
}
