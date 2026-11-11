//! VR on a standalone headset (Meta Quest), through OpenXR.
//!
//! The renderer keeps drawing the game as it does offscreen; this crate opens the
//! Vulkan device the way OpenXR wants it (`XR_KHR_vulkan_enable2`), runs the session,
//! and hands the renderer one view per eye. After each eye is drawn, [`submit_eye`]
//! copies the offscreen frame into that eye's swapchain image.
//!
//! World axes are Minecraft's: Y up, yaw 0 facing +Z. The headset's LOCAL space is
//! turned by the player's body yaw (`Ry(PI - yaw)`, the same rotation
//! `camera::view_matrix` uses), so the head looks where the player looks.
//!
//! The Touch controllers become a gamepad ([`pad`]), so menus and play work through
//! the game's existing controller support.
//!
//! Android only; other targets compile an empty crate.
#![cfg(target_os = "android")]

mod blit;

use ash::vk::{self, Handle};
use glam::{Quat, Vec3};
use openxr as xr;
use std::ffi::{CStr, CString, c_void};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;
use wgpu::hal::api::Vulkan;
use wgpu::hal::vulkan as hv;

/// Eye images are drawn at this fraction of the runtime's recommended size: the game
/// is drawn twice per frame, and the Quest's recommended size is large.
const RESOLUTION_SCALE: f32 = 0.7;
/// The farthest the head may lean away from where it started before the view stops
/// following it, in metres (the player's body does not move with it).
const MAX_LEAN: f32 = 0.5;
const VIEW_TYPE: xr::ViewConfigurationType = xr::ViewConfigurationType::PRIMARY_STEREO;
/// Menus show on a panel this far in front of where the head faced when they opened, and
/// this wide, in metres.
const MENU_DISTANCE: f32 = 1.6;
const MENU_WIDTH: f32 = 1.6;
/// The HUD's panel, in front of the eyes, moving with the head.
const HUD_DISTANCE: f32 = 1.2;
const HUD_WIDTH: f32 = 1.3;

/// Where the HUD's panel (the game's 2D layer: HUD, menus, the title screen's buttons)
/// shows this frame, over the eyes' view, transparent where it draws nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    /// Fixed in the room where the head faced when the menu opened (menus, the title
    /// and loading screens).
    Room,
    /// Before the eyes, moving with the head (the HUD in play).
    Head,
}

/// The Touch controllers read as a gamepad.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pad {
    pub a: bool,
    pub b: bool,
    pub x: bool,
    pub y: bool,
    pub menu: bool,
    pub left_click: bool,
    pub right_click: bool,
    pub left_grip: bool,
    pub right_grip: bool,
    /// Thumbsticks, x right and y up, -1..1.
    pub left: [f32; 2],
    pub right: [f32; 2],
    /// Left and right trigger, 0..1.
    pub triggers: [f32; 2],
}

/// One eye of the frame being drawn, in Minecraft's world axes.
#[derive(Clone, Copy, Debug)]
pub struct Eye {
    /// The eye's orientation in the world (local to world).
    pub rotation: Quat,
    /// The eye's position relative to the game's camera, in world axes.
    pub offset: Vec3,
    /// Tangents of the field of view: left (negative), right, up, down (negative).
    pub fov: [f32; 4],
}

/// What the renderer needs for a frame: both eyes and the eye image size.
#[derive(Clone, Copy, Debug)]
pub struct FrameViews {
    pub eyes: [Eye; 2],
    pub size: [u32; 2],
}

/// The activity the OpenXR loader and runtime attach to, set once by `android_main`.
struct Context {
    vm: usize,
    activity: usize,
    /// Where `xr.pending` is kept: a launch whose session never got focus leaves it,
    /// and the next launch starts flat.
    marker_dir: PathBuf,
}
static CONTEXT: OnceLock<Context> = OnceLock::new();

static XR: Mutex<Option<Xr>> = Mutex::new(None);

// Safety: every use of the OpenXR and Vulkan handles goes through the `XR` mutex.
unsafe impl Send for Xr {}

fn xr_state() -> MutexGuard<'static, Option<Xr>> {
    XR.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Xr {
    instance: xr::Instance,
    system: xr::SystemId,
    vk_instance: ash::Instance,
    physical_device: vk::PhysicalDevice,
    /// The device extensions the runtime needs (`xrGetVulkanDeviceExtensionsKHR`).
    device_extensions: Vec<&'static CStr>,
    session: Option<Session>,
}

