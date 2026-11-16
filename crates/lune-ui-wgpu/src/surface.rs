//! A window's swapchain: the surface, its configuration, and acquire / present. Windowing-free:
//! the app creates the `wgpu::Surface` from any window handle (`Instance::create_surface`), asks
//! for a redraw when [`acquire`](WindowSurface::acquire) says so, and tells its window a frame is
//! about to be presented before [`present`](WindowSurface::present). [`SurfaceOptions`] sets the
//! present mode, the frame latency and the colour depth ([`SurfaceDepth`]:
//! [`WindowSurface::with_options`], [`WindowSurface::set_options`]); the defaults present as
//! 0.4.0 did, and `Auto` depth gives the 8-bit format of 0.6 on a glass window whatever the
//! display; an opaque window (`opaque: true`) over a display of 10 bits or more gets `Ten`.
//!
//! # Changing the depth of a live window
//!
//! Set the options between frames, never while a texture from [`WindowSurface::acquire`] is
//! alive; then re-target the window's renderer, and rebuild what the app made for the old format:
//!
//! ```ignore
//! surface.set_options(&gpu, SurfaceOptions { depth: SurfaceDepth::Float16, ..surface.options() });
//! if renderer.follow(&mut gpu, &surface) {
//!     // The format changed: build the `CustomDraw` pipelines again from `surface.format()`.
//! }
//! ```
//!
//! `..surface.options()` keeps the present mode, latency, display depth and `opaque` as they are;
//! `..SurfaceOptions::default()` would reset them.

use crate::gpu::Gpu;

/// What [`WindowSurface::acquire`] got.
#[derive(Debug)]
pub enum Acquired {
    /// The texture to draw this frame into.
    Frame(wgpu::SurfaceTexture),
    /// Skip this frame (hidden or minimised window, or a timeout); the platform sends a redraw
    /// when the window is visible again.
    Skip,
    /// The surface was outdated or lost and has been reconfigured; draw again on a new redraw.
    Redraw,
}

/// The colour depth of a window's surface: how many bits per channel its format holds.
///
/// Every intermediate of a frame (blur targets, the scene target) is made in the surface's
/// format, so the window's depth is the precision of the whole frame. A depth the surface cannot
/// present falls back down `Unorm16`, `Float16`, `Ten`, `Eight` ([`WindowSurface::depth`] reports
/// what was configured).
///
/// | variant | format | note |
/// | --- | --- | --- |
/// | `Eight` | `Bgra8Unorm` (else `Rgba8Unorm`) | the format of 0.6 |
/// | `Ten` | `Rgb10a2Unorm` | alpha has 2 bits, 4 levels |
/// | `Float16` | `Rgba16Float` | half-precision float; twice the memory of `Eight` |
/// | `Unorm16` | `Rgba16Unorm` | where the device and surface offer it; Vulkan only in practice |
///
/// `Ten`'s alpha has 4 levels, so a translucent (glass) window shows its edges, shadows and
/// background in 4 steps of opacity: a fallback and `Auto` never land on `Ten` while the
/// configured alpha mode is not `Opaque`, but an explicit `Ten` is honoured. How the OS composites
/// a deep format over the desktop is not verified on every platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum SurfaceDepth {
    /// 8 bits per channel: `Bgra8Unorm`, else `Rgba8Unorm`.
    Eight,
    /// 10 bits per channel and 2 bits of alpha: `Rgb10a2Unorm`.
    Ten,
    /// 16-bit float per channel: `Rgba16Float`, configured in the sRGB colour space.
    Float16,
    /// 16-bit normalised per channel: `Rgba16Unorm`, where the device offers it.
    Unorm16,
    /// Chosen by the surface from [`SurfaceOptions::display_bits`] and the alpha mode: 8-bit on a
    /// translucent window or an unknown display, `Ten` on an opaque one over a display of 10 bits
    /// or more.
    #[default]
    Auto,
}

