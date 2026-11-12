//! Hardware input on Android: game controllers, a mouse and a keyboard.
//!
//! winit has no gamepad events and reads every motion event as a touch, so the
//! patched winit (third_party/winit) hands each input event to [`filter`] first.
//! Controller buttons and axes become a [`PadState`] that `pad.rs` reads as its
//! hardware pad, so a controller plays like one on the desktop (bindings, menu
//! cursor, deadzones). Mouse events are queued for the game thread
//! (`Game::android_input_frame`). Keyboard keys stay with winit, which already
//! maps them to the physical keys the game handles; they are only noted so the
//! touch controls can hide.
use horizon_client::gamepad::{Button, PadState};
use std::sync::Mutex;
use winit::platform::android::activity::input::{Axis, InputEvent, KeyAction, Keycode, MotionAction};

/// `InputDevice.SOURCE_*` bits.
const SOURCE_KEYBOARD: u32 = 0x0000_0101;
const SOURCE_DPAD: u32 = 0x0000_0201;
const SOURCE_GAMEPAD: u32 = 0x0000_0401;
const SOURCE_JOYSTICK: u32 = 0x0100_0010;
const SOURCE_MOUSE: u32 = 0x0000_2002;
const SOURCE_MOUSE_RELATIVE: u32 = 0x0002_0004;

/// What the game thread gets from a mouse.
#[derive(Clone, Copy, Debug)]
pub(super) enum MouseInput {
    /// The pointer, in window pixels.
    Move([f64; 2]),
    /// Movement while the pointer is captured, in pixels.
    Delta([f64; 2]),
    /// 0 left, 1 right, 2 middle, 3 back, 4 forward.
    Button(u8, bool),
    /// Wheel clicks, up and right positive.
    Wheel([f32; 2]),
}

#[derive(Default)]
struct Shared {
    /// Buttons held on the controller's keys.
    keys: u32,
    /// Analog trigger buttons held as keys (L2, R2).
    key_triggers: [bool; 2],
    left: [f32; 2],
    z_rz: [f32; 2],
    rx_ry: [f32; 2],
    /// Analog triggers: LTRIGGER/RTRIGGER and BRAKE/GAS.
    triggers: [f32; 2],
    /// The hat (the D-pad on most controllers) as buttons.
    hat: u32,
    /// The right stick shows up on RX/RY (some DirectInput controllers); Z/RZ
    /// then carry the triggers.
    rx_ry_stick: bool,
    /// Trigger axes have moved (LTRIGGER, RTRIGGER, GAS, BRAKE).
    trigger_axes: bool,
    mouse: Vec<MouseInput>,
    mouse_buttons: u32,
    /// A primary click without a button state is held.
    tap: bool,
    keyboard_used: bool,
    mouse_used: bool,
    /// Escape (or Back) presses from a hardware keyboard.
    escapes: u32,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared {
    keys: 0,
    key_triggers: [false; 2],
    left: [0.0; 2],
    z_rz: [0.0; 2],
    rx_ry: [0.0; 2],
    triggers: [0.0; 2],
    hat: 0,
    rx_ry_stick: false,
    trigger_axes: false,
    mouse: Vec::new(),
    mouse_buttons: 0,
    tap: false,
    keyboard_used: false,
    mouse_used: false,
    escapes: 0,
});

fn shared() -> std::sync::MutexGuard<'static, Shared> {
    SHARED.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Route input events through [`filter`]; called once before the event loop starts.
pub(super) fn install() {
    winit::platform::android::set_input_filter(filter);
}

/// The controller as the game reads it.
pub(super) fn pad_state() -> PadState {
    let s = shared();
    let right = if s.rx_ry_stick { s.rx_ry } else { s.z_rz };
    let mut triggers = s.triggers;
    if s.rx_ry_stick && !s.trigger_axes {
        triggers = [s.z_rz[0].max(0.0), s.z_rz[1].max(0.0)];
    }
    for i in 0..2 {
        if s.key_triggers[i] {
            triggers[i] = 1.0;
        }
    }
    let pad = PadState { buttons: s.keys | s.hat, left: s.left, right: [right[0], -right[1]], triggers };
    drop(s);
    let pad = match horizon_xr::pad() {
        Some(touch) => with_touch_controllers(pad, touch),
        None => pad,
    };
    if pad.buttons != 0 || pad.left != [0.0; 2] || pad.right != [0.0; 2] || pad.triggers != [0.0; 2] {
        horizon_xr::note_input(horizon_xr::InputKind::PadHeld);
    }
    pad
}