struct Session {
    session: xr::Session<xr::Vulkan>,
    waiter: xr::FrameWaiter,
    stream: xr::FrameStream<xr::Vulkan>,
    local: xr::Space,
    view: xr::Space,
    events: xr::EventDataBuffer,
    running: bool,
    focused: bool,
    eyes: [blit::EyeChain; 2],
    /// The panel for menus and the HUD (`Panel`), the eyes' size.
    panel: blit::EyeChain,
    panel_drawn: Option<Panel>,
    /// Where the open menu's panel is, in LOCAL space; `None` while no menu is open.
    menu_pose: Option<xr::Posef>,
    /// The game has a menu (or the title screen) open (`set_menu`).
    menu_open: bool,
    size: [u32; 2],
    blit: blit::Blit,
    blit_alpha: blit::Blit,
    device: wgpu::Device,
    actions: Actions,
    pad: Pad,
    /// The frame between `begin_frame` and `end_frame`.
    pending: Option<(xr::FrameState, Vec<xr::View>)>,
    eye_drawn: [bool; 2],
    /// The head's pose in LOCAL space at the last `begin_frame`.
    head: Option<(Quat, Vec3)>,
    /// Where the head started; leaning away from it moves the view.
    anchor: Option<Vec3>,
    /// The player's yaw minus the head's yaw in LOCAL space, in degrees.
    body_yaw: f32,
    /// The yaw `look` last gave the player; `None` until the first look.
    last_yaw: Option<f32>,
    stats: Stats,
}

/// What happened to the frames since the last report, logged every few seconds.
#[derive(Default)]
struct Stats {
    since: Option<std::time::Instant>,
    waited: u32,
    skipped: u32,
    shown: u32,
    blank: u32,
    draws: std::collections::BTreeMap<&'static str, u32>,
    /// The renderer's own counters at the last frame, for the report.
    renderer: Option<String>,
}

impl Stats {
    fn report(&mut self) {
        let now = std::time::Instant::now();
        let since = *self.since.get_or_insert(now);
        if now.duration_since(since) < Duration::from_secs(2) {
            return;
        }
        log::info!(
            "Horizon VR: {:.1}s: {} frames waited, {} skipped by the runtime, {} shown, {} \
             blank (eye not drawn); eye draws {:?}",
            now.duration_since(since).as_secs_f32(),
            self.waited,
            self.skipped,
            self.shown,
            self.blank,
            self.draws
        );
        if let Some(renderer) = &self.renderer {
            log::info!("Horizon VR: renderer: {renderer}");
        }
        *self = Self {
            since: Some(now),
            ..Self::default()
        };
    }
}

/// The renderer's counters for the next report; `text` runs only when one is due soon.
pub fn note_renderer(text: impl FnOnce() -> String) {
    if let Some(s) = xr_state().as_mut().and_then(|x| x.session.as_mut())
        && s.stats.since.is_some_and(|t| t.elapsed() > Duration::from_millis(1900))
    {
        s.stats.renderer = Some(text());
    }
}

/// Counts what the renderer did with an eye, for the frame report.
pub fn note_draw(outcome: &'static str) {
    if let Some(s) = xr_state().as_mut().and_then(|x| x.session.as_mut()) {
        *s.stats.draws.entry(outcome).or_default() += 1;
    }
}

/// Set by `android_main` before the renderer starts.
///
/// # Safety
/// `vm` must be the process's `JavaVM` and `activity` a JNI reference to the activity
/// that will run the game, both valid for the rest of the process.
pub unsafe fn set_android_context(vm: *mut c_void, activity: *mut c_void, marker_dir: PathBuf) {
    let _ = CONTEXT.set(Context {
        vm: vm as usize,
        activity: activity as usize,
        marker_dir,
    });
}

fn marker() -> Option<PathBuf> {
    CONTEXT.get().map(|c| c.marker_dir.join("xr.pending"))
}

/// Whether a VR session drives the frames (its device is open).
pub fn active() -> bool {
    xr_state().as_ref().is_some_and(|x| x.session.is_some())
}

/// The eye image size, once the session exists.
pub fn eye_size() -> Option<[u32; 2]> {
    xr_state().as_ref()?.session.as_ref().map(|s| s.size)
}

/// The Touch controllers as a gamepad, while the session has input focus.
pub fn pad() -> Option<Pad> {
    let guard = xr_state();
    let s = guard.as_ref()?.session.as_ref()?;
    (s.running && s.focused).then_some(s.pad)
}

/// A Vulkan instance and adapter made through OpenXR, or `None` to open the device
/// the usual way (not a headset, no runtime, or VR failed to start last time).
pub fn instance_and_adapter() -> Option<(wgpu::Instance, wgpu::Adapter)> {
    let ctx = CONTEXT.get()?;
    // `HORIZON_XR=0`: flat, as on a phone (A/B checks on a headset).
    if std::env::var("HORIZON_XR").as_deref() == Ok("0") {
        log::info!("Horizon VR: off (HORIZON_XR=0)");
        return None;
    }
    // Tried already (the renderer retries backends): the instance is made once.
    if xr_state().is_some() {
        return None;
    }
    if let Some(marker) = marker()
        && marker.exists()
    {
        let _ = std::fs::remove_file(&marker);
        log::warn!("Horizon VR: the last VR launch never got focus; starting flat this time");
        return None;
    }
    match open_instance(ctx) {
        Ok(opened) => Some(opened),
        Err(e) => {
            log::warn!("Horizon VR: not starting VR: {e}");
            None
        }
    }
}

