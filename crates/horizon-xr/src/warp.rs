//! Application SpaceWarp (`XR_FB_space_warp`): the game draws every other frame and the
//! headset makes up the ones between from each eye's motion vectors and depth.
//!
//! The vectors come from a full-screen pass over the eye's depth, not from the world's
//! own draws: each pixel is put back where the previous frame saw that point of the
//! world (`reproject`), which covers the terrain, still almost everything on screen. The
//! head's own movement stays out of them: the runtime reprojects for it, so the previous
//! frame's view is the current head pose seen from where the player's body was.
use crate::blit::EyeChain;
use ash::vk;
use openxr as xr;

/// Colour format of the motion vectors: signed 16-bit floats, as the runtime asks.
const MOTION_FORMAT: (vk::Format, wgpu::TextureFormat) =
    (vk::Format::R16G16B16A16_SFLOAT, wgpu::TextureFormat::Rgba16Float);

const SHADER: &str = r"
@group(0) @binding(0) var depth: texture_depth_2d;

struct Reproject {
    // Current NDC (homogeneous) to the previous frame's clip space.
    matrix: mat4x4<f32>,
    // x: the eye depth image's size over the motion image's; y: 1 flips the vectors'
    // y to Vulkan's NDC (down).
    scale: vec4<f32>,
};
@group(0) @binding(1) var<uniform> reproject: Reproject;

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

struct Frag {
    @location(0) motion: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs(in: Out) -> Frag {
    let size = vec2<f32>(textureDimensions(depth));
    let texel = vec2<i32>(clamp(in.uv * size, vec2<f32>(0.0), size - 1.0));
    let d = textureLoad(depth, texel, 0);
    let ndc = vec4<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0, d, 1.0);
    let prev = reproject.matrix * ndc;
    var motion = vec3<f32>(0.0);
    if abs(prev.w) > 1e-6 {
        motion = ndc.xyz - prev.xyz / prev.w;
    }
    if reproject.scale.y > 0.5 {
        motion.y = -motion.y;
    }
    var out: Frag;
    out.motion = vec4<f32>(motion, 1.0);
    out.depth = d;
    return out;
}
";

/// The motion and depth images of both eyes, and the pass that fills them.
pub struct SpaceWarp {
    pub size: [u32; 2],
    pub motion: [EyeChain; 2],
    pub depth: [EyeChain; 2],
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniforms: [wgpu::Buffer; 2],
    /// Each eye's images were filled this frame.
    pub filled: [bool; 2],
    /// The depth's range in metres for the runtime: (distance at depth 0, at depth 1).
    pub range: (f32, f32),
    /// Skip the extrapolation this frame (a teleport, a jump of the view).
    pub skip: bool,
    flip_y: bool,
}

impl SpaceWarp {
    pub fn new(
        session: &xr::Session<xr::Vulkan>,
        device: &wgpu::Device,
        size: [u32; 2],
    ) -> Result<Self, String> {
        let offered = session
            .enumerate_swapchain_formats()
            .map_err(|e| format!("swapchain formats: {e}"))?;
        let depth_format = [
            (vk::Format::D32_SFLOAT, wgpu::TextureFormat::Depth32Float),
            (vk::Format::D24_UNORM_S8_UINT, wgpu::TextureFormat::Depth24PlusStencil8),
            (vk::Format::D16_UNORM, wgpu::TextureFormat::Depth16Unorm),
        ]
        .into_iter()
        .find(|(vk_format, _)| offered.contains(&(vk_format.as_raw() as u32)))
        .ok_or("no depth swapchain format")?;
        if !offered.contains(&(MOTION_FORMAT.0.as_raw() as u32)) {
            return Err("no RGBA16F swapchain format".into());
        }
        let motion = [
            EyeChain::with_usage(session, device, MOTION_FORMAT, size, false)?,
            EyeChain::with_usage(session, device, MOTION_FORMAT, size, false)?,
        ];
        let depth = [
            EyeChain::with_usage(session, device, depth_format, size, true)?,
            EyeChain::with_usage(session, device, depth_format, size, true)?,
        ];
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Horizon VR space warp"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Horizon VR space warp"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Horizon VR space warp"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Horizon VR space warp"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(MOTION_FORMAT.1.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format.1,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let uniform = || {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Horizon VR space warp"),
                size: 80,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        log::info!(
            "Horizon VR: space warp on, motion and depth {}x{}, depth {:?}",
            size[0],
            size[1],
            depth_format.1
        );
        Ok(Self {
            size,
            motion,
            depth,
            pipeline,
            layout,
            uniforms: [uniform(), uniform()],
            filled: [false; 2],
            range: (100.0, 0.05),
            skip: false,
            flip_y: std::env::var("HORIZON_XR_SW_FLIPY").as_deref() != Ok("0"),
        })
    }

    /// Fills eye `eye`'s motion and depth images from `depth` (the eye's depth after the
    /// world, reversed Z) and `reproject` (current NDC to the previous frame's clip).
    pub fn fill(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        eye: usize,
        depth: &wgpu::TextureView,
        reproject: glam::Mat4,
    ) -> Result<(), String> {
        let mut data = [0f32; 20];
        data[..16].copy_from_slice(&reproject.to_cols_array());
        data[16] = 1.0;
        data[17] = if self.flip_y { 1.0 } else { 0.0 };
        queue.write_buffer(&self.uniforms[eye], 0, bytemuck_bytes(&data));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Horizon VR space warp"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.uniforms[eye].as_entire_binding(),
                },
            ],
        });
        let motion = self.motion[eye].acquire()?.clone();
        let target = match self.depth[eye].acquire() {
            Ok(view) => view.clone(),
            Err(e) => {
                let _ = self.motion[eye].release();
                return Err(e);
            }
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Horizon VR space warp"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Horizon VR space warp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &motion,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        let released = self.motion[eye].release();
        self.depth[eye].release()?;
        released?;
        self.filled[eye] = true;
        Ok(())
    }

    /// The info to chain to eye `eye`'s projection view.
    pub fn info(&self, eye: usize) -> xr::sys::CompositionLayerSpaceWarpInfoFB {
        let rect = xr::Rect2Di {
            offset: xr::Offset2Di { x: 0, y: 0 },
            extent: xr::Extent2Di { width: self.size[0] as i32, height: self.size[1] as i32 },
        };
        let sub = |chain: &EyeChain| xr::sys::SwapchainSubImage {
            swapchain: chain.handle.as_raw(),
            image_rect: rect,
            image_array_index: 0,
        };
        xr::sys::CompositionLayerSpaceWarpInfoFB {
            ty: xr::sys::CompositionLayerSpaceWarpInfoFB::TYPE,
            next: std::ptr::null(),
            layer_flags: if self.skip {
                xr::sys::CompositionLayerSpaceWarpInfoFlagsFB::FRAME_SKIP
            } else {
                xr::sys::CompositionLayerSpaceWarpInfoFlagsFB::EMPTY
            },
            motion_vector_sub_image: sub(&self.motion[eye]),
            app_space_delta_pose: xr::Posef::IDENTITY,
            depth_sub_image: sub(&self.depth[eye]),
            min_depth: 0.0,
            max_depth: 1.0,
            near_z: self.range.0,
            far_z: self.range.1,
        }
    }
}

fn bytemuck_bytes(data: &[f32]) -> &[u8] {
    // Safety: f32 has no padding and any byte pattern is valid to read as bytes.
    unsafe { std::slice::from_raw_parts(data.as_ptr().cast(), std::mem::size_of_val(data)) }
}