/// VR: the Touch controllers on top of any other controller. Right stick turns (the head
/// looks up and down), left stick walks, A jumps, B goes back, Y opens the inventory,
/// triggers use and attack, the sticks' clicks sprint and sneak, the menu button pauses.
/// X (dropping) and the grips (F5, F3, one item from the carried stack) are `xr_buttons`'.
fn with_touch_controllers(mut pad: PadState, touch: horizon_xr::Pad) -> PadState {
    for (held, button) in [
        (touch.a, Button::South),
        (touch.b, Button::East),
        // Y with X held throws the rest of the stack (`xr_buttons`), it opens nothing.
        (touch.y && !touch.x, Button::North),
        (touch.menu, Button::Start),
        (touch.left_click, Button::LeftStick),
        (touch.right_click, Button::RightStick),
    ] {
        if held {
            pad.set(button, true);
        }
    }
    // The left stick walks where the head faces, though the player's yaw follows the
    // right hand's aim: turned by the difference (Minecraft's input: x left, z ahead).
    let touch = {
        let turn = horizon_xr::move_turn().to_radians();
        let (sin, cos) = turn.sin_cos();
        let (x, z) = (-touch.left[0], touch.left[1]);
        let (x, z) = (x * cos - z * sin, x * sin + z * cos);
        horizon_xr::Pad { left: [-x, z], ..touch }
    };
    // Over the wrist's hotbar, the right trigger picks a slot (`xr_look`), not the pad.
    let touch = if horizon_xr::wrist_slot().is_some() {
        horizon_xr::Pad { triggers: [touch.triggers[0], 0.0], ..touch }
    } else {
        touch
    };
    // In a menu the triggers click where their controllers point (`xr_pointer`), not the
    // pad, and the right stick scrolls there.
    let touch = if horizon_xr::menu_open() {
        horizon_xr::Pad { triggers: [0.0; 2], right: [0.0; 2], ..touch }
    } else {
        touch
    };
    let stronger = |a: [f32; 2], b: [f32; 2]| if b[0].hypot(b[1]) > a[0].hypot(a[1]) { b } else { a };
    pad.left = stronger(pad.left, touch.left);
    pad.right = stronger(pad.right, touch.right);
    pad.triggers = [pad.triggers[0].max(touch.triggers[0]), pad.triggers[1].max(touch.triggers[1])];
    pad
}

/// Everything let go (the app went to the background, a controller went away).
pub(super) fn reset() {
    let mut s = shared();
    s.keys = 0;
    s.key_triggers = [false; 2];
    s.left = [0.0; 2];
    s.z_rz = [0.0; 2];
    s.rx_ry = [0.0; 2];
    s.triggers = [0.0; 2];
    s.hat = 0;
    s.mouse_buttons = 0;
    s.tap = false;
    s.mouse.clear();
}

/// Mouse events since the last call.
pub(super) fn take_mouse() -> Vec<MouseInput> {
    std::mem::take(&mut shared().mouse)
}

/// A hardware keyboard or a mouse was used since the last call.
pub(super) fn take_used() -> (bool, bool) {
    let mut s = shared();
    let used = (s.keyboard_used, s.mouse_used);
    s.keyboard_used = false;
    s.mouse_used = false;
    used
}

fn has(source: u32, bits: u32) -> bool {
    source & bits == bits
}

/// The controller button an Android key code stands for. BUTTON_1..12 are
/// DirectInput controllers without a key layout, in the usual order
/// (1 X, 2 A, 3 B, 4 Y, 5 L1, 6 R1, 7 L2, 8 R2, 9 select, 10 start, 11 L3, 12 R3).
fn pad_button(code: Keycode) -> Option<Button> {
    Some(match code {
        Keycode::ButtonA | Keycode::Button2 => Button::South,
        Keycode::ButtonB | Keycode::Button3 => Button::East,
        Keycode::ButtonX | Keycode::Button1 => Button::West,
        Keycode::ButtonY | Keycode::Button4 => Button::North,
        Keycode::ButtonL1 | Keycode::Button5 => Button::LeftBumper,
        Keycode::ButtonR1 | Keycode::Button6 => Button::RightBumper,
        Keycode::ButtonL2 | Keycode::Button7 => Button::LeftTrigger,
        Keycode::ButtonR2 | Keycode::Button8 => Button::RightTrigger,
        Keycode::ButtonSelect | Keycode::Button9 => Button::Select,
        Keycode::ButtonStart | Keycode::Button10 => Button::Start,
        Keycode::ButtonThumbl | Keycode::Button11 => Button::LeftStick,
        Keycode::ButtonThumbr | Keycode::Button12 => Button::RightStick,
        Keycode::DpadUp => Button::DPadUp,
        Keycode::DpadDown => Button::DPadDown,
        Keycode::DpadLeft => Button::DPadLeft,
        Keycode::DpadRight => Button::DPadRight,
        // A controller's back key (the View/Select button on many).
        Keycode::Back => Button::Select,
        _ => return None,
    })
}