fn open_instance(ctx: &Context) -> Result<(wgpu::Instance, wgpu::Adapter), String> {
    // Safety: the pointers come from android-activity and live as long as the process.
    let platform =
        unsafe { xr::AndroidPlatformInfo::new(ctx.vm as *mut c_void, ctx.activity as *mut c_void) };
    // Safety: libopenxr_loader.so is the Khronos loader shipped in the APK.
    let entry = unsafe { xr::Entry::load(&platform) }
        .map_err(|e| format!("no OpenXR loader: {e}"))?;
    let available = entry
        .enumerate_extensions()
        .map_err(|e| format!("OpenXR extensions: {e}"))?;
    // XR_KHR_vulkan_enable, not ..._enable2: wgpu makes the instance and the device itself
    // as on a phone, with the runtime's extensions added. Made by the runtime
    // (xrCreateVulkanDeviceKHR), the device drew no terrain on Quest: the indirect draws'
    // features did not reach it.
    if !available.khr_vulkan_enable {
        return Err("the OpenXR runtime has no XR_KHR_vulkan_enable".into());
    }
    let mut wanted = xr::ExtensionSet::default();
    wanted.khr_vulkan_enable = true;
    wanted.khr_android_create_instance = true;
    let instance = entry
        .create_instance(
            &xr::ApplicationInfo {
                application_name: "Horizon",
                application_version: 1,
                engine_name: "Horizon",
                engine_version: 1,
                api_version: xr::Version::new(1, 0, 0),
            },
            &wanted,
            &[],
            &platform,
        )
        .map_err(|e| format!("OpenXR instance: {e}"))?;
    let props = instance.properties().map_err(|e| e.to_string())?;
    log::info!(
        "Horizon VR: OpenXR runtime {} {}",
        props.runtime_name,
        props.runtime_version
    );
    let system = instance
        .system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)
        .map_err(|e| format!("no headset: {e}"))?;
    let reqs = instance
        .graphics_requirements::<xr::Vulkan>(system)
        .map_err(|e| format!("Vulkan requirements: {e}"))?;

    // Safety: loads the system's libvulkan.so.
    let vk_entry = unsafe { ash::Entry::load() }.map_err(|e| format!("no Vulkan: {e}"))?;
    let offered = unsafe { vk_entry.try_enumerate_instance_version() }
        .ok()
        .flatten()
        .unwrap_or(vk::API_VERSION_1_0);
    let version = offered.min(vk::API_VERSION_1_3);
    let xr_version = xr::Version::new(
        vk::api_version_major(version) as u16,
        vk::api_version_minor(version) as u16,
        0,
    );
    if xr_version < reqs.min_api_version_supported {
        return Err(format!(
            "the runtime needs Vulkan {}, the device offers {xr_version}",
            reqs.min_api_version_supported
        ));
    }
    let flags = wgpu::InstanceFlags::empty();
    let mut extensions = hv::Instance::desired_extensions(&vk_entry, version, flags)
        .map_err(|e| format!("Vulkan extensions: {e:?}"))?;
    let runtime_instance = instance
        .vulkan_legacy_instance_extensions(system)
        .map_err(|e| format!("the runtime's Vulkan instance extensions: {e}"))?;
    for name in leak_names(&runtime_instance) {
        if !extensions.contains(&name) {
            extensions.push(name);
        }
    }
    let device_extensions = leak_names(
        &instance
            .vulkan_legacy_device_extensions(system)
            .map_err(|e| format!("the runtime's Vulkan device extensions: {e}"))?,
    );
    log::info!(
        "Horizon VR: the runtime asks for Vulkan instance extensions [{runtime_instance}] and \
         device extensions {device_extensions:?}"
    );
    let extension_names: Vec<*const std::ffi::c_char> = extensions.iter().map(|e| e.as_ptr()).collect();
    let app_info = vk::ApplicationInfo::default()
        .application_name(c"Horizon")
        .engine_name(c"Horizon")
        .api_version(version);
    let create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&extension_names);
    // Safety: a valid create info, its extensions offered by the loader.
    let vk_instance = unsafe { vk_entry.create_instance(&create_info, None) }
        .map_err(|e| format!("Vulkan instance: {e:?}"))?;
    // Safety: created from `vk_entry` with `version`, `extensions` and `flags`; wgpu-hal
    // owns it from here.
    let hal_instance = unsafe {
        hv::Instance::from_raw(
            vk_entry.clone(),
            vk_instance.clone(),
            version,
            android_sdk_version(),
            None,
            extensions,
            flags,
            Default::default(),
            false,
            None,
        )
    }
    .map_err(|e| format!("wgpu instance: {e:?}"))?;
    // Safety: the instance came from `create_vulkan_instance` for this system.
    let raw_device =
        unsafe { instance.vulkan_graphics_device(system, vk_instance.handle().as_raw() as _) }
            .map_err(|e| format!("the headset's GPU: {e}"))?;
    let physical_device = vk::PhysicalDevice::from_raw(raw_device as u64);
    let exposed = hal_instance
        .expose_adapter(physical_device)
        .ok_or("wgpu can't use the headset's GPU")?;
    // Safety: the adapter belongs to this instance.
    let wgpu_instance = unsafe { wgpu::Instance::from_hal::<Vulkan>(hal_instance) };
    let adapter = unsafe { wgpu_instance.create_adapter_from_hal(exposed) };
    *xr_state() = Some(Xr {
        instance,
        system,
        vk_instance,
        physical_device,
        device_extensions,
        session: None,
    });
    if let Some(marker) = marker() {
        let _ = std::fs::write(marker, b"1");
    }
    log::info!("Horizon VR: Vulkan {xr_version} instance ready through OpenXR");
    Ok((wgpu_instance, adapter))
}

