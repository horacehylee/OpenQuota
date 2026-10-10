use std::thread;
use std::time::Duration;

use tauri::image::Image;
use tiny_skia::{LineCap, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

type Rgba = (u8, u8, u8, u8);

const ICON_SIZE: u32 = 32;
const CENTER: f32 = 16.0;
const RADIUS: f32 = 10.5;
const STROKE_WIDTH: f32 = 2.9;
const TRACK_ALPHA: u8 = 107;
const ARC_STEPS: usize = 96;

/// Draws the Windows tray as a monochrome O whose solid arc shows quota left.
pub(crate) fn render_gauge(remaining_fraction: f64, light_theme: bool) -> Image<'static> {
    Image::new_owned(
        render_rgba(remaining_fraction, light_theme),
        ICON_SIZE,
        ICON_SIZE,
    )
}

fn render_rgba(remaining_fraction: f64, light_theme: bool) -> Vec<u8> {
    let mut pixmap = Pixmap::new(ICON_SIZE, ICON_SIZE).expect("tray icon dimensions are valid");
    let foreground = foreground_color(light_theme);
    let circle = circle_path();
    let stroke = Stroke {
        width: STROKE_WIDTH,
        line_cap: LineCap::Round,
        ..Default::default()
    };

    stroke_path(&mut pixmap, &circle, foreground, TRACK_ALPHA, &stroke);
    if let Some(arc) = arc_path(remaining_fraction) {
        stroke_path(&mut pixmap, &arc, foreground, 255, &stroke);
    }

    pixmap.take_demultiplied()
}

fn stroke_path(pixmap: &mut Pixmap, path: &Path, color: Rgba, alpha: u8, stroke: &Stroke) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(color.0, color.1, color.2, alpha);
    paint.anti_alias = true;
    pixmap.stroke_path(path, &paint, stroke, Transform::identity(), None);
}

fn circle_path() -> Path {
    PathBuilder::from_circle(CENTER, CENTER, RADIUS).expect("tray circle path is valid")
}

fn arc_path(remaining_fraction: f64) -> Option<Path> {
    let fraction = remaining_fraction.clamp(0.0, 1.0) as f32;
    if !remaining_fraction.is_finite() || fraction <= 0.0 {
        return None;
    }
    if fraction >= 1.0 {
        return Some(circle_path());
    }

    let steps = ((ARC_STEPS as f32 * fraction).ceil() as usize).max(1);
    let sweep = std::f32::consts::TAU * fraction;
    let mut builder = PathBuilder::new();
    let first_angle = -std::f32::consts::FRAC_PI_2;
    builder.move_to(
        CENTER + RADIUS * first_angle.cos(),
        CENTER + RADIUS * first_angle.sin(),
    );
    for step in 1..=steps {
        let angle = first_angle + sweep * (step as f32 / steps as f32);
        builder.line_to(CENTER + RADIUS * angle.cos(), CENTER + RADIUS * angle.sin());
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
        thread::sleep(Duration::from_secs(2));
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
    fn tray_is_a_monochrome_o_progress() {
        let empty = render_gauge(0.0, false);
        let half = render_gauge(0.5, false);
        let full = render_gauge(1.0, false);

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

        let empty = empty.rgba();
        let half = half.rgba();
        let full = full.rgba();

        assert_eq!(empty.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        assert!(alpha_weight(half) > alpha_weight(empty));
        assert!(alpha_weight(full) > alpha_weight(half));
        assert_eq!(colored_pixels(half), 0);
        assert_eq!(colored_pixels(full), 0);
    }
}