fn is_gamepad_key(code: Keycode) -> bool {
    matches!(
        code,
        Keycode::ButtonA
            | Keycode::ButtonB
            | Keycode::ButtonX
            | Keycode::ButtonY
            | Keycode::ButtonL1
            | Keycode::ButtonR1
            | Keycode::ButtonL2
            | Keycode::ButtonR2
            | Keycode::ButtonSelect
            | Keycode::ButtonStart
            | Keycode::ButtonThumbl
            | Keycode::ButtonThumbr
            | Keycode::Button1
            | Keycode::Button2
            | Keycode::Button3
            | Keycode::Button4
            | Keycode::Button5
            | Keycode::Button6
            | Keycode::Button7
            | Keycode::Button8
            | Keycode::Button9
            | Keycode::Button10
            | Keycode::Button11
            | Keycode::Button12
    )
}

/// Every input event, before winit: `true` takes it.
fn filter(event: &InputEvent<'_>) -> bool {
    match event {
        InputEvent::KeyEvent(key) => {
            let source: u32 = key.source().into();
            let code = key.key_code();
            let down = match key.action() {
                KeyAction::Down => true,
                KeyAction::Up => false,
                _ => return false,
            };
            // A mouse's right button turns into Back when nobody takes it: the
            // button already went to the game.
            if code == Keycode::Back && has(source, SOURCE_MOUSE) {
                return true;
            }
            let from_pad = has(source, SOURCE_GAMEPAD)
                || has(source, SOURCE_JOYSTICK)
                || is_gamepad_key(code)
                || (source == SOURCE_DPAD
                    && matches!(code, Keycode::DpadUp | Keycode::DpadDown | Keycode::DpadLeft | Keycode::DpadRight));
            if from_pad {
                horizon_xr::note_input(horizon_xr::InputKind::AndroidKey);
                let Some(button) = pad_button(code) else {
                    // Home/Mode and the rest stay with the system.
                    return false;
                };
                let mut s = shared();
                match button {
                    Button::LeftTrigger => s.key_triggers[0] = down,
                    Button::RightTrigger => s.key_triggers[1] = down,
                    _ => {
                        if down {
                            s.keys |= button.bit();
                        } else {
                            s.keys &= !button.bit();
                        }
                    }
                }
                return true;
            }
            // A hardware keyboard's Esc: close the
            // screen or pause, as the touch back does.
            if has(source, SOURCE_KEYBOARD)
                && !has(source, SOURCE_MOUSE)
                && code == Keycode::Escape
            {
                let mut s = shared();
                s.keyboard_used = true;
                if down && key.repeat_count() == 0 {
                    s.escapes = 1;
                }
                return true;
            }
            if has(source, SOURCE_KEYBOARD)
                && !matches!(code, Keycode::Back | Keycode::Home | Keycode::VolumeUp | Keycode::VolumeDown | Keycode::VolumeMute | Keycode::Power)
            {
                shared().keyboard_used = true;
            }
            false
        }
        InputEvent::MotionEvent(motion) => {
            let source: u32 = motion.source().into();
            if has(source, SOURCE_JOYSTICK) {
                horizon_xr::note_input(horizon_xr::InputKind::AndroidMotion);
                if motion.action() != MotionAction::Move {
                    return true;
                }
                let p = motion.pointer_at_index(0);
                let a = |axis| {
                    let v = p.axis_value(axis);
                    if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 }
                };
                let mut s = shared();
                s.left = [a(Axis::X), -a(Axis::Y)];
                s.z_rz = [a(Axis::Z), a(Axis::Rz)];
                s.rx_ry = [a(Axis::Rx), a(Axis::Ry)];
                if s.rx_ry[0].abs() > 0.3 || s.rx_ry[1].abs() > 0.3 {
                    s.rx_ry_stick = true;
                }
                let (lt, rt) = (a(Axis::Ltrigger).max(a(Axis::Brake)), a(Axis::Rtrigger).max(a(Axis::Gas)));
                if lt > 0.05 || rt > 0.05 {
                    s.trigger_axes = true;
                }
                s.triggers = [lt.max(0.0), rt.max(0.0)];
                let (hx, hy) = (a(Axis::HatX), a(Axis::HatY));
                let mut hat = 0;
                if hx < -0.5 {
                    hat |= Button::DPadLeft.bit();
                } else if hx > 0.5 {
                    hat |= Button::DPadRight.bit();
                }
                if hy < -0.5 {
                    hat |= Button::DPadUp.bit();
                } else if hy > 0.5 {
                    hat |= Button::DPadDown.bit();
                }
                s.hat = hat;
                return true;
            }
            let relative = has(source, SOURCE_MOUSE_RELATIVE);
            if !relative && !has(source, SOURCE_MOUSE) {
                return false;
            }
            let p = motion.pointer_at_index(0);
            let mut s = shared();
            s.mouse_used = true;
            match motion.action() {
                MotionAction::Scroll => {
                    let w = [p.axis_value(Axis::Hscroll), p.axis_value(Axis::Vscroll)];
                    if w != [0.0, 0.0] {
                        s.mouse.push(MouseInput::Wheel(w));
                    }
                }
                MotionAction::Move | MotionAction::HoverMove | MotionAction::Down | MotionAction::Up => {
                    if relative {
                        let d = [f64::from(p.x()), f64::from(p.y())];
                        if d != [0.0, 0.0] {
                            s.mouse.push(MouseInput::Delta(d));
                        }
                    } else {
                        s.mouse.push(MouseInput::Move([f64::from(p.x()), f64::from(p.y())]));
                    }
                }
                _ => {}
            }
            // Buttons from the state each event carries: press and release events, and
            // the down/up a primary click also sends, all agree on it.
            let mut buttons = motion.button_state().0 & 0x1f;
            // A touchpad tap may come as down/up without a button state: the primary button.
            let action = motion.action();
            if buttons == 0 && action == MotionAction::Down {
                s.tap = true;
            }
            if s.tap {
                if matches!(action, MotionAction::Up | MotionAction::Cancel) {
                    s.tap = false;
                } else {
                    buttons |= 1;
                }
            }
            let changed = buttons ^ s.mouse_buttons;
            for i in 0..5u8 {
                if changed & (1 << i) != 0 {
                    s.mouse.push(MouseInput::Button(i, buttons & (1 << i) != 0));
                }
            }
            s.mouse_buttons = buttons;
            true
        }
        _ => false,
    }
}

