//! Each eye's OpenXR swapchain, its images as wgpu textures, and the pass that copies
//! the game's offscreen frame into them.
//!
//! The copy is a draw, not `copy_texture_to_texture`: the offscreen frame is BGRA, the
//! swapchain usually RGBA, and a render pass leaves the image in
//! `COLOR_ATTACHMENT_OPTIMAL`, the layout the runtime takes it back in.
use ash::vk::{self, Handle};
use openxr as xr;
use wgpu::hal::api::Vulkan;
use wgpu::hal::vulkan as hv;

/// Swapchain formats the game can draw into, best first: (Vulkan format, wgpu format).
const FORMATS: [(vk::Format, wgpu::TextureFormat); 4] = [
    (vk::Format::R8G8B8A8_SRGB, wgpu::TextureFormat::Rgba8UnormSrgb),
    (vk::Format::B8G8R8A8_SRGB, wgpu::TextureFormat::Bgra8UnormSrgb),
    (vk::Format::R8G8B8A8_UNORM, wgpu::TextureFormat::Rgba8Unorm),
    (vk::Format::B8G8R8A8_UNORM, wgpu::TextureFormat::Bgra8Unorm),
];

pub fn pick_format(offered: &[u32]) -> Option<(vk::Format, wgpu::TextureFormat)> {
    FORMATS
        .into_iter()
        .find(|(vk_format, _)| offered.contains(&(vk_format.as_raw() as u32)))
}

pub struct EyeChain {
    pub handle: xr::Swapchain<xr::Vulkan>,
    views: Vec<wgpu::TextureView>,
}

impl EyeChain {
    pub fn new(
        session: &xr::Session<xr::Vulkan>,
        device: &wgpu::Device,
        format: (vk::Format, wgpu::TextureFormat),
        size: [u32; 2],
    ) -> Result<Self, String> {
        let handle = session
            .create_swapchain(&xr::SwapchainCreateInfo {
                create_flags: xr::SwapchainCreateFlags::EMPTY,
                usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT
                    | xr::SwapchainUsageFlags::SAMPLED,
                format: format.0.as_raw() as u32,
                sample_count: 1,
                width: size[0],
                height: size[1],
                face_count: 1,
                array_size: 1,
                mip_count: 1,
            })
            .map_err(|e| format!("swapchain: {e}"))?;
        let images = handle
            .enumerate_images()
            .map_err(|e| format!("swapchain images: {e}"))?;
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        let views = images
            .into_iter()
            .map(|raw| {
                let hal_desc = wgpu::hal::TextureDescriptor {
                    label: Some("Horizon VR eye"),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: format.1,
                    usage: wgpu::TextureUses::COLOR_TARGET,
                    memory_flags: wgpu::hal::MemoryFlags::empty(),
                    view_formats: vec![],
                };
                // Safety: the runtime owns the image (the no-op drop callback keeps
                // wgpu from destroying it) and made it with this size and format.
                let texture = unsafe {
                    let hal_device = device
                        .as_hal::<Vulkan>()
                        .expect("the VR device is a Vulkan device");
                    let hal_texture = hal_device.texture_from_raw(
                        vk::Image::from_raw(raw),
                        &hal_desc,
                        Some(Box::new(|| {})),
                        hv::TextureMemory::External,
                    );
                    drop(hal_device);
                    device.create_texture_from_hal::<Vulkan>(
                        hal_texture,
                        &wgpu::TextureDescriptor {
                            label: Some("Horizon VR eye"),
                            size: extent,
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: format.1,
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                            view_formats: &[],
                        },
                        wgpu::TextureUses::UNINITIALIZED,
                    )
                };
                texture.create_view(&wgpu::TextureViewDescriptor::default())
            })
            .collect();
        Ok(Self { handle, views })
    }

    /// Acquires the next image, fills it with `color` and gives it back.
    pub fn fill(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color: wgpu::Color,
    ) -> Result<(), String> {
        let index = self
            .handle
            .acquire_image()
            .map_err(|e| format!("xrAcquireSwapchainImage: {e}"))? as usize;
        self.handle
            .wait_image(xr::Duration::INFINITE)
            .map_err(|e| format!("xrWaitSwapchainImage: {e}"))?;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Horizon VR fill"),
        });
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Horizon VR fill"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.views[index],
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        queue.submit([encoder.finish()]);
        self.handle
            .release_image()
            .map_err(|e| format!("xrReleaseSwapchainImage: {e}"))
    }

    /// Acquires the next image, draws `frame` into it and gives it back.
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        blit: &Blit,
        frame: &wgpu::Texture,
    ) -> Result<(), String> {
        let index = self
            .handle
            .acquire_image()
            .map_err(|e| format!("xrAcquireSwapchainImage: {e}"))? as usize;
        self.handle
            .wait_image(xr::Duration::INFINITE)
            .map_err(|e| format!("xrWaitSwapchainImage: {e}"))?;
        let source = frame.create_view(&wgpu::TextureViewDescriptor::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Horizon VR blit"),
            layout: &blit.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&blit.sampler),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Horizon VR blit"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Horizon VR blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.views[index],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&blit.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        self.handle
            .release_image()
            .map_err(|e| format!("xrReleaseSwapchainImage: {e}"))
    }
}

/// The full-screen pass from the offscreen frame to an eye image.
pub struct Blit {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

const SHADER: &str = r"
@group(0) @binding(0) var frame: texture_2d<f32>;
@group(0) @binding(1) var frame_sampler: sampler;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: Out;
    out.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

// The game's frame holds sRGB-encoded colour in a UNORM texture. An sRGB eye image
// encodes what it is given, so it gets the colour decoded first.
fn decode(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_srgb(in: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(decode(textureSample(frame, frame_sampler, in.uv).rgb), 1.0);
}

@fragment
fn fs_unorm(in: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(frame, frame_sampler, in.uv).rgb, 1.0);
}
";

impl Blit {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Horizon VR blit"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Horizon VR blit"),
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
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Horizon VR blit"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Horizon VR blit"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(if format.is_srgb() { "fs_srgb" } else { "fs_unorm" }),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Horizon VR blit"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            layout,
            sampler,
        }
    }
}
