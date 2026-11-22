//! `demo --dump <dir>`: the screens as PNGs, rendered headless (no window), for a person or a
//! script that wants to see them without a phone. Five fixed states, 402x874 points at scale 3
//! (1206x2622 pixels) with the phone's safe area, each after enough frames of app time for every
//! transition to settle.

use std::path::Path;
use std::time::Duration;

use lune_ui::dump;
use lune_ui::frame::FrameRenderer;
use lune_ui::gpu::{self, Gpu};
use lune_ui::layout::Insets;

use crate::frame_params;
use crate::showcase::{Headless, Showcase, Tab};

/// The window's size in logical points: an iPhone 16 Pro's screen.
pub const WINDOW: (f32, f32) = (402.0, 874.0);
/// Device pixels per point.
pub const SCALE: f32 = 3.0;
/// The phone's safe area: the notch above, the home indicator below.
pub const SAFE: Insets = Insets::new(0.0, 62.0, 0.0, 34.0);
/// The frames a state runs before it is drawn, 60 to the second: two seconds.
const SETTLE_FRAMES: u32 = 120;

/// The five states, by file name: the tab, Home's depth and whether the sheet is open.
pub const STATES: [(&str, Tab, usize, bool); 5] = [
    ("home", Tab::Home, 1, false),
    ("detail", Tab::Home, 2, false),
    ("sheet", Tab::Home, 2, true),
    ("cards", Tab::Cards, 1, false),
    ("settings", Tab::Settings, 1, false),
];

/// Writes `<name>.png` for each of [`STATES`] into `dir` (made if it is missing).
pub fn write_all(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut gpu = pollster::block_on(Gpu::new(gpu::instance(None), None))?;
    for (name, tab, depth, sheet) in STATES {
        let mut run = Headless::new(WINDOW, SCALE, SAFE);
        run.demo.showcase = Showcase::with_state(tab, depth, sheet);
        let rendered = run
            .run(SETTLE_FRAMES, Duration::from_micros(16_667))
            .ok_or("no frame")?;
        let (rgba, size) = draw(&mut gpu, &mut run.text, &rendered.scene)?;
        dump::write_png(&dir.join(format!("{name}.png")), size, &rgba)?;
    }
    Ok(())
}

/// Draws `scene` into an offscreen target at the demo's size and returns its RGBA bytes.
fn draw(
    gpu: &mut Gpu,
    text: &mut lune_ui::text::TextCache,
    scene: &lune_ui::scene::Scene,
) -> Result<(Vec<u8>, (u32, u32)), String> {
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let mut renderer = FrameRenderer::new(gpu, format);
    let size = (
        (WINDOW.0 * SCALE).round() as u32,
        (WINDOW.1 * SCALE).round() as u32,
    );
    let target = dump::target(&gpu.device, format, size);
    let view = target.create_view(&Default::default());
    let params = frame_params(size, SCALE);
    // Twice: the first draw fills the glyph atlas, the second is the one to keep.
    for _ in 0..2 {
        renderer.draw(gpu, text, scene, &view, params)?;
        gpu.end_frame(text);
    }
    Ok((dump::read_rgba(&gpu.device, &gpu.queue, &target)?, size))
}