impl super::Game<'_> {
    /// Once per loop iteration, before the pads are read: mouse events go to the
    /// game as the window's own would, the pointer is captured while playing with
    /// a mouse, and the touch controls hide while a controller, keyboard or mouse
    /// plays (a touch brings them back, `touch_event`).
    /// VR: the player looks where the head does (`horizon_xr::look`), through
    /// `Entity.turn` like the mouse, so everything that follows a turn still does.
    /// VR: the right controller's tip in the world and its speed in the room (metres a
    /// second). The room's speed is the hand's own: walking, jumping, falling or turning
    /// the body move the tip in the world, not in the room.
    fn xr_right_tip(&self) -> Option<([f64; 3], f64)> {
        use std::time::Instant;
        static LAST: Mutex<Option<(Instant, [f32; 3])>> = Mutex::new(None);
        let now = Instant::now();
        let eye = self.eye_position()?;
        let hand = horizon_xr::last_hands()?[horizon_xr::Hand::RightAim as usize]?;
        let room = horizon_xr::room_hands()?[horizon_xr::Hand::RightAim as usize]?.to_array();
        let tip = [0, 1, 2].map(|i| eye[i] + f64::from(hand.offset[i]));
        let mut last = LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let speed = last.map_or(0.0, |(t, p): (Instant, [f32; 3])| {
            let dt = now.duration_since(t).as_secs_f64().max(1e-3);
            (0..3).map(|i| f64::from(room[i] - p[i]).powi(2)).sum::<f64>().sqrt() / dt
        });
        *last = Some((now, room));
        Some((tip, speed))
    }
    /// VR: a swing of the right controller (at least 2.5 m/s in the room) whose tip meets
    /// a mob hits it, as a click on it would: the player turns to it for a moment and the
    /// attack goes the usual way (reach, cooldown, damage). Returns where to look.
    fn xr_swing(&mut self, hand: Option<([f64; 3], f64)>) -> Option<[f64; 3]> {
        use std::time::{Duration, Instant};
        static TARGET: Mutex<Option<(Instant, [f64; 3])>> = Mutex::new(None);
        static COOLDOWN: Mutex<Option<Instant>> = Mutex::new(None);
        let now = Instant::now();
        let held = *TARGET.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((until, at)) = held
            && now < until
        {
            return Some(at);
        }
        if self.screen != super::Screen::Playing || self.title.is_some() {
            return None;
        }
        let (tip, speed) = hand?;
        let mut cooldown = COOLDOWN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if speed < 2.5 || cooldown.is_some_and(|t| now < t) {
            return None;
        }
        // A mob the tip is in or touching: within 0.6 m of its middle sideways, between its
        // feet and two metres up.
        let target = self
            .mobs
            .iter()
            .filter(|m| m.alive())
            .filter_map(|m| {
                let (x, y, z) = (m.position.x as f64, m.position.y as f64, m.position.z as f64);
                let side = (tip[0] - x).hypot(tip[2] - z);
                (side < 0.6 && tip[1] > y - 0.1 && tip[1] < y + 2.0).then_some(([x, tip[1], z], side))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))?
            .0;
        *cooldown = Some(now + Duration::from_millis(400));
        *TARGET.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((now + Duration::from_millis(150), target));
        let mut s = shared();
        s.mouse.push(MouseInput::Button(0, true));
        s.mouse.push(MouseInput::Button(0, false));
        Some(target)
    }
    /// VR: what the right hand does. A blow into a block (the tip entering it at 2 m/s or
    /// more in the room) mines it, as holding the attack on it does, for a moment after
    /// each blow: keep striking, in any direction, to break it. Resting or moving slowly
    /// does nothing. Food or a potion held near the mouth is eaten or drunk (use held).
    /// Returns where to look while mining.
    fn xr_hand_actions(&mut self, hand: Option<([f64; 3], f64)>) -> Option<[f64; 3]> {
        use std::time::{Duration, Instant};
        static MINING: Mutex<Option<([i32; 3], Instant)>> = Mutex::new(None);
        static EATING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let now = Instant::now();
        let playing = self.screen == super::Screen::Playing && self.title.is_none();
        let place = playing.then_some(()).and(hand).zip(self.eye_position());
        let mut s = shared();
        let mut mining = MINING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(((tip, speed), eye)) = place else {
            if mining.take().is_some() {
                s.mouse.push(MouseInput::Button(0, false));
            }
            if EATING.swap(false, std::sync::atomic::Ordering::Relaxed) {
                s.mouse.push(MouseInput::Button(1, false));
            }
            return None;
        };
        // The mouth: a little below the eyes; anywhere within 30 cm of it will do.
        let mouth = [eye[0], eye[1] - 0.12, eye[2]];
        let near_mouth = (0..3).map(|i| (tip[i] - mouth[i]).powi(2)).sum::<f64>().sqrt() < 0.3;
        let edible = near_mouth
            && self.visible_hand_items()[0].as_ref().is_some_and(|stack| {
                let components = self
                    .pack
                    .item_renderer
                    .models
                    .components(&horizon_client::item_render::stack_view(stack));
                components.get("minecraft:consumable").is_some() || components.get("minecraft:food").is_some()
            });
        if EATING.swap(edible, std::sync::atomic::Ordering::Relaxed) != edible {
            s.mouse.push(MouseInput::Button(1, edible));
        }
        let block = tip.map(|v| v.floor() as i32);
        let struck = !edible
            && speed >= 2.0
            && !self.res.facts().is_air(self.world.block(block[0], block[1], block[2]));
        if struck {
            if mining.is_some_and(|(b, _)| b != block) {
                s.mouse.push(MouseInput::Button(0, false));
            }
            if mining.is_none_or(|(b, _)| b != block) {
                s.mouse.push(MouseInput::Button(0, true));
            }
            *mining = Some((block, now + Duration::from_millis(350)));
        }
        match *mining {
            Some((block, until)) if now < until => Some(block.map(|v| f64::from(v) + 0.5)),
            Some(_) => {
                *mining = None;
                s.mouse.push(MouseInput::Button(0, false));
                None
            }
            None => None,
        }
    }
    pub(super) fn xr_look(&mut self) {
        horizon_xr::set_menu(self.title.is_some() || self.screen != super::Screen::Playing);
        // A swing at a mob first, else the hand's own actions (they track every frame).
        let tip = self.xr_right_tip();
        let swing = self.xr_swing(tip);
        let hand = self.xr_hand_actions(tip);
        if let Some(at) = swing.or(hand)
            && let Some(eye) = self.eye_position()
        {
            // Look at the mob being hit until the swing's click lands.
            let d = [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]];
            let yaw = (-d[0]).atan2(d[2]).to_degrees() as f32;
            let pitch = (-d[1]).atan2(d[0].hypot(d[2])).to_degrees() as f32;
            let yaw = self.yaw + ((yaw - self.yaw + 180.0).rem_euclid(360.0) - 180.0);
            let turn = [f64::from(yaw - self.yaw), f64::from(pitch - self.pitch)].map(|d| d / 0.15);
            self.turn_player(turn[0], turn[1]);
            return;
        }
        horizon_xr::note_controller_setting(self.settings.controller.enabled);
        // The hotbar on the left wrist: where it is in the HUD's image, and the slot the
        // right controller touches there.
        let hotbar = self.renderer.as_ref().filter(|_| self.title.is_none()).map(|r| {
            let scale = horizon_client::hud::HudBatch::new([r.config.width, r.config.height]).scale;
            let (w, h) = (r.config.width as f32, r.config.height as f32);
            let left = w / 2.0 - 91.0 * scale;
            horizon_xr::Hotbar {
                // Up to the held item's name above the bars (Gui's toolHighlight).
                crop: [left - 2.0 * scale, h - 72.0 * scale, 186.0 * scale, 72.0 * scale],
                slots: [left + 1.0 * scale, h - 22.0 * scale, 20.0 * scale, 22.0 * scale],
            }
        });
        horizon_xr::set_hotbar(hotbar);
        // The slot the right controller is over on the wrist, picked when its trigger goes
        // down there (as QuestCraft does).
        static TRIGGER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let trigger = horizon_xr::pad().is_some_and(|p| p.triggers[1] > 0.5);
        let pressed = trigger && !TRIGGER.swap(trigger, std::sync::atomic::Ordering::Relaxed);
        if !trigger {
            TRIGGER.store(false, std::sync::atomic::Ordering::Relaxed);
        }
        if pressed && let Some(slot) = horizon_xr::wrist_slot() {
            self.hotbar_presses.push(slot);
        }
        // The left trigger uses along the left controller: while it is (even lightly) held,
        // the player looks where the left hand points, so the use lands there, and the
        // right hand stays free to attack.
        static LEFT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let left_trigger = horizon_xr::pad().map_or(0.0, |p| p.triggers[0]);
        let playing = self.screen == super::Screen::Playing && self.title.is_none();
        let was_left = LEFT.load(std::sync::atomic::Ordering::Relaxed);
        let look_left = playing && if was_left { left_trigger > 0.08 } else { left_trigger > 0.15 };
        LEFT.store(look_left, std::sync::atomic::Ordering::Relaxed);
        horizon_xr::set_look_hand(look_left);
        // The crosshairs: the looking hand's on the nearer of the block (lying on its face)
        // and the mob the player picks; the other hand's on the block along it.
        let aim = self.eye_position().and_then(|eye| {
            let hit = self.pick();
            let face = hit.map(|hit| glam::Vec3::new(hit.face[0] as f32, hit.face[1] as f32, hit.face[2] as f32));
            let block = hit.map(|hit| hit.point);
            // mob_hit's point is from the mob's feet.
            let mob = self.mob_hit().map(|(i, _, at)| {
                let feet = &self.mobs[i].position;
                [feet.x as f64 + at[0], feet.y as f64 + at[1], feet.z as f64 + at[2]]
            });
            let distance = |p: &[f64; 3]| (0..3).map(|i| (p[i] - eye[i]).powi(2)).sum::<f64>();
            let (at, normal) = match (block, mob) {
                (Some(b), Some(m)) if distance(&m) < distance(&b) => (m, None),
                (Some(b), _) => (b, face),
                (None, m) => (m?, None),
            };
            Some((
                glam::Vec3::new((at[0] - eye[0]) as f32, (at[1] - eye[1]) as f32, (at[2] - eye[2]) as f32),
                normal,
            ))
        });
        let other = self.eye_position().zip(horizon_xr::hand_angles(!look_left)).and_then(|(eye, (yaw, pitch))| {
            let dir = super::pick_view_vector(yaw, pitch);
            let hit = self.clip_ray(eye, dir, self.block_reach)?;
            let at = hit.point;
            Some((
                glam::Vec3::new((at[0] - eye[0]) as f32, (at[1] - eye[1]) as f32, (at[2] - eye[2]) as f32),
                Some(glam::Vec3::new(hit.face[0] as f32, hit.face[1] as f32, hit.face[2] as f32)),
            ))
        });
        let aim = aim.filter(|_| playing);
        let other = other.filter(|_| playing);
        horizon_xr::set_aim_points(if look_left { [aim, other] } else { [other, aim] });
        let Some((yaw, pitch)) = horizon_xr::look(self.yaw) else {
            return;
        };
        let turn = [f64::from(yaw - self.yaw), f64::from(pitch - self.pitch)].map(|d| d / 0.15);
        if turn != [0.0; 2] {
            self.turn_player(turn[0], turn[1]);
        }
    }
    /// VR: the right controller points at the menu's panel like a mouse, its trigger
    /// the left button.
    fn xr_pointer(&mut self) {
        static CLICK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        static AT: Mutex<[f64; 2]> = Mutex::new([-1.0; 2]);
        let pointer = horizon_xr::pointer();
        let click = pointer.is_some_and(|(_, held)| held);
        let mut s = shared();
        if let (Some((uv, _)), Some(r)) = (pointer, &self.renderer) {
            let at = [f64::from(uv[0]) * f64::from(r.config.width), f64::from(uv[1]) * f64::from(r.config.height)];
            let mut last = AT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if (at[0] - last[0]).abs() + (at[1] - last[1]).abs() > 0.5 {
                *last = at;
                s.mouse.push(MouseInput::Move(at));
            }
        }
        // The right stick, up or down, turns the wheel where it points: about eight notches
        // a second pushed all the way.
        static WHEEL: Mutex<(f32, Option<std::time::Instant>)> = Mutex::new((0.0, None));
        let mut wheel = WHEEL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = std::time::Instant::now();
        let dt = wheel.1.map_or(0.0, |t| now.duration_since(t).as_secs_f32().min(0.1));
        wheel.1 = Some(now);
        let stick = horizon_xr::pad().filter(|_| pointer.is_some()).map_or(0.0, |p| p.right[1]);
        if stick.abs() > 0.3 {
            wheel.0 += stick * 8.0 * dt;
        } else {
            wheel.0 = 0.0;
        }
        while wheel.0.abs() >= 1.0 {
            let notch = wheel.0.signum();
            wheel.0 -= notch;
            s.mouse.push(MouseInput::Wheel([0.0, notch]));
        }
        drop(wheel);
        if CLICK.swap(click, std::sync::atomic::Ordering::Relaxed) != click {
            // The right grip held makes the click a shift-click (quick move), as the
            // keyboard's Shift does.
            if click && horizon_xr::right_grip_held() {
                self.modifiers = winit::keyboard::ModifiersState::SHIFT;
            } else if !click {
                self.modifiers = winit::keyboard::ModifiersState::empty();
            }
            s.mouse.push(MouseInput::Button(0, click));
        }
    }
    /// VR: the buttons that are no controller button. X throws one item, and Y while X is
    /// held the rest of the stack: the held item in play (Q, Control+Q), in a menu the
    /// carried stack (clicks outside the window) or else the slot pointed at. In a menu the
    /// right grip puts one item of the carried stack where the pointer is (a right click).
    /// In play the left grip alone changes the camera (F5), both grips together show or
    /// hide the debug screen (F3).
    fn xr_buttons(&mut self) {
        use std::sync::atomic::{AtomicU8, Ordering};
        use winit::keyboard::{KeyCode, ModifiersState};
        // Last frame's buttons: 1 X, 2 Y, 4 left grip, 8 right grip; 16 both grips were
        // held during this left grip's press.
        static HELD: AtomicU8 = AtomicU8::new(0);
        let Some(pad) = horizon_xr::pad() else {
            return;
        };
        let now = u8::from(pad.x) | u8::from(pad.y) << 1 | u8::from(pad.left_grip) << 2 | u8::from(pad.right_grip) << 3;
        let before = HELD.load(Ordering::Relaxed);
        let pressed = |bit: u8| now & bit != 0 && before & bit == 0;
        let released = |bit: u8| now & bit == 0 && before & bit != 0;
        let mut chord = before & 16;
        if now & 12 == 12 {
            chord = 16;
        }
        let in_menu = self.title.is_none() && self.screen != super::Screen::Playing;
        let playing = self.title.is_none() && self.screen == super::Screen::Playing;
        let throw = if pressed(1) {
            Some(false)
        } else if pad.x && pressed(2) {
            Some(true)
        } else {
            None
        };
        if let Some(all) = throw {
            if in_menu && self.carried.is_some() {
                // A click outside the window throws the carried stack: the right button one
                // item, the left all of it.
                let back = horizon_xr::pointer().zip(self.renderer.as_ref()).map(|((uv, _), r)| {
                    [f64::from(uv[0]) * f64::from(r.config.width), f64::from(uv[1]) * f64::from(r.config.height)]
                });
                let mut s = shared();
                s.mouse.push(MouseInput::Move([1.0, 1.0]));
                let button = if all { 0 } else { 1 };
                s.mouse.push(MouseInput::Button(button, true));
                s.mouse.push(MouseInput::Button(button, false));
                if let Some(back) = back {
                    s.mouse.push(MouseInput::Move(back));
                }
            } else if in_menu || playing {
                let modifiers = self.modifiers;
                self.modifiers = if all { ModifiersState::CONTROL } else { ModifiersState::empty() };
                let _ = self.key_pressed(KeyCode::KeyQ, KeyCode::KeyQ);
                self.modifiers = modifiers;
            }
        }
        if in_menu && pressed(8) && self.carried.is_some() {
            let mut s = shared();
            s.mouse.push(MouseInput::Button(1, true));
            s.mouse.push(MouseInput::Button(1, false));
        }
        if playing && now & 12 == 12 && before & 12 != 12 {
            use horizon_client::debug_keys::KeyAction;
            self.physical_keys.insert(KeyCode::F3);
            self.debug_key(KeyCode::F3, KeyAction::Press);
            self.physical_keys.remove(&KeyCode::F3);
            self.debug_key(KeyCode::F3, KeyAction::Release);
        }
        if released(4) {
            if playing && chord == 0 {
                self.cycle_camera();
            }
            chord = 0;
        }
        if now & 4 == 0 && now & 8 == 0 {
            chord = 0;
        }
        HELD.store(now | chord, Ordering::Relaxed);
    }
    pub(super) fn android_input_frame(&mut self, event_loop: &dyn super::event_loop::EventLoopCtl) {
        if horizon_xr::active() {
            self.xr_pointer();
            self.xr_buttons();
        }
        use winit::{
            application::ApplicationHandler,
            dpi::PhysicalPosition,
            event::{DeviceId, ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
        };
        let escapes = std::mem::take(&mut shared().escapes);
        for _ in 0..escapes {
            self.touch_back();
        }
        let (keyboard, mouse) = take_used();
        if keyboard || mouse {
            self.touch.hardware = true;
            self.pad.in_use = false;
        }
        if mouse {
            self.touch.mouse = true;
        }
        let Some(window_id) = self.window.as_ref().map(|w| w.id()) else {
            take_mouse();
            return;
        };
        let device_id = DeviceId::dummy();
        for input in take_mouse() {
            let in_menu = self.pad_in_menu();
            let event = match input {
                MouseInput::Move(p) if in_menu => WindowEvent::CursorMoved {
                    device_id,
                    position: PhysicalPosition::new(p[0], p[1]),
                },
                MouseInput::Move(_) => continue,
                MouseInput::Delta(d) => {
                    if !in_menu {
                        self.mouse.on_move(0.0, 0.0, d[0], d[1], true, true);
                        self.last_input = std::time::Instant::now();
                    }
                    continue;
                }
                MouseInput::Button(i, down) => WindowEvent::MouseInput {
                    device_id,
                    state: if down { ElementState::Pressed } else { ElementState::Released },
                    button: match i {
                        0 => MouseButton::Left,
                        1 => MouseButton::Right,
                        2 => MouseButton::Middle,
                        3 => MouseButton::Back,
                        _ => MouseButton::Forward,
                    },
                },
                MouseInput::Wheel(w) => WindowEvent::MouseWheel {
                    device_id,
                    delta: MouseScrollDelta::LineDelta(w[0], w[1]),
                    phase: TouchPhase::Moved,
                },
            };
            self.handle_window_event(event_loop, window_id, event, super::input_trace::InputOrigin::User);
        }
        // The pointer is captured for turning the view, and let go in menus.
        let capture = self.touch.mouse && self.focused && self.player.is_some() && !self.pad_in_menu();
        if self.touch.pointer_captured != Some(capture) {
            self.touch.pointer_captured = Some(capture);
            super::android_ime::pointer_capture(capture);
        }
    }
}
