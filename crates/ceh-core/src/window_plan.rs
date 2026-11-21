//! Where and how large the browser window opens. A new session that always looks the same (one size, one place)
//! is one more thing a site can match a banned session by; people's windows differ. Pure: the caller supplies
//! the random numbers, the screen's free area and applies the result.

/// The free area of the screen in the browser's coordinates (`screen.availLeft/Top/Width/Height`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Screen {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
/// The site's layout needs about this much; a screen smaller than that gets the whole free area.
pub const MIN_WIDTH: u32 = 1024;
pub const MIN_HEIGHT: u32 = 700;
/// Share of sessions that use the whole free area, like a maximized window (the most common state of a window).
const FILL_SHARE: f64 = 0.4;

fn spread(low: f64, high: f64, sample: f64) -> f64 {
    low + (high - low) * sample.clamp(0.0, 1.0)
}
/// Whether the free area and its far edges can be written as `i32` coordinates.
fn representable(screen: Screen) -> bool {
    i64::from(screen.left) + i64::from(screen.width) <= i64::from(i32::MAX)
        && i64::from(screen.top) + i64::from(screen.height) <= i64::from(i32::MAX)
}
/// `samples` are four independent numbers in `[0, 1)`: the kind of window, its width, its height and its place.
/// `None` for a free area whose far edges do not fit `i32` (no window can be placed inside it), never a wrapped number.
pub fn plan_window(screen: Screen, samples: [f64; 4]) -> Option<WindowRect> {
    if !representable(screen) {
        return None;
    }
    let whole = WindowRect { x: screen.left, y: screen.top, width: screen.width, height: screen.height };
    if screen.width <= MIN_WIDTH || screen.height <= MIN_HEIGHT || samples[0].clamp(0.0, 1.0) < FILL_SHARE {
        return Some(whole);
    }
    let width = (spread(0.70, 0.97, samples[1]) * f64::from(screen.width)).round() as u32;
    let height = (spread(0.72, 0.97, samples[2]) * f64::from(screen.height)).round() as u32;
    let width = width.clamp(MIN_WIDTH, screen.width);
    let height = height.clamp(MIN_HEIGHT, screen.height);
    // The place uses one number for both axes: a window dragged by a person drifts along a diagonal, not at random.
    let place = samples[3].clamp(0.0, 1.0);
    let x = screen.left + (f64::from(screen.width - width) * place).round() as i32;
    let y = screen.top + (f64::from(screen.height - height) * spread(0.0, 1.0, 1.0 - place * 0.6)).round() as i32;
    Some(WindowRect { x, y, width, height })
}

#[cfg(test)]
mod tests {
    use super::*;
    const MAC: Screen = Screen { left: 0, top: 38, width: 1800, height: 1130 };

    fn inside(rect: WindowRect, screen: Screen) -> bool {
        rect.x >= screen.left
            && rect.y >= screen.top
            && rect.x + rect.width as i32 <= screen.left + screen.width as i32
            && rect.y + rect.height as i32 <= screen.top + screen.height as i32
    }
    #[test]
    fn every_window_stays_inside_the_free_area_and_big_enough() {
        for a in 0..=10 {
            for b in 0..=10 {
                for c in 0..=10 {
                    for d in 0..=10 {
                        let samples = [a, b, c, d].map(|n| f64::from(n) / 10.0);
                        for screen in [MAC, Screen { left: -1920, top: 0, width: 1920, height: 1040 }, Screen { left: 0, top: 0, width: 1366, height: 728 }] {
                            let rect = plan_window(screen, samples).unwrap();
                            assert!(inside(rect, screen), "{rect:?} outside {screen:?} for {samples:?}");
                            assert!(rect.width >= MIN_WIDTH && rect.height >= MIN_HEIGHT, "{rect:?}");
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn a_small_screen_gets_all_of_it() {
        let small = Screen { left: 0, top: 0, width: 1024, height: 640 };
        assert_eq!(plan_window(small, [0.9, 0.5, 0.5, 0.5]), Some(WindowRect { x: 0, y: 0, width: 1024, height: 640 }));
    }
    #[test]
    fn some_sessions_fill_the_screen_and_the_others_differ_from_each_other() {
        assert_eq!(plan_window(MAC, [0.1, 0.9, 0.9, 0.9]), Some(WindowRect { x: 0, y: 38, width: 1800, height: 1130 }));
        let first = plan_window(MAC, [0.7, 0.2, 0.3, 0.1]).unwrap();
        let second = plan_window(MAC, [0.7, 0.8, 0.9, 0.9]).unwrap();
        assert_ne!(first, second);
        assert!(first.width < MAC.width && second.width < MAC.width, "a sized window is not the whole screen");
    }
    #[test]
    fn the_same_numbers_give_the_same_window() {
        let samples = [0.6, 0.33, 0.77, 0.5];
        assert_eq!(plan_window(MAC, samples), plan_window(MAC, samples));
    }
    #[test]
    fn a_free_area_that_does_not_fit_i32_is_refused_instead_of_wrapping() {
        let far = Screen { left: 2147483547, top: 0, width: 1800, height: 1130 };
        assert_eq!(plan_window(far, [0.9, 0.0, 0.0, 0.5]), None);
        let tall = Screen { left: 0, top: i32::MAX - 100, width: 1800, height: 1130 };
        assert_eq!(plan_window(tall, [0.1, 0.0, 0.0, 0.5]), None);
        let edge = Screen { left: i32::MAX - 1800, top: 0, width: 1800, height: 1130 };
        assert!(plan_window(edge, [0.9, 0.5, 0.5, 1.0]).is_some(), "the last representable screen still works");
    }
}