/// How a [`WindowSurface`] presents and what depth it has (D299). The default is `Fifo` with a
/// latency of 2 and an `Auto` depth over an unknown display, which is 8-bit.
///
/// `Fifo` waits for the display, and [`WindowSurface::acquire`] blocks while every drawable is
/// queued, so a run of frames asked for at once waits there for up to a refresh interval each.
/// An app that draws only when a frame is due can pick a mode that does not wait.
/// `AutoNoVsync` becomes the first of `Immediate`, `Mailbox` and `Fifo` that the surface lists
/// (wgpu's order), and a mode that does not wait for the display can tear.
///
/// To change one field of a live surface, start from [`WindowSurface::options`]
/// (`SurfaceOptions { present_mode, ..surface.options() }`): `..SurfaceOptions::default()` would
/// also reset the depth request and the display's depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceOptions {
    /// The present mode. A mode the surface does not list falls back to `Fifo`; `AutoVsync` and
    /// `AutoNoVsync` are resolved by wgpu.
    pub present_mode: wgpu::PresentMode,
    /// wgpu's `desired_maximum_frame_latency`: how many frames may be queued ahead of the
    /// display. wgpu clamps it to the backend's range (Metal: 1 to 2).
    pub frame_latency: u32,
    /// The colour depth asked for; see [`SurfaceDepth`].
    pub depth: SurfaceDepth,
    /// The bits per channel of the display that holds the window, for `Auto`; `None` is unknown
    /// (`Auto` then gives 8-bit). A value of 0 or above 16 counts as unknown. The platform
    /// adapter reads it (`lune_ui_platform::desktop::display_depth`).
    pub display_bits: Option<u8>,
    /// The window is opaque: prefer the `Opaque` alpha mode, so `Auto` may pick a format with a
    /// 2-bit alpha. The default `false` keeps the alpha mode of 0.6 (premultiplied, else
    /// post-multiplied), the glass window's.
    pub opaque: bool,
}

impl Default for SurfaceOptions {
    fn default() -> Self {
        Self {
            present_mode: wgpu::PresentMode::Fifo,
            frame_latency: 2,
            depth: SurfaceDepth::Auto,
            display_bits: None,
            opaque: false,
        }
    }
}

/// The mode to configure for `want`: `AutoVsync` and `AutoNoVsync` pass (wgpu resolves them), a
/// mode in `supported` passes, any other becomes `Fifo`, which every surface offers.
fn resolve_present_mode(
    want: wgpu::PresentMode,
    supported: &[wgpu::PresentMode],
) -> wgpu::PresentMode {
    use wgpu::PresentMode::{AutoNoVsync, AutoVsync, Fifo};
    if matches!(want, AutoVsync | AutoNoVsync) || supported.contains(&want) {
        want
    } else {
        Fifo
    }
}

/// One format the surface lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Offered {
    format: wgpu::TextureFormat,
    /// The surface can present it in the sRGB colour space.
    srgb: bool,
    /// It can be configured with the colour space `Auto` (wgpu's `SurfaceCapabilities::formats`).
    auto: bool,
}

/// What [`resolve_format`] chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Chosen {
    format: wgpu::TextureFormat,
    depth: SurfaceDepth,
    color_space: wgpu::SurfaceColorSpace,
}

/// The depth a format gives.
fn depth_of(format: wgpu::TextureFormat) -> SurfaceDepth {
    match format {
        wgpu::TextureFormat::Rgb10a2Unorm => SurfaceDepth::Ten,
        wgpu::TextureFormat::Rgba16Float => SurfaceDepth::Float16,
        wgpu::TextureFormat::Rgba16Unorm => SurfaceDepth::Unorm16,
        _ => SurfaceDepth::Eight,
    }
}

/// The alpha mode to configure: `opaque` prefers `Opaque`, then premultiplied (our output is;
/// DirectComposition offers it), then post-multiplied (Metal's only translucent mode, which
/// CoreAnimation composites as premultiplied); else the first the surface lists.
fn resolve_alpha(
    opaque: bool,
    supported: &[wgpu::CompositeAlphaMode],
) -> Option<wgpu::CompositeAlphaMode> {
    use wgpu::CompositeAlphaMode::{Opaque, PostMultiplied, PreMultiplied};
    let order: &[_] = if opaque {
        &[Opaque, PreMultiplied, PostMultiplied]
    } else {
        &[PreMultiplied, PostMultiplied]
    };
    order
        .iter()
        .copied()
        .find(|m| supported.contains(m))
        .or_else(|| supported.first().copied())
}

