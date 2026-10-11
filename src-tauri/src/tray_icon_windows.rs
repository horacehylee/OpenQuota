use std::thread;
use std::time::Duration;

use tauri::image::Image;
use tiny_skia::{LineCap, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

type Rgba = (u8, u8, u8, u8);

const ICON_SIZE: u32 = 32;
const CENTER: f32 = 16.0;
const RADIUS: f32 = 10.5;
const STROKE_WIDTH: f32 = 2.9;
const INNER_RADIUS: f32 = 5.1;
const INNER_STROKE_WIDTH: f32 = 2.3;
const TRACK_ALPHA: u8 = 107;
const ARC_STEPS: usize = 96;

/// Draws the Windows tray as nested monochrome Os: session outside, weekly inside.
pub(crate) fn render_gauge(
    session_fraction: f64,
    weekly_fraction: f64,
    light_theme: bool,
) -> Image<'static> {
    Image::new_owned(
        render_rgba(session_fraction, weekly_fraction, light_theme),
        ICON_SIZE,
        ICON_SIZE,
    )
}

fn render_rgba(session_fraction: f64, weekly_fraction: f64, light_theme: bool) -> Vec<u8> {
    let mut pixmap = Pixmap::new(ICON_SIZE, ICON_SIZE).expect("tray icon dimensions are valid");
    let foreground = foreground_color(light_theme);

    stroke_ring(
        &mut pixmap,
        RADIUS,
        session_fraction,
        STROKE_WIDTH,
        foreground,
    );
    stroke_ring(
        &mut pixmap,
        INNER_RADIUS,
        weekly_fraction,
        INNER_STROKE_WIDTH,
        foreground,
    );

    pixmap.take_demultiplied()
}

fn stroke_ring(
    pixmap: &mut Pixmap,
    radius: f32,
    remaining_fraction: f64,
    stroke_width: f32,
    foreground: Rgba,
) {
    let stroke = Stroke {
        width: stroke_width,
        line_cap: LineCap::Round,
        ..Default::default()
    };

    stroke_path(
        pixmap,
        &circle_path(radius),
        foreground,
        TRACK_ALPHA,
        &stroke,
    );
    if let Some(arc) = arc_path(radius, remaining_fraction) {
        stroke_path(pixmap, &arc, foreground, 255, &stroke);
    }
}

fn stroke_path(pixmap: &mut Pixmap, path: &Path, color: Rgba, alpha: u8, stroke: &Stroke) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(color.0, color.1, color.2, alpha);
    paint.anti_alias = true;
    pixmap.stroke_path(path, &paint, stroke, Transform::identity(), None);
}

fn circle_path(radius: f32) -> Path {
    PathBuilder::from_circle(CENTER, CENTER, radius).expect("tray circle path is valid")
}

fn arc_path(radius: f32, remaining_fraction: f64) -> Option<Path> {
    let fraction = remaining_fraction.clamp(0.0, 1.0) as f32;
    if !remaining_fraction.is_finite() || fraction <= 0.0 {
        return None;
    }
    if fraction >= 1.0 {
        return Some(circle_path(radius));
    }

    let steps = ((ARC_STEPS as f32 * fraction).ceil() as usize).max(1);
    let sweep = std::f32::consts::TAU * fraction;
    let mut builder = PathBuilder::new();
    let first_angle = -std::f32::consts::FRAC_PI_2;
    builder.move_to(
        CENTER + radius * first_angle.cos(),
        CENTER + radius * first_angle.sin(),
    );
    for step in 1..=steps {
        let angle = first_angle + sweep * (step as f32 / steps as f32);
        builder.line_to(CENTER + radius * angle.cos(), CENTER + radius * angle.sin());
    }
    builder.finish()
}

fn foreground_color(light_theme: bool) -> Rgba {
    if light_theme {
        (0, 0, 0, 255)
    } else {
        (255, 255, 255, 255)
    }
}

pub(crate) fn system_uses_light_theme() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let value_name = wide("SystemUsesLightTheme");
    let mut value = 0_u32;
    let mut size = u32::try_from(size_of_val(&value)).expect("DWORD size is valid");
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value_name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            std::ptr::from_mut(&mut value).cast(),
            &mut size,
        )
    };

    result == 0 && value == 1
}

/// Polls the Windows taskbar theme and calls back once after each change.
pub(crate) fn spawn_theme_monitor<F>(mut on_theme_change: F)
where
    F: FnMut(bool) + Send + 'static,
{
    let mut light_theme = system_uses_light_theme();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(60 * 60));
        let current = system_uses_light_theme();
        if current != light_theme {
            light_theme = current;
            on_theme_change(current);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{foreground_color, render_gauge, ICON_SIZE};

    #[test]
    fn foreground_follows_the_taskbar_theme() {
        assert_eq!(foreground_color(true), (0, 0, 0, 255));
        assert_eq!(foreground_color(false), (255, 255, 255, 255));
    }

    #[test]
    fn tray_shows_separate_outer_and_inner_progress() {
        let empty = render_gauge(0.0, 0.0, false);
        let outer_half = render_gauge(0.5, 0.0, false);
        let inner_half = render_gauge(0.0, 0.5, false);
        let both_half = render_gauge(0.5, 0.5, false);

        let alpha_weight = |rgba: &[u8]| {
            rgba.as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| u64::from(pixel[3]))
                .sum::<u64>()
        };
        let colored_pixels = |rgba: &[u8]| {
            rgba.as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| {
                    let max = pixel[..3].iter().max().unwrap();
                    let min = pixel[..3].iter().min().unwrap();
                    pixel[3] > 0 && max - min > 1
                })
                .count()
        };

        let ring_pixels = |rgba: &[u8], radius: f32| {
            rgba.as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .filter(|(index, pixel)| {
                    let x = (*index as f32 % ICON_SIZE as f32) + 0.5;
                    let y = ((*index as f32 / ICON_SIZE as f32).trunc()) + 0.5;
                    let distance = (x - 16.0).hypot(y - 16.0);
                    (distance - radius).abs() < 1.5 && pixel[3] > 200
                })
                .count()
        };

        let empty = empty.rgba();
        let outer_half = outer_half.rgba();
        let inner_half = inner_half.rgba();
        let both_half = both_half.rgba();

        assert_eq!(empty.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        assert!(alpha_weight(outer_half) > alpha_weight(empty));
        assert!(alpha_weight(inner_half) > alpha_weight(empty));
        assert!(alpha_weight(both_half) > alpha_weight(outer_half).max(alpha_weight(inner_half)));
        assert_eq!(ring_pixels(outer_half, 5.1), 0);
        assert!(ring_pixels(inner_half, 5.1) > 0);
        assert_eq!(ring_pixels(inner_half, 10.5), 0);
        assert!(ring_pixels(outer_half, 10.5) > 0);
        assert_eq!(colored_pixels(both_half), 0);
    }
}
