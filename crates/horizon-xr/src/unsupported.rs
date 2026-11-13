//! Targets with no OpenXR backend. Every call reports that VR is off.

use glam::Vec3;
use std::path::PathBuf;

/// Where the hotbar is in the HUD's image (pixels from the top left).
#[derive(Clone, Copy, Debug)]
pub struct Hotbar {
    /// x, y, width, height of what the wrist shows (the hotbar, health, food, armour).
    pub crop: [f32; 4],
    /// The left edge and top of the first slot, a slot's width and the row's height.
    pub slots: [f32; 4],
}

/// Where the HUD's panel shows this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    /// Fixed in the room where the head faced when the menu opened.
    Room,
    /// Before the eyes, moving with the head.
    Head,
}

/// The headset controllers read as a gamepad.
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
    pub rotation: glam::Quat,
    pub offset: Vec3,
    pub fov: [f32; 4],
}

/// A controller's pose for the frame, in Minecraft's world axes.
#[derive(Clone, Copy, Debug)]
pub struct HandPose {
    pub rotation: glam::Quat,
    pub offset: Vec3,
}

/// The controllers, in `Hands` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hand {
    LeftGrip = 0,
    RightGrip = 1,
    LeftAim = 2,
    RightAim = 3,
}

/// What the renderer needs for a frame: both eyes and the eye image size.
#[derive(Clone, Copy, Debug)]
pub struct FrameViews {
    pub eyes: [Eye; 2],
    pub size: [u32; 2],
    pub hands: [Option<HandPose>; 4],
}

/// What `note_input` counts.
#[derive(Clone, Copy, Debug)]
pub enum InputKind {
    AndroidKey = 0,
    AndroidMotion = 1,
    PadHeld = 2,
}

pub fn note_controller_setting(_: bool) {}
pub fn note_input(_: InputKind) {}
pub fn note_renderer(_: impl FnOnce() -> String) {}
pub fn note_draw(_: &'static str) {}

/// No activity to attach to on this target.
pub unsafe fn set_android_context(_: *mut std::ffi::c_void, _: *mut std::ffi::c_void, _: PathBuf) {}

pub fn active() -> bool {
    false
}
pub fn eye_size() -> Option<[u32; 2]> {
    None
}
pub fn pad() -> Option<Pad> {
    None
}
pub fn instance_and_adapter() -> Option<(wgpu::Instance, wgpu::Adapter)> {
    None
}
pub fn open_device(
    _: &wgpu::Adapter,
    _: &wgpu::DeviceDescriptor<'_>,
) -> Option<Result<(wgpu::Device, wgpu::Queue), wgpu::RequestDeviceError>> {
    None
}
pub fn begin_frame() -> Option<FrameViews> {
    None
}
pub fn room_hands() -> Option<[Option<Vec3>; 4]> {
    None
}
pub fn last_hands() -> Option<[Option<HandPose>; 4]> {
    None
}
pub fn set_hotbar(_: Option<Hotbar>) {}
pub fn wrist_slot() -> Option<usize> {
    None
}
pub fn move_turn() -> f32 {
    0.0
}
pub fn set_aim_points(_: [Option<(Vec3, Option<Vec3>)>; 2]) {}
pub fn set_look_hand(_: bool) {}
pub fn head_look() -> Option<(f32, f32, Vec3)> {
    None
}
pub fn hand_angles(_: bool) -> Option<(f32, f32)> {
    None
}
pub fn right_grip_held() -> bool {
    false
}
pub fn pointer() -> Option<([f32; 2], bool)> {
    None
}
pub fn submit_eye(_: usize, _: &wgpu::Queue, _: &wgpu::Texture) {}
pub fn set_menu(_: bool) {}
pub fn menu_open() -> bool {
    false
}
pub fn submit_panel(_: Panel, _: &wgpu::Queue, _: &wgpu::Texture) {}
pub fn end_frame(_: &wgpu::Queue) {}
pub fn look(_: f32) -> Option<(f32, f32)> {
    None
}