/// The format, depth and colour space for `want`. `alpha_opaque` is whether the configured alpha
/// mode is `Opaque`; `unorm16_ok` whether the device can render to `Rgba16Unorm`. A format is
/// usable when the surface lists it with the sRGB colour space (wgpu's `Auto` would show a float
/// surface's gamma values as linear), so DX12's linear-only float surface never resolves. `None`
/// when the surface lists no format.
fn resolve_format(
    want: SurfaceDepth,
    display_bits: Option<u8>,
    offered: &[Offered],
    alpha_opaque: bool,
    unorm16_ok: bool,
) -> Option<Chosen> {
    use SurfaceDepth::{Auto, Eight, Float16, Ten, Unorm16};
    use wgpu::TextureFormat as F;
    let usable = |f: F| {
        offered.iter().any(|o| o.format == f && o.srgb) && (f != F::Rgba16Unorm || unorm16_ok)
    };
    let deep = matches!(display_bits, Some(10..=16));
    let chain: &[SurfaceDepth] = match want {
        Unorm16 => &[Unorm16, Float16, Ten, Eight],
        Float16 => &[Float16, Ten, Eight],
        Ten => &[Ten, Eight],
        Eight => &[Eight],
        Auto if deep && alpha_opaque => &[Ten, Eight],
        // A translucent window's `Auto` stays 8-bit over a deep display, whatever the OS: `Ten`
        // has 4 alpha levels and a float surface costs twice the memory. The user chose this
        // after seeing 8, 10 and 16-bit glass live (DECISIONS D330); an app that wants more asks
        // for `Ten` or `Float16`, or passes `opaque: true`.
        Auto if deep => &[Eight],
        Auto => &[Eight],
    };
    for (i, depth) in chain.iter().enumerate() {
        let format = match depth {
            // A fallback never lands on a 2-bit alpha while the window is translucent; an explicit
            // or `Auto`-chosen `Ten` at the head of its chain is honoured.
            Ten if i > 0 && !alpha_opaque => continue,
            Ten => F::Rgb10a2Unorm,
            Float16 => F::Rgba16Float,
            Unorm16 => F::Rgba16Unorm,
            _ => {
                let eight = [F::Bgra8Unorm, F::Rgba8Unorm]
                    .into_iter()
                    .find(|&f| usable(f));
                if let Some(f) = eight.or_else(|| offered.iter().find(|o| o.srgb).map(|o| o.format))
                {
                    return Some(Chosen {
                        format: f,
                        depth: depth_of(f),
                        color_space: wgpu::SurfaceColorSpace::Srgb,
                    });
                }
                // No format lists sRGB: today's last resort, the first one `Auto` can configure.
                let last = offered
                    .iter()
                    .find(|o| o.auto)
                    .or_else(|| offered.first())?;
                return Some(Chosen {
                    format: last.format,
                    depth: depth_of(last.format),
                    color_space: wgpu::SurfaceColorSpace::Auto,
                });
            }
        };
        if usable(format) {
            return Some(Chosen {
                format,
                depth: *depth,
                color_space: wgpu::SurfaceColorSpace::Srgb,
            });
        }
    }
    None
}

/// Whether a `set_options` must choose the format and alpha mode again: only a change of what
/// they depend on does, so a call that changes the present mode or the latency alone keeps both.
fn needs_reresolve(stored: &SurfaceOptions, new: &SurfaceOptions) -> bool {
    stored.depth != new.depth
        || stored.display_bits != new.display_bits
        || stored.opaque != new.opaque
}

/// One window's surface, configured for a [`Gpu`].
pub struct WindowSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// The present modes the surface lists, read once at creation.
    present_modes: Vec<wgpu::PresentMode>,
    /// The alpha modes the surface lists, read once at creation.
    alpha_modes: Vec<wgpu::CompositeAlphaMode>,
    /// The formats the surface lists, read once at creation.
    offered: Vec<Offered>,
    /// The device can render to `Rgba16Unorm`.
    unorm16_ok: bool,
    /// The options as asked (the depth request, the display's depth, `opaque`).
    asked: SurfaceOptions,
    /// The depth of the configured format, never `Auto`.
    depth: SurfaceDepth,
}

impl std::fmt::Debug for WindowSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowSurface")
            .field("config", &self.config)
            .field("depth", &self.depth)
            .finish_non_exhaustive()
    }
}

