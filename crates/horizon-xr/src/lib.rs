//! OpenXR for Horizon.
//!
//! Android (Meta Quest) and Linux (Monado, SteamVR, WiVRn, ALVR, and any other
//! OpenXR runtime) share one session: the renderer draws each eye offscreen and
//! this crate copies those images into the runtime's swapchains. macOS and
//! Windows compile a stub that never starts a session, so the flat window path
//! used by [horizon-rust/horizon](https://github.com/horizon-rust/horizon) is unchanged.

#[cfg(any(target_os = "android", target_os = "linux"))]
#[path = "session.rs"]
mod session;

#[cfg(any(target_os = "android", target_os = "linux"))]
pub use session::*;

#[cfg(not(any(target_os = "android", target_os = "linux")))]
mod unsupported;

#[cfg(not(any(target_os = "android", target_os = "linux")))]
pub use unsupported::*;