/// A space-separated list of extension names, kept for the whole process (wgpu-hal
/// takes `&'static CStr`s; it is read once).
fn leak_names(list: &str) -> Vec<&'static CStr> {
    list.split_whitespace()
        .filter_map(|name| CString::new(name).ok())
        .map(|name| &*Box::leak(name.into_boxed_c_str()))
        .collect()
}

fn android_sdk_version() -> u32 {
    std::process::Command::new("getprop")
        .arg("ro.build.version.sdk")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Opens the device for `adapter` (from [`instance_and_adapter`]) through OpenXR and
/// starts the VR session on it. `None` when the adapter is not VR's.
pub fn open_device(
    adapter: &wgpu::Adapter,
    desc: &wgpu::DeviceDescriptor<'_>,
) -> Option<Result<(wgpu::Device, wgpu::Queue), wgpu::RequestDeviceError>> {
    let mut guard = xr_state();
    let xr = guard.as_mut()?;
    let opened = match open_vulkan_device(xr, adapter, desc) {
        Ok(opened) => opened,
        Err(e) => {
            log::error!("Horizon VR: the device did not open: {e}");
            return None;
        }
    };
    let (device, queue, raw_device) = match opened {
        Ok(opened) => opened,
        Err(e) => return Some(Err(e)),
    };
    match start_session(xr, &device, raw_device) {
        Ok(session) => xr.session = Some(session),
        Err(e) => log::error!("Horizon VR: no session: {e}"),
    }
    Some(Ok((device, queue)))
}

type Opened = Result<(wgpu::Device, wgpu::Queue, vk::Device), wgpu::RequestDeviceError>;

fn open_vulkan_device(
    xr: &Xr,
    adapter: &wgpu::Adapter,
    desc: &wgpu::DeviceDescriptor<'_>,
) -> Result<Opened, String> {
    let (open, handle) = {
        // Safety: read and opened from here; dropped before wgpu takes the adapter back.
        let hal = unsafe { adapter.as_hal::<Vulkan>() }.ok_or("not a Vulkan adapter")?;
        let wanted = xr.device_extensions.clone();
        // Safety: what wgpu's own request_device does, with the runtime's extensions.
        let open = unsafe {
            hal.open_with_callback(
                desc.required_features,
                &desc.required_limits,
                &desc.memory_hints,
                Some(Box::new(move |args: hv::CreateDeviceCallbackArgs| {
                    for name in &wanted {
                        if !args.extensions.contains(name) {
                            args.extensions.push(name);
                        }
                    }
                })),
            )
        }
        .map_err(|e| format!("Vulkan device: {e:?}"))?;
        let handle = open.device.raw_device().handle();
        (open, handle)
    };
    // Safety: opened from this adapter with exactly `desc`'s features.
    Ok(unsafe { adapter.create_device_from_hal(open, desc) }.map(|(d, q)| (d, q, handle)))
}

fn start_session(xr: &Xr, device: &wgpu::Device, raw_device: vk::Device) -> Result<Session, String> {
    // Safety: the handles are the ones OpenXR made; the queue is family 0, index 0.
    let (session, waiter, stream) = unsafe {
        xr.instance.create_session::<xr::Vulkan>(
            xr.system,
            &xr::vulkan::SessionCreateInfo {
                instance: xr.vk_instance.handle().as_raw() as _,
                physical_device: xr.physical_device.as_raw() as _,
                device: raw_device.as_raw() as _,
                queue_family_index: 0,
                queue_index: 0,
            },
        )
    }
    .map_err(|e| format!("session: {e}"))?;
    let local = session
        .create_reference_space(xr::ReferenceSpaceType::LOCAL, xr::Posef::IDENTITY)
        .map_err(|e| format!("LOCAL space: {e}"))?;
    let view = session
        .create_reference_space(xr::ReferenceSpaceType::VIEW, xr::Posef::IDENTITY)
        .map_err(|e| format!("VIEW space: {e}"))?;
    let actions = Actions::new(&xr.instance).map_err(|e| format!("controllers: {e}"))?;
    session
        .attach_action_sets(&[&actions.set])
        .map_err(|e| format!("controllers: {e}"))?;
    let views = xr
        .instance
        .enumerate_view_configuration_views(xr.system, VIEW_TYPE)
        .map_err(|e| format!("views: {e}"))?;
    let recommended = [
        views[0].recommended_image_rect_width,
        views[0].recommended_image_rect_height,
    ];
    let size = recommended.map(|side| ((side as f32 * RESOLUTION_SCALE) as u32 / 8 * 8).max(64));
    let formats = session
        .enumerate_swapchain_formats()
        .map_err(|e| format!("swapchain formats: {e}"))?;
    let format = blit::pick_format(&formats).ok_or_else(|| {
        format!("no swapchain format the game can draw into (offered {formats:?})")
    })?;
    log::info!(
        "Horizon VR: eyes {}x{} (recommended {}x{}), swapchain {:?}",
        size[0],
        size[1],
        recommended[0],
        recommended[1],
        format.1
    );
    let eyes = [
        blit::EyeChain::new(&session, device, format, size)?,
        blit::EyeChain::new(&session, device, format, size)?,
    ];
    let panel = blit::EyeChain::new(&session, device, format, size)?;
    Ok(Session {
        session,
        waiter,
        stream,
        local,
        view,
        events: xr::EventDataBuffer::new(),
        running: false,
        focused: false,
        eyes,
        panel,
        panel_drawn: None,
        menu_pose: None,
        menu_open: false,
        size,
        blit: blit::Blit::new(device, format.1, false),
        blit_alpha: blit::Blit::new(device, format.1, true),
        device: device.clone(),
        actions,
        pad: Pad::default(),
        pending: None,
        eye_drawn: [false; 2],
        head: None,
        anchor: None,
        body_yaw: 0.0,
        last_yaw: None,
        stats: Stats::default(),
    })
}

/// Waits for the headset's next frame and returns where the eyes will be. `None`
/// when nothing is to be drawn this time (the session is not running, or the runtime
/// skips the frame); otherwise the caller draws both eyes, [`submit_eye`]s each and
/// calls [`end_frame`].
pub fn begin_frame() -> Option<FrameViews> {
    let mut guard = xr_state();
    let xr = guard.as_mut()?;
    let s = xr.session.as_mut()?;
    poll_events(&xr.instance, s);
    if !s.running {
        drop(guard);
        // Not shown: don't spin the game loop.
        std::thread::sleep(Duration::from_millis(20));
        return None;
    }
    s.stats.report();
    let state = match s.waiter.wait() {
        Ok(state) => state,
        Err(e) => {
            log::warn!("Horizon VR: xrWaitFrame: {e}");
            return None;
        }
    };
    if let Err(e) = s.stream.begin() {
        log::warn!("Horizon VR: xrBeginFrame: {e}");
        return None;
    }
    s.stats.waited += 1;
    if s.focused {
        s.pad = s.actions.read(&s.session).unwrap_or_default();
    }
    let located = state
        .should_render
        .then(|| {
            let (_, views) = s
                .session
                .locate_views(VIEW_TYPE, state.predicted_display_time, &s.local)
                .ok()?;
            let head = s.view.locate(&s.local, state.predicted_display_time).ok()?;
            Some((views, head))
        })
        .flatten();
    let Some((views, head)) = located.filter(|(views, _)| views.len() == 2) else {
        s.stats.skipped += 1;
        let _ = s
            .stream
            .end(state.predicted_display_time, xr::EnvironmentBlendMode::OPAQUE, &[]);
        return None;
    };
    let head_rotation = quat(head.pose.orientation);
    let head_position = vec3(head.pose.position);
    s.head = Some((head_rotation, head_position));
    let anchor = *s.anchor.get_or_insert(head_position);
    // Leaning moves the view, up to MAX_LEAN from where the head started.
    let lean = (head_position - anchor).clamp_length_max(MAX_LEAN);
    let body = body_rotation(s.body_yaw);
    let eyes = [0, 1].map(|i| {
        let v = &views[i];
        Eye {
            rotation: body * quat(v.pose.orientation),
            offset: body * (vec3(v.pose.position) - head_position + lean),
            fov: [
                v.fov.angle_left.tan(),
                v.fov.angle_right.tan(),
                v.fov.angle_up.tan(),
                v.fov.angle_down.tan(),
            ],
        }
    });
    if s.menu_open && s.menu_pose.is_none() {
        s.menu_pose = Some(menu_pose(head_rotation, head_position));
    }
    s.pending = Some((state, views));
    s.eye_drawn = [false; 2];
    s.panel_drawn = None;
    Some(FrameViews { eyes, size: s.size })
}

/// Copies `frame` (the offscreen target the eye was drawn into) into eye `eye`'s
/// swapchain image.
pub fn submit_eye(eye: usize, queue: &wgpu::Queue, frame: &wgpu::Texture) {
    let mut guard = xr_state();
    let Some(s) = guard.as_mut().and_then(|x| x.session.as_mut()) else {
        return;
    };
    if s.pending.is_none() || eye > 1 {
        return;
    }
    match s.eyes[eye].draw(&s.device, queue, &s.blit, frame) {
        Ok(()) => s.eye_drawn[eye] = true,
        Err(e) => log::warn!("Horizon VR: eye {eye}: {e}"),
    }
}

/// Whether the game has a menu open (the title screen, a loading screen, pause, the
/// inventory...): the frame then goes to a panel in the room, and the head does not turn
/// the player.
pub fn set_menu(open: bool) {
    if let Some(s) = xr_state().as_mut().and_then(|x| x.session.as_mut()) {
        s.menu_open = open;
        if !open {
            s.menu_pose = None;
        }
    }
}

/// See [`set_menu`].
pub fn menu_open() -> bool {
    xr_state()
        .as_ref()
        .and_then(|x| x.session.as_ref())
        .is_some_and(|s| s.menu_open)
}

/// Copies `frame` into the panel (`Panel`) for the frame `begin_frame` started.
pub fn submit_panel(kind: Panel, queue: &wgpu::Queue, frame: &wgpu::Texture) {
    let mut guard = xr_state();
    let Some(s) = guard.as_mut().and_then(|x| x.session.as_mut()) else {
        return;
    };
    if s.pending.is_none() {
        return;
    }
    if kind == Panel::Room && s.menu_pose.is_none() {
        s.menu_pose = s.head.map(|(rotation, position)| menu_pose(rotation, position));
    }
    match s.panel.draw(&s.device, queue, &s.blit_alpha, frame) {
        Ok(()) => s.panel_drawn = Some(kind),
        Err(e) => log::warn!("Horizon VR: panel: {e}"),
    }
}

/// A menu panel facing the head, `MENU_DISTANCE` ahead of it, upright.
fn menu_pose(head: Quat, position: Vec3) -> xr::Posef {
    let f = head * Vec3::NEG_Z;
    let turn = Quat::from_rotation_y((-f.x).atan2(-f.z));
    let at = position + turn * Vec3::new(0.0, 0.0, -MENU_DISTANCE);
    pose(turn, at)
}

fn pose(rotation: Quat, position: Vec3) -> xr::Posef {
    xr::Posef {
        orientation: xr::Quaternionf {
            x: rotation.x,
            y: rotation.y,
            z: rotation.z,
            w: rotation.w,
        },
        position: xr::Vector3f {
            x: position.x,
            y: position.y,
            z: position.z,
        },
    }
}

/// Shows the frame `begin_frame` started. An eye the game did not draw shows dark red,
/// so a frame that never came is told apart from a black one; a menu frame shows only
/// its panel.
pub fn end_frame(queue: &wgpu::Queue) {
    let mut guard = xr_state();
    let Some(s) = guard.as_mut().and_then(|x| x.session.as_mut()) else {
        return;
    };
    let Some((state, views)) = s.pending.take() else {
        return;
    };
    let rect = xr::Rect2Di {
        offset: xr::Offset2Di { x: 0, y: 0 },
        extent: xr::Extent2Di {
            width: s.size[0] as i32,
            height: s.size[1] as i32,
        },
    };
    for eye in 0..2 {
        if !s.eye_drawn[eye] {
            s.stats.blank += 1;
            match s.eyes[eye].fill(&s.device, queue, wgpu::Color { r: 0.25, g: 0.0, b: 0.0, a: 1.0 }) {
                Ok(()) => s.eye_drawn[eye] = true,
                Err(e) => log::warn!("Horizon VR: eye {eye}: {e}"),
            }
        }
    }
    let aspect = s.size[1] as f32 / s.size[0] as f32;
    let result = if s.eye_drawn == [true; 2] {
        s.stats.shown += 1;
        let projection_views = [0, 1].map(|i| {
            xr::CompositionLayerProjectionView::new()
                .pose(views[i].pose)
                .fov(views[i].fov)
                .sub_image(
                    xr::SwapchainSubImage::new()
                        .swapchain(&s.eyes[i].handle)
                        .image_array_index(0)
                        .image_rect(rect),
                )
        });
        let layer = xr::CompositionLayerProjection::new()
            .space(&s.local)
            .views(&projection_views);
        let (space, at, width) = match s.panel_drawn {
            Some(Panel::Room) => (
                &s.local,
                s.menu_pose
                    .unwrap_or_else(|| pose(Quat::IDENTITY, Vec3::new(0.0, 0.0, -MENU_DISTANCE))),
                MENU_WIDTH,
            ),
            _ => (
                &s.view,
                pose(Quat::IDENTITY, Vec3::new(0.0, 0.0, -HUD_DISTANCE)),
                HUD_WIDTH,
            ),
        };
        let panel = xr::CompositionLayerQuad::new()
            .space(space)
            .layer_flags(xr::CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA)
            .eye_visibility(xr::EyeVisibility::BOTH)
            .sub_image(
                xr::SwapchainSubImage::new()
                    .swapchain(&s.panel.handle)
                    .image_array_index(0)
                    .image_rect(rect),
            )
            .pose(at)
            .size(xr::Extent2Df {
                width,
                height: width * aspect,
            });
        if s.panel_drawn.is_some() {
            s.stream.end(
                state.predicted_display_time,
                xr::EnvironmentBlendMode::OPAQUE,
                &[&layer, &panel],
            )
        } else {
            s.stream.end(
                state.predicted_display_time,
                xr::EnvironmentBlendMode::OPAQUE,
                &[&layer],
            )
        }
    } else {
        s.stream
            .end(state.predicted_display_time, xr::EnvironmentBlendMode::OPAQUE, &[])
    };
    if let Err(e) = result {
        log::warn!("Horizon VR: xrEndFrame: {e}");
    }
}

/// The player's yaw and pitch from the head, given the game's current ones. Turns
/// the game made since the last call (a stick, the server) turn the body.
pub fn look(yaw: f32) -> Option<(f32, f32)> {
    let mut guard = xr_state();
    let s = guard.as_mut()?.session.as_mut()?;
    let (head, _) = s.head?;
    if s.menu_open {
        // The menu stays where it is; the player turns to the head once it closes.
        return None;
    }
    match s.last_yaw {
        // The first look keeps the player facing where they were.
        None => s.body_yaw = yaw - yaw_pitch(body_rotation(0.0) * head).0,
        Some(last) => s.body_yaw += wrap_degrees(yaw - last),
    }
    let (new_yaw, new_pitch) = yaw_pitch(body_rotation(s.body_yaw) * head);
    let new_yaw = yaw + wrap_degrees(new_yaw - yaw);
    s.last_yaw = Some(new_yaw);
    Some((new_yaw, new_pitch))
}

/// LOCAL space to world for a body facing `yaw` (Minecraft degrees).
fn body_rotation(yaw: f32) -> Quat {
    Quat::from_rotation_y(std::f32::consts::PI - yaw.to_radians())
}

/// Minecraft's yaw and pitch (degrees) of an orientation's forward (-Z).
fn yaw_pitch(rotation: Quat) -> (f32, f32) {
    let f = rotation * Vec3::NEG_Z;
    (
        (-f.x).atan2(f.z).to_degrees(),
        (-f.y).clamp(-1.0, 1.0).asin().to_degrees(),
    )
}

fn wrap_degrees(d: f32) -> f32 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

fn quat(q: xr::Quaternionf) -> Quat {
    Quat::from_xyzw(q.x, q.y, q.z, q.w).normalize()
}

fn vec3(v: xr::Vector3f) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn poll_events(instance: &xr::Instance, s: &mut Session) {
    loop {
        let event = match instance.poll_event(&mut s.events) {
            Ok(Some(event)) => event,
            Ok(None) => break,
            Err(e) => {
                log::warn!("Horizon VR: xrPollEvent: {e}");
                break;
            }
        };
        match event {
            xr::Event::SessionStateChanged(e) => {
                let state = e.state();
                log::info!("Horizon VR: session {state:?}");
                match state {
                    xr::SessionState::READY => match s.session.begin(VIEW_TYPE) {
                        Ok(_) => s.running = true,
                        Err(e) => log::error!("Horizon VR: xrBeginSession: {e}"),
                    },
                    xr::SessionState::STOPPING => {
                        s.running = false;
                        s.focused = false;
                        if let Err(e) = s.session.end() {
                            log::warn!("Horizon VR: xrEndSession: {e}");
                        }
                    }
                    xr::SessionState::FOCUSED => {
                        s.focused = true;
                        // VR got going: the next launch starts in VR again.
                        if let Some(marker) = marker() {
                            let _ = std::fs::remove_file(marker);
                        }
                    }
                    xr::SessionState::VISIBLE => s.focused = false,
                    xr::SessionState::EXITING | xr::SessionState::LOSS_PENDING => {
                        s.running = false;
                        s.focused = false;
                    }
                    _ => {}
                }
            }
            xr::Event::ReferenceSpaceChangePending(_) => {
                // Recentred: the head starts over where it is, facing where the player faces.
                s.anchor = None;
                s.last_yaw = None;
            }
            xr::Event::EventsLost(e) => {
                log::warn!("Horizon VR: {} events lost", e.lost_event_count());
            }
            _ => {}
        }
    }
}

/// The Touch controllers' buttons, sticks and triggers.
struct Actions {
    set: xr::ActionSet,
    bools: Vec<xr::Action<bool>>,
    sticks: [xr::Action<xr::Vector2f>; 2],
    triggers: [xr::Action<f32>; 2],
    grips: [xr::Action<f32>; 2],
}

/// The buttons in `Actions::bools` order, with their Touch controller paths.
const BUTTONS: [(&str, &str); 7] = [
    ("a", "/user/hand/right/input/a/click"),
    ("b", "/user/hand/right/input/b/click"),
    ("x", "/user/hand/left/input/x/click"),
    ("y", "/user/hand/left/input/y/click"),
    ("menu", "/user/hand/left/input/menu/click"),
    ("left_stick_click", "/user/hand/left/input/thumbstick/click"),
    ("right_stick_click", "/user/hand/right/input/thumbstick/click"),
];

impl Actions {
    fn new(instance: &xr::Instance) -> xr::Result<Self> {
        let set = instance.create_action_set("gameplay", "Gameplay", 0)?;
        let bools = BUTTONS
            .iter()
            .map(|(name, _)| set.create_action::<bool>(name, name, &[]))
            .collect::<xr::Result<Vec<_>>>()?;
        let sticks = [
            set.create_action::<xr::Vector2f>("left_stick", "Left stick", &[])?,
            set.create_action::<xr::Vector2f>("right_stick", "Right stick", &[])?,
        ];
        let triggers = [
            set.create_action::<f32>("left_trigger", "Left trigger", &[])?,
            set.create_action::<f32>("right_trigger", "Right trigger", &[])?,
        ];
        let grips = [
            set.create_action::<f32>("left_grip", "Left grip", &[])?,
            set.create_action::<f32>("right_grip", "Right grip", &[])?,
        ];
        let path = |p: &str| instance.string_to_path(p);
        let mut bindings = Vec::new();
        for (action, (_, p)) in bools.iter().zip(BUTTONS) {
            bindings.push(xr::Binding::new(action, path(p)?));
        }
        for (i, hand) in ["left", "right"].into_iter().enumerate() {
            bindings.push(xr::Binding::new(
                &sticks[i],
                path(&format!("/user/hand/{hand}/input/thumbstick"))?,
            ));
            bindings.push(xr::Binding::new(
                &triggers[i],
                path(&format!("/user/hand/{hand}/input/trigger/value"))?,
            ));
            bindings.push(xr::Binding::new(
                &grips[i],
                path(&format!("/user/hand/{hand}/input/squeeze/value"))?,
            ));
        }
        instance.suggest_interaction_profile_bindings(
            path("/interaction_profiles/oculus/touch_controller")?,
            &bindings,
        )?;
        Ok(Self {
            set,
            bools,
            sticks,
            triggers,
            grips,
        })
    }

    fn read(&self, session: &xr::Session<xr::Vulkan>) -> xr::Result<Pad> {
        session.sync_actions(&[(&self.set).into()])?;
        let b = |i: usize| -> xr::Result<bool> {
            Ok(self.bools[i].state(session, xr::Path::NULL)?.current_state)
        };
        let stick = |i: usize| -> xr::Result<[f32; 2]> {
            let v = self.sticks[i].state(session, xr::Path::NULL)?.current_state;
            Ok([v.x, v.y])
        };
        let f = |a: &xr::Action<f32>| -> xr::Result<f32> {
            Ok(a.state(session, xr::Path::NULL)?.current_state)
        };
        Ok(Pad {
            a: b(0)?,
            b: b(1)?,
            x: b(2)?,
            y: b(3)?,
            menu: b(4)?,
            left_click: b(5)?,
            right_click: b(6)?,
            left_grip: f(&self.grips[0])? > 0.5,
            right_grip: f(&self.grips[1])? > 0.5,
            left: stick(0)?,
            right: stick(1)?,
            triggers: [f(&self.triggers[0])?, f(&self.triggers[1])?],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_head_looking_ahead_faces_the_body() {
        for yaw in [0.0f32, 90.0, -45.0, 180.0] {
            let (y, p) = yaw_pitch(body_rotation(yaw));
            assert!(wrap_degrees(y - yaw).abs() < 1e-3, "{yaw} -> {y}");
            assert!(p.abs() < 1e-3);
        }
    }

    #[test]
    fn looking_down_is_positive_pitch() {
        let down = Quat::from_rotation_x(-0.5);
        let (_, p) = yaw_pitch(body_rotation(0.0) * down);
        assert!((p - 0.5f32.to_degrees()).abs() < 1e-3, "{p}");
    }

    #[test]
    fn the_screen_camera_and_the_body_rotation_agree() {
        // camera::view_matrix is the conjugate of Ry(PI - yaw) * Rx(-pitch).
        let yaw = 37.0f32;
        let pitch = 20.0f32;
        let q = body_rotation(yaw) * Quat::from_rotation_x(-pitch.to_radians());
        let (y, p) = yaw_pitch(q);
        assert!((y - yaw).abs() < 1e-3 && (p - pitch).abs() < 1e-3, "{y} {p}");
    }
}