/// Whether the device can render to `Rgba16Unorm`: it was opened with the 16-bit norm and
/// adapter-specific format features, and the adapter allows the format as a blendable, filterable
/// render attachment.
fn unorm16_usable(gpu: &Gpu) -> bool {
    let need = wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
        | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
    if !gpu.device.features().contains(need) {
        return false;
    }
    let f = gpu
        .adapter
        .get_texture_format_features(wgpu::TextureFormat::Rgba16Unorm);
    f.allowed_usages
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        && f.flags.contains(
            wgpu::TextureFormatFeatureFlags::BLENDABLE
                | wgpu::TextureFormatFeatureFlags::FILTERABLE,
        )
}

impl WindowSurface {
    /// Configures `surface` at `size` physical pixels with [`SurfaceOptions::default`]: the
    /// `Bgra8Unorm` format (else `Rgba8Unorm`) when the display's depth is unknown, in the sRGB
    /// colour space; the alpha mode prefers premultiplied, then post-multiplied.
    pub fn new(
        gpu: &Gpu,
        surface: wgpu::Surface<'static>,
        size: (u32, u32),
    ) -> Result<Self, String> {
        Self::with_options(gpu, surface, size, SurfaceOptions::default())
    }

    /// [`Self::new`] with the present mode, frame latency, depth and display of `options`; a mode
    /// the surface does not list becomes `Fifo`, and a depth it cannot present falls back (see
    /// [`SurfaceDepth`]). [`Self::options`] reports the options asked, [`Self::depth`] and
    /// [`Self::format`] what was configured.
    pub fn with_options(
        gpu: &Gpu,
        surface: wgpu::Surface<'static>,
        size: (u32, u32),
        options: SurfaceOptions,
    ) -> Result<Self, String> {
        let caps = surface.get_capabilities(&gpu.adapter);
        if caps.format_capabilities.is_empty() || caps.alpha_modes.is_empty() {
            return Err("surface: not supported by the adapter".into());
        }
        let offered: Vec<Offered> = caps
            .format_capabilities
            .iter()
            .map(|c| Offered {
                format: c.format,
                srgb: c.color_spaces.contains(wgpu::SurfaceColorSpaces::SRGB),
                auto: caps.formats.contains(&c.format),
            })
            .collect();
        let unorm16_ok = unorm16_usable(gpu);
        let alpha_mode = resolve_alpha(options.opaque, &caps.alpha_modes)
            .ok_or("surface: not supported by the adapter")?;
        let chosen = resolve_format(
            options.depth,
            options.display_bits,
            &offered,
            alpha_mode == wgpu::CompositeAlphaMode::Opaque,
            unorm16_ok,
        )
        .ok_or("surface: not supported by the adapter")?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: chosen.format,
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode: resolve_present_mode(options.present_mode, &caps.present_modes),
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: options.frame_latency,
            color_space: chosen.color_space,
        };
        surface.configure(&gpu.device, &config);
        Ok(Self {
            surface,
            config,
            present_modes: caps.present_modes,
            alpha_modes: caps.alpha_modes,
            offered,
            unorm16_ok,
            asked: options,
            depth: chosen.depth,
        })
    }

    /// The options in force: the present mode actually in use (after the fallback to `Fifo`), the
    /// frame latency, and the depth request, display depth and `opaque` as last asked. Passing it
    /// back to [`Self::set_options`] changes nothing; for the resolved depth read
    /// [`Self::depth`].
    pub fn options(&self) -> SurfaceOptions {
        SurfaceOptions {
            present_mode: self.config.present_mode,
            frame_latency: self.config.desired_maximum_frame_latency,
            ..self.asked
        }
    }

    /// Reconfigures the surface with `options` at the current size. The present mode and the
    /// frame latency always apply. The format and the alpha mode are chosen again only when
    /// `depth`, `display_bits` or `opaque` differ from the options in force ([`Self::options`]),
    /// so a call that changes the present mode alone keeps the format; a call built with
    /// `..SurfaceOptions::default()` resets the depth request and the display's depth, so start
    /// from `..surface.options()`. When [`Self::format`] changed, make the window's
    /// [`FrameRenderer`](crate::frame::FrameRenderer) again (`FrameRenderer::follow`) and any
    /// `CustomDraw` pipelines built for the old format. Call it between frames, never while a
    /// texture from [`Self::acquire`] is alive (present or drop it first): wgpu refuses to
    /// configure a surface with an outstanding texture and its default error handler panics.
    pub fn set_options(&mut self, gpu: &Gpu, options: SurfaceOptions) {
        if needs_reresolve(&self.asked, &options) {
            if let Some(alpha_mode) = resolve_alpha(options.opaque, &self.alpha_modes) {
                self.config.alpha_mode = alpha_mode;
                if let Some(chosen) = resolve_format(
                    options.depth,
                    options.display_bits,
                    &self.offered,
                    alpha_mode == wgpu::CompositeAlphaMode::Opaque,
                    self.unorm16_ok,
                ) {
                    self.config.format = chosen.format;
                    self.config.color_space = chosen.color_space;
                    self.depth = chosen.depth;
                }
            }
        }
        self.asked = options;
        self.config.present_mode = resolve_present_mode(options.present_mode, &self.present_modes);
        self.config.desired_maximum_frame_latency = options.frame_latency;
        self.surface.configure(&gpu.device, &self.config);
    }

    /// The depth of the configured format, never [`SurfaceDepth::Auto`]: what `Auto` or a
    /// fallback resolved to.
    pub fn depth(&self) -> SurfaceDepth {
        self.depth
    }

    /// The surface's colour format: make the window's
    /// [`FrameRenderer`](crate::frame::FrameRenderer) with it, and again when it changes.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// The configured size in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// The chosen composite alpha mode.
    pub fn alpha_mode(&self) -> wgpu::CompositeAlphaMode {
        self.config.alpha_mode
    }

    /// Reconfigures the surface to `width` by `height` physical px, each at least 1. Call it when
    /// the window resizes, between frames and never while a texture from [`Self::acquire`] is
    /// alive (present or drop it first: wgpu refuses to configure a surface with an outstanding
    /// texture and its default error handler panics); [`Self::acquire`] does it itself for an
    /// outdated surface.
    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&gpu.device, &self.config);
    }

    /// The next surface texture. An outdated, suboptimal or lost surface is reconfigured (a
    /// suboptimal texture is dropped, not drawn) and [`Acquired::Redraw`] returned; a hidden or
    /// minimised window, or a timeout, gives [`Acquired::Skip`].
    ///
    /// Under `Fifo` a frame started before the display can take one waits here for up to a
    /// refresh interval (see [`SurfaceOptions`]).
    pub fn acquire(&mut self, gpu: &Gpu) -> Result<Acquired, String> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => Ok(Acquired::Frame(t)),
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                // The texture must not be alive when the surface is configured (D11).
                drop(texture);
                let (w, h) = self.size();
                self.resize(gpu, w, h);
                Ok(Acquired::Redraw)
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                let (w, h) = self.size();
                self.resize(gpu, w, h);
                Ok(Acquired::Redraw)
            }
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => {
                Ok(Acquired::Skip)
            }
            other => Err(format!("surface: {other:?}")),
        }
    }

    /// Shows a finished frame. Tell the window a frame is coming first (the windowing library's
    /// pre-present notification).
    pub fn present(&self, gpu: &Gpu, frame: wgpu::SurfaceTexture) {
        gpu.queue.present(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::PresentMode::{AutoNoVsync, AutoVsync, Fifo, Immediate, Mailbox};

    use SurfaceDepth::{Auto, Eight, Float16, Ten, Unorm16};
    use wgpu::CompositeAlphaMode as A;
    use wgpu::SurfaceColorSpace::{Auto as CsAuto, Srgb};
    use wgpu::TextureFormat as F;

    fn o(format: F, srgb: bool) -> Offered {
        Offered {
            format,
            srgb,
            auto: true,
        }
    }

    /// Metal: all sRGB.
    fn metal() -> Vec<Offered> {
        [
            F::Bgra8UnormSrgb,
            F::Bgra8Unorm,
            F::Rgba16Float,
            F::Rgb10a2Unorm,
        ]
        .map(|f| o(f, true))
        .to_vec()
    }

    /// DX12 through a Visual: the float surface lists no sRGB.
    fn windows() -> Vec<Offered> {
        [
            F::Bgra8UnormSrgb,
            F::Bgra8Unorm,
            F::Rgba8UnormSrgb,
            F::Rgba8Unorm,
            F::Rgb10a2Unorm,
            F::Rgba16Float,
        ]
        .map(|f| o(f, f != F::Rgba16Float))
        .to_vec()
    }

    /// A Vulkan list: Windows' plus `Rgba16Unorm` with sRGB.
    fn vulkan() -> Vec<Offered> {
        let mut v = windows();
        v.push(o(F::Rgba16Unorm, true));
        v
    }

    fn pick(
        want: SurfaceDepth,
        bits: Option<u8>,
        offered: &[Offered],
        alpha_opaque: bool,
        unorm16_ok: bool,
    ) -> (F, SurfaceDepth) {
        let c = resolve_format(want, bits, offered, alpha_opaque, unorm16_ok).expect("a result");
        (c.format, c.depth)
    }

    const B8: (F, SurfaceDepth) = (F::Bgra8Unorm, Eight);
    const T10: (F, SurfaceDepth) = (F::Rgb10a2Unorm, Ten);
    const H16: (F, SurfaceDepth) = (F::Rgba16Float, Float16);

    #[test]
    fn the_default_is_fifo_with_a_latency_of_2_and_an_auto_depth() {
        let d = SurfaceOptions::default();
        assert_eq!(
            (
                d.present_mode,
                d.frame_latency,
                d.depth,
                d.display_bits,
                d.opaque
            ),
            (Fifo, 2, Auto, None, false)
        );
    }

    #[test]
    fn metal_translucent() {
        let m = metal();
        assert_eq!(pick(Auto, None, &m, false, false), B8);
        assert_eq!(pick(Auto, Some(8), &m, false, false), B8);
        // The glass window's `Auto` over a deep display stays 8-bit: the user chose it after
        // seeing 8, 10 and 16-bit glass live (`Ten`'s 2-bit alpha, float's doubled memory).
        assert_eq!(pick(Auto, Some(10), &m, false, false), B8);
        assert_eq!(pick(Ten, Some(10), &m, false, false), T10);
        assert_eq!(pick(Float16, None, &m, false, false), H16);
        assert_eq!(pick(Unorm16, None, &m, false, false), H16);
        assert_eq!(pick(Eight, Some(10), &m, false, false), B8);
    }

    #[test]
    fn metal_opaque() {
        let m = metal();
        assert_eq!(pick(Auto, Some(10), &m, true, false), T10);
        assert_eq!(pick(Auto, Some(12), &m, true, false), T10);
        assert_eq!(pick(Unorm16, None, &m, true, false), H16);
    }

    #[test]
    fn windows_translucent_never_lands_on_ten_by_fallback() {
        let w = windows();
        assert_eq!(pick(Auto, Some(10), &w, false, false), B8);
        assert_eq!(pick(Float16, None, &w, false, false), B8);
        assert_eq!(pick(Unorm16, None, &w, false, false), B8);
        assert_eq!(pick(Ten, None, &w, false, false), T10);
    }

    #[test]
    fn windows_opaque() {
        let w = windows();
        assert_eq!(pick(Float16, None, &w, true, false), T10);
        assert_eq!(pick(Auto, Some(10), &w, true, false), T10);
    }

    #[test]
    fn unorm16_needs_the_device() {
        let v = vulkan();
        assert_eq!(
            pick(Unorm16, None, &v, false, true),
            (F::Rgba16Unorm, Unorm16)
        );
        // Not usable: Float16 lacks sRGB here, Ten is skipped while translucent.
        assert_eq!(pick(Unorm16, None, &v, false, false), B8);
        let mut m = metal();
        m.push(o(F::Rgba16Unorm, true));
        assert_eq!(pick(Unorm16, None, &m, false, false), H16);
    }

    #[test]
    fn an_odd_display_depth_is_unknown() {
        let m = metal();
        for bits in [Some(0), Some(17), Some(255)] {
            assert_eq!(pick(Auto, bits, &m, true, false), B8);
        }
        assert_eq!(pick(Auto, Some(16), &m, true, false), T10);
    }

    #[test]
    fn a_surface_without_srgb_gets_its_first_format_with_the_auto_colour_space() {
        let offered = [o(F::Rgba8UnormSrgb, false), o(F::Rgb10a2Unorm, false)];
        let c = resolve_format(Auto, None, &offered, false, false).expect("a result");
        assert_eq!(
            (c.format, c.depth, c.color_space),
            (F::Rgba8UnormSrgb, Eight, CsAuto)
        );
        let c = resolve_format(Ten, None, &offered, true, false).expect("a result");
        assert_eq!(
            (c.format, c.depth, c.color_space),
            (F::Rgba8UnormSrgb, Eight, CsAuto)
        );
        assert!(resolve_format(Auto, None, &[], false, false).is_none());
    }

    #[test]
    fn every_chosen_format_that_lists_srgb_is_configured_in_srgb() {
        for offered in [metal(), windows(), vulkan()] {
            for want in [Auto, Eight, Ten, Float16, Unorm16] {
                let c = resolve_format(want, Some(10), &offered, true, true).expect("a result");
                assert_eq!(c.color_space, Srgb, "{want:?}");
            }
        }
    }

    #[test]
    fn only_rgba8_unorm_offered_gives_it() {
        let offered = [o(F::Rgba8Unorm, true)];
        assert_eq!(
            pick(Auto, None, &offered, false, false),
            (F::Rgba8Unorm, Eight)
        );
    }

    #[test]
    fn the_alpha_mode_follows_opaque() {
        let metal_modes = [A::Opaque, A::PostMultiplied];
        let visual = [
            A::Auto,
            A::Inherit,
            A::Opaque,
            A::PostMultiplied,
            A::PreMultiplied,
        ];
        assert_eq!(resolve_alpha(false, &metal_modes), Some(A::PostMultiplied));
        assert_eq!(resolve_alpha(true, &metal_modes), Some(A::Opaque));
        assert_eq!(resolve_alpha(false, &visual), Some(A::PreMultiplied));
        assert_eq!(resolve_alpha(true, &visual), Some(A::Opaque));
        // An HWND surface lists Opaque only; so does an `opaque: false` request.
        assert_eq!(resolve_alpha(false, &[A::Opaque]), Some(A::Opaque));
        assert_eq!(
            resolve_alpha(false, &[A::Inherit, A::Auto]),
            Some(A::Inherit)
        );
        assert_eq!(resolve_alpha(false, &[]), None);
    }

    #[test]
    fn a_surface_that_lists_only_opaque_alpha_lets_auto_pick_ten() {
        // The format rule reads the configured alpha mode, not the `opaque` flag.
        let w = windows();
        let alpha = resolve_alpha(false, &[A::Opaque]).expect("a result");
        assert_eq!(pick(Auto, Some(10), &w, alpha == A::Opaque, false), T10);
    }

    #[test]
    fn set_options_reresolves_only_for_depth_display_or_opaque() {
        let base = SurfaceOptions::default();
        let with = |f: fn(&mut SurfaceOptions)| {
            let mut o = base;
            f(&mut o);
            o
        };
        assert!(!needs_reresolve(&base, &base));
        assert!(!needs_reresolve(
            &base,
            &with(|o| o.present_mode = wgpu::PresentMode::AutoNoVsync)
        ));
        assert!(!needs_reresolve(&base, &with(|o| o.frame_latency = 1)));
        assert!(needs_reresolve(&base, &with(|o| o.depth = Ten)));
        assert!(needs_reresolve(&base, &with(|o| o.display_bits = Some(10))));
        assert!(needs_reresolve(&base, &with(|o| o.opaque = true)));
    }

    #[test]
    fn a_mode_the_surface_lists_passes_and_any_other_becomes_fifo() {
        assert_eq!(resolve_present_mode(Mailbox, &[Fifo, Immediate]), Fifo);
        assert_eq!(
            resolve_present_mode(Immediate, &[Fifo, Immediate]),
            Immediate
        );
        assert_eq!(resolve_present_mode(Fifo, &[]), Fifo);
    }

    #[test]
    fn the_auto_modes_pass_through_for_wgpu_to_resolve() {
        assert_eq!(resolve_present_mode(AutoNoVsync, &[Fifo]), AutoNoVsync);
        assert_eq!(resolve_present_mode(AutoVsync, &[]), AutoVsync);
    }
}
