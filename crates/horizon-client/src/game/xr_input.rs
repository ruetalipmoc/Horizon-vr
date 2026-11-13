//! OpenXR play: head look, controller aim, gestures and the Touch pad.
//!
//! Compiled on every target. With no session (`horizon_xr::active` is false) every
//! entry point returns immediately, so the flat desktop client is unchanged.
//! Android still reads keys and motion in `android_input`; this module only
//! covers the headset.

use horizon_client::gamepad::{Button, PadState};
use std::sync::{Mutex, PoisonError};

/// Synthetic pointer events the headset's aim turns into window input.
enum MouseInput {
    Move([f64; 2]),
    Button(u8, bool),
    Wheel([f32; 2]),
}

struct Pointer {
    mouse: Vec<MouseInput>,
}

static POINTER: Mutex<Pointer> = Mutex::new(Pointer { mouse: Vec::new() });

fn shared() -> std::sync::MutexGuard<'static, Pointer> {
    POINTER.lock().unwrap_or_else(PoisonError::into_inner)
}

fn take_mouse() -> Vec<MouseInput> {
    std::mem::take(&mut shared().mouse)
}

/// VR: a bow is held, so the right trigger draws its string (`xr_bow`), never attacks.
static BOW_HELD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// VR: food or a potion is held at the mouth (the use button held by the gesture).
static EATING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// VR: the Touch controllers on top of any other controller. Right stick turns (the head
/// looks up and down), left stick walks, A jumps, B goes back, Y opens the inventory,
/// triggers use and attack, the sticks' clicks sprint and sneak, the menu button pauses.
/// X (dropping) and the grips (the off hand, F3, one item from the carried stack) are
/// `xr_buttons`'.
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
        horizon_xr::Pad {
            left: [-x, z],
            ..touch
        }
    };
    // Over the wrist's hotbar, the right trigger picks a slot (`xr_look`), not the pad; with
    // a bow it draws the string (`xr_bow`).
    let touch = if horizon_xr::wrist_slot().is_some()
        || BOW_HELD.load(std::sync::atomic::Ordering::Relaxed)
    {
        horizon_xr::Pad {
            triggers: [touch.triggers[0], 0.0],
            ..touch
        }
    } else {
        touch
    };
    // In a menu the triggers click where their controllers point (`xr_pointer`), not the
    // pad, and the right stick scrolls there.
    let touch = if horizon_xr::menu_open() {
        horizon_xr::Pad {
            triggers: [0.0; 2],
            right: [0.0; 2],
            ..touch
        }
    } else {
        touch
    };
    let stronger = |a: [f32; 2], b: [f32; 2]| {
        if b[0].hypot(b[1]) > a[0].hypot(a[1]) {
            b
        } else {
            a
        }
    };
    pad.left = stronger(pad.left, touch.left);
    pad.right = stronger(pad.right, touch.right);
    pad.triggers = [
        pad.triggers[0].max(touch.triggers[0]),
        pad.triggers[1].max(touch.triggers[1]),
    ];
    pad
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
        let mut last = LAST
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let speed = last.map_or(0.0, |(t, p): (Instant, [f32; 3])| {
            let dt = now.duration_since(t).as_secs_f64().max(1e-3);
            (0..3)
                .map(|i| f64::from(room[i] - p[i]).powi(2))
                .sum::<f64>()
                .sqrt()
                / dt
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
        let held = *TARGET
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((until, at)) = held
            && now < until
        {
            return Some(at);
        }
        if self.screen != super::Screen::Playing || self.title.is_some() {
            return None;
        }
        let (tip, speed) = hand?;
        let mut cooldown = COOLDOWN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
                let (x, y, z) = (
                    m.position.x as f64,
                    m.position.y as f64,
                    m.position.z as f64,
                );
                let side = (tip[0] - x).hypot(tip[2] - z);
                (side < 0.6 && tip[1] > y - 0.1 && tip[1] < y + 2.0)
                    .then_some(([x, tip[1], z], side))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))?
            .0;
        *cooldown = Some(now + Duration::from_millis(400));
        *TARGET
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
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
        let now = Instant::now();
        let playing = self.screen == super::Screen::Playing && self.title.is_none();
        let place = playing.then_some(()).and(hand).zip(self.eye_position());
        let mut s = shared();
        let mut mining = MINING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        let near_mouth = (0..3)
            .map(|i| (tip[i] - mouth[i]).powi(2))
            .sum::<f64>()
            .sqrt()
            < 0.3;
        let edible = near_mouth
            && self.visible_hand_items()[0].as_ref().is_some_and(|stack| {
                let components = self
                    .pack
                    .item_renderer
                    .models
                    .components(&horizon_client::item_render::stack_view(stack));
                components.get("minecraft:consumable").is_some()
                    || components.get("minecraft:food").is_some()
            });
        if EATING.swap(edible, std::sync::atomic::Ordering::Relaxed) != edible {
            s.mouse.push(MouseInput::Button(1, edible));
        }
        let block = tip.map(|v| v.floor() as i32);
        let struck = !edible
            && speed >= 2.0
            && !self
                .res
                .facts()
                .is_air(self.world.block(block[0], block[1], block[2]));
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
    /// VR: swimming by arm strokes. In water, hands pulled back (against where the head
    /// looks) at more than half a metre a second push the player where the head looks, as
    /// hard as they pull; water's drag slows them again as usual.
    fn xr_swim(&mut self) {
        use std::time::Instant;
        static LAST: Mutex<Option<(Instant, [Option<glam::Vec3>; 2])>> = Mutex::new(None);
        let now = Instant::now();
        let hands = horizon_xr::room_hands().map(|h| {
            [
                h[horizon_xr::Hand::LeftGrip as usize],
                h[horizon_xr::Hand::RightGrip as usize],
            ]
        });
        let mut last = LAST
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = std::mem::replace(&mut *last, hands.map(|h| (now, h)));
        let playing = self.screen == super::Screen::Playing && self.title.is_none();
        let in_water = self
            .player
            .as_ref()
            .is_some_and(|p| p.fluids.was_touching_water);
        let (Some(hands), Some((then, before)), Some((yaw, pitch, forward))) =
            (hands, previous, horizon_xr::head_look())
        else {
            return;
        };
        if !playing || !in_water || self.riding.is_some() {
            return;
        }
        let dt = now.duration_since(then).as_secs_f32();
        if !(0.001..0.1).contains(&dt) {
            return;
        }
        // Metres a second the hands pull back, the stronger one.
        let pull = (0..2)
            .filter_map(|i| Some((hands[i]? - before[i]?) / dt))
            .map(|v| (-v.dot(forward)).max(0.0))
            .fold(0.0f32, f32::max);
        if pull < 0.5 {
            return;
        }
        let dir = super::pick_view_vector(yaw, pitch);
        // Per tick, 0.02 blocks a tick for each metre a second: with water's drag a steady
        // 2 m/s stroke swims about 4 blocks a second.
        let push = 0.02 * f64::from(pull.min(4.0)) * f64::from(dt) * 20.0;
        if let Some(p) = self.player.as_mut() {
            let v = &mut p.velocity;
            v.x += dir[0] * push;
            v.y += dir[1] * push;
            v.z += dir[2] * push;
            let speed = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
            if speed > 0.35 {
                let k = 0.35 / speed;
                v.x *= k;
                v.y *= k;
                v.z *= k;
            }
        }
    }
    /// VR: the bow with both hands. The bow shows in the left hand; the right trigger
    /// squeezed with the right hand near it takes the string and holds use (drawing the
    /// bow), and letting go shoots. The arrow flies from the right hand through the left,
    /// so the player looks that way while drawing and a moment after. Returns where to look.
    fn xr_bow(&mut self) -> Option<[f64; 3]> {
        use std::time::{Duration, Instant};
        // Drawing, the last aim, and until when the player keeps looking along it.
        static BOW: Mutex<(bool, [f64; 3], Option<Instant>)> = Mutex::new((false, [0.0; 3], None));
        static TRIGGER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let now = Instant::now();
        let playing = self.screen == super::Screen::Playing && self.title.is_none();
        let held = playing
            && self.visible_hand_items()[0]
                .as_ref()
                .is_some_and(|s| &*s.item == "minecraft:bow");
        BOW_HELD.store(held, std::sync::atomic::Ordering::Relaxed);
        let trigger = horizon_xr::pad().is_some_and(|p| p.triggers[1] > 0.5);
        let pressed = trigger && !TRIGGER.swap(trigger, std::sync::atomic::Ordering::Relaxed);
        if !trigger {
            TRIGGER.store(false, std::sync::atomic::Ordering::Relaxed);
        }
        let hands = horizon_xr::last_hands().and_then(|h| {
            Some((
                h[horizon_xr::Hand::LeftGrip as usize]?.offset,
                h[horizon_xr::Hand::RightGrip as usize]?.offset,
            ))
        });
        let eye = self.eye_position()?;
        let mut bow = BOW
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (left, right) = match hands {
            Some(h) if held => h,
            _ => {
                if bow.0 {
                    bow.0 = false;
                    shared().mouse.push(MouseInput::Button(1, false));
                }
                return None;
            }
        };
        let string = left - right;
        if string.length() > 0.02 {
            let d = string.normalize();
            bow.1 = [f64::from(d.x), f64::from(d.y), f64::from(d.z)];
        }
        if !bow.0 && pressed && string.length() < 0.2 {
            bow.0 = true;
            shared().mouse.push(MouseInput::Button(1, true));
        } else if bow.0 && !trigger {
            bow.0 = false;
            bow.2 = Some(now + Duration::from_millis(150));
            shared().mouse.push(MouseInput::Button(1, false));
        }
        let aiming = bow.0 || bow.2.is_some_and(|t| now < t);
        aiming.then(|| [0, 1, 2].map(|i| eye[i] + bow.1[i] * 10.0))
    }
    pub(super) fn xr_look(&mut self) {
        if !horizon_xr::active() {
            return;
        }
        horizon_xr::set_menu(self.title.is_some() || self.screen != super::Screen::Playing);
        self.xr_swim();
        // A swing at a mob first, else the hand's own actions (they track every frame).
        let bow = self.xr_bow();
        let tip = self.xr_right_tip();
        let (swing, hand) = if bow.is_some() || BOW_HELD.load(std::sync::atomic::Ordering::Relaxed)
        {
            (None, None)
        } else {
            (self.xr_swing(tip), self.xr_hand_actions(tip))
        };
        if let Some(at) = bow.or(swing).or(hand)
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
        let hotbar = self
            .renderer
            .as_ref()
            .filter(|_| self.title.is_none())
            .map(|r| {
                let scale =
                    horizon_client::hud::HudBatch::new([r.config.width, r.config.height]).scale;
                let (w, h) = (r.config.width as f32, r.config.height as f32);
                let left = w / 2.0 - 91.0 * scale;
                horizon_xr::Hotbar {
                    // Up to the held item's name above the bars (Gui's toolHighlight).
                    crop: [
                        left - 2.0 * scale,
                        h - 72.0 * scale,
                        186.0 * scale,
                        72.0 * scale,
                    ],
                    slots: [
                        left + 1.0 * scale,
                        h - 22.0 * scale,
                        20.0 * scale,
                        22.0 * scale,
                    ],
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
        // The player looks along the right hand; the left trigger's uses go along the left
        // one (`xr_left_use`).
        let playing = self.screen == super::Screen::Playing && self.title.is_none();
        let look_left = false;
        // The crosshairs: the right hand's on the nearer of the block (lying on its face)
        // and the mob the player picks; the left hand's on the block along it.
        let aim = self.eye_position().and_then(|eye| {
            let hit = self.pick();
            let face = hit.map(|hit| {
                glam::Vec3::new(hit.face[0] as f32, hit.face[1] as f32, hit.face[2] as f32)
            });
            let block = hit.map(|hit| hit.point);
            // mob_hit's point is from the mob's feet.
            let mob = self.mob_hit().map(|(i, _, at)| {
                let feet = &self.mobs[i].position;
                [
                    feet.x as f64 + at[0],
                    feet.y as f64 + at[1],
                    feet.z as f64 + at[2],
                ]
            });
            let distance = |p: &[f64; 3]| (0..3).map(|i| (p[i] - eye[i]).powi(2)).sum::<f64>();
            let (at, normal) = match (block, mob) {
                (Some(b), Some(m)) if distance(&m) < distance(&b) => (m, None),
                (Some(b), _) => (b, face),
                (None, m) => (m?, None),
            };
            Some((
                glam::Vec3::new(
                    (at[0] - eye[0]) as f32,
                    (at[1] - eye[1]) as f32,
                    (at[2] - eye[2]) as f32,
                ),
                normal,
            ))
        });
        let other = self
            .eye_position()
            .zip(horizon_xr::hand_angles(!look_left))
            .and_then(|(eye, (yaw, pitch))| {
                let dir = super::pick_view_vector(yaw, pitch);
                let hit = self.clip_ray(eye, dir, self.block_reach)?;
                let at = hit.point;
                Some((
                    glam::Vec3::new(
                        (at[0] - eye[0]) as f32,
                        (at[1] - eye[1]) as f32,
                        (at[2] - eye[2]) as f32,
                    ),
                    Some(glam::Vec3::new(
                        hit.face[0] as f32,
                        hit.face[1] as f32,
                        hit.face[2] as f32,
                    )),
                ))
            });
        let aim = aim.filter(|_| playing);
        let other = other.filter(|_| playing);
        horizon_xr::set_aim_points(if look_left {
            [aim, other]
        } else {
            [other, aim]
        });
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
            let at = [
                f64::from(uv[0]) * f64::from(r.config.width),
                f64::from(uv[1]) * f64::from(r.config.height),
            ];
            let mut last = AT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if (at[0] - last[0]).abs() + (at[1] - last[1]).abs() > 0.5 {
                *last = at;
                s.mouse.push(MouseInput::Move(at));
            }
        }
        // The right stick, up or down, turns the wheel where it points: about eight notches
        // a second pushed all the way.
        static WHEEL: Mutex<(f32, Option<std::time::Instant>)> = Mutex::new((0.0, None));
        let mut wheel = WHEEL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = std::time::Instant::now();
        let dt = wheel
            .1
            .map_or(0.0, |t| now.duration_since(t).as_secs_f32().min(0.1));
        wheel.1 = Some(now);
        let stick = horizon_xr::pad()
            .filter(|_| pointer.is_some())
            .map_or(0.0, |p| p.right[1]);
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
    /// VR: a use (the left trigger) goes along the left controller, for that moment only:
    /// the player turns to it, picks again, uses, and turns back, so the right hand keeps
    /// aiming and attacking meanwhile. Food at the mouth is eaten wherever the hand points.
    pub(super) fn xr_left_use(&mut self, use_now: impl FnOnce(&mut Self)) {
        let angles = horizon_xr::hand_angles(true)
            .filter(|_| !EATING.load(std::sync::atomic::Ordering::Relaxed));
        let Some((yaw, pitch)) = angles else {
            use_now(self);
            return;
        };
        let (old_yaw, old_pitch) = (self.yaw, self.pitch);
        let (old_hit, old_mob) = (self.tick_hit.take(), self.tick_mob.take());
        self.yaw = old_yaw + ((yaw - old_yaw + 180.0).rem_euclid(360.0) - 180.0);
        self.pitch = pitch.clamp(-90.0, 90.0);
        self.tick_hit = Some(self.pick());
        self.tick_mob = Some(self.mob_hit());
        use_now(self);
        self.yaw = old_yaw;
        self.pitch = old_pitch;
        self.tick_hit = old_hit;
        self.tick_mob = old_mob;
    }
    /// VR: the buttons that are no controller button. X throws one item, and Y while X is
    /// held the rest of the stack: the held item in play (Q, Control+Q), in a menu the
    /// carried stack (clicks outside the window) or else the slot pointed at. In a menu the
    /// right grip puts one item of the carried stack where the pointer is (a right click).
    /// In play the left grip alone swaps the held item with the left hand's (F), both grips
    /// together show or hide the debug screen (F3).
    fn xr_buttons(&mut self) {
        use std::sync::atomic::{AtomicU8, Ordering};
        use winit::keyboard::{KeyCode, ModifiersState};
        // Last frame's buttons: 1 X, 2 Y, 4 left grip, 8 right grip; 16 both grips were
        // held during this left grip's press.
        static HELD: AtomicU8 = AtomicU8::new(0);
        let Some(pad) = horizon_xr::pad() else {
            return;
        };
        let now = u8::from(pad.x)
            | u8::from(pad.y) << 1
            | u8::from(pad.left_grip) << 2
            | u8::from(pad.right_grip) << 3;
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
                let back = horizon_xr::pointer()
                    .zip(self.renderer.as_ref())
                    .map(|((uv, _), r)| {
                        [
                            f64::from(uv[0]) * f64::from(r.config.width),
                            f64::from(uv[1]) * f64::from(r.config.height),
                        ]
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
                self.modifiers = if all {
                    ModifiersState::CONTROL
                } else {
                    ModifiersState::empty()
                };
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
                // key.swapOffhand (F): the held item to the left hand and back.
                let _ = self.key_pressed(KeyCode::KeyF, KeyCode::KeyF);
            }
            chord = 0;
        }
        if now & 4 == 0 && now & 8 == 0 {
            chord = 0;
        }
        HELD.store(now | chord, Ordering::Relaxed);
    }
}

impl super::Game<'_> {
    /// Once per frame, before pads are read: the menu pointer and the buttons
    /// that are not themselves pad buttons (drop, off-hand, debug screen).
    /// The pointer's clicks are delivered as the window's own mouse events.
    pub(super) fn xr_input_frame(&mut self, event_loop: &dyn super::event_loop::EventLoopCtl) {
        if horizon_xr::active() {
            self.xr_pointer();
            self.xr_buttons();
            self.dispatch_pointer(event_loop);
        }
    }

    fn dispatch_pointer(&mut self, event_loop: &dyn super::event_loop::EventLoopCtl) {
        use winit::{
            dpi::PhysicalPosition,
            event::{
                DeviceId, ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent,
            },
        };
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
                MouseInput::Button(i, down) => WindowEvent::MouseInput {
                    device_id,
                    state: if down {
                        ElementState::Pressed
                    } else {
                        ElementState::Released
                    },
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
            self.handle_window_event(
                event_loop,
                window_id,
                event,
                super::input_trace::InputOrigin::User,
            );
        }
    }
}

/// The OpenXR controllers merged onto `pad`. No session: `pad` unchanged.
pub(super) fn overlay(pad: PadState) -> PadState {
    if !horizon_xr::active() {
        return pad;
    }
    let Some(touch) = horizon_xr::pad() else {
        return pad;
    };
    let pad = with_touch_controllers(pad, touch);
    if pad.buttons != 0 || pad.left != [0.0; 2] || pad.right != [0.0; 2] || pad.triggers != [0.0; 2]
    {
        horizon_xr::note_input(horizon_xr::InputKind::PadHeld);
    }
    pad
}
