//! The OS code of the phone functions, one file per platform; the safe API is `crate::mobile`.

#[cfg(target_os = "ios")]
mod ios;
#[cfg(not(target_os = "ios"))]
mod other;

#[cfg(target_os = "ios")]
pub(crate) use ios::attach_to_scene;
#[cfg(not(target_os = "ios"))]
pub(crate) use other::attach_to_scene;
