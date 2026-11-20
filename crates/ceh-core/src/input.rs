//! Shared interaction semantics. No backend-specific humanization or UI handles.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum SelectionKey{Home,Down,Commit}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct InputLimit;
pub fn selection_keys(enabled_index:usize,safe_commit:bool)->Result<Vec<SelectionKey>,InputLimit>{
 if !safe_commit||enabled_index>256{return Err(InputLimit);}
 let mut keys=Vec::with_capacity(enabled_index+2);keys.push(SelectionKey::Home);keys.extend(std::iter::repeat_n(SelectionKey::Down,enabled_index));keys.push(SelectionKey::Commit);Ok(keys)
}
/// Bounds (inclusive, ms) of the random hover before a press and of the button hold; never constants.
pub struct PointerBounds{pub hover_ms:(u64,u64),pub press_ms:(u64,u64)}
pub const POINTER_BOUNDS:PointerBounds=PointerBounds{hover_ms:(15,90),press_ms:(50,140)};

/// How far the cursor may stray from the element centre in each direction while staying inside the page's
/// viewport (WebDriver rejects a pointer move outside it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reach {
    pub left: i32,
    pub right: i32,
    pub up: i32,
    pub down: i32,
}
/// One move of the cursor: a position relative to the target element's centre and the time the move takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerStep {
    pub x: i32,
    pub y: i32,
    pub ms: u64,
}
/// xorshift64*: small, dependency-free, deterministic per seed.
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut rng = Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        for _ in 0..4 {
            rng.unit();
        }
        rng
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    /// Uniform in [low, high].
    pub fn range(&mut self, low: f64, high: f64) -> f64 {
        low + self.unit() * (high - low)
    }
    pub fn ms(&mut self, bounds: (u64, u64)) -> u64 {
        bounds.0 + (self.unit() * (bounds.1 - bounds.0 + 1) as f64) as u64
    }
    /// Standard normal (Box-Muller).
    pub fn normal(&mut self) -> f64 {
        let (u, v) = (self.unit().max(1e-12), self.unit());
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
    }
}
/// Size of the element to click, in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementBox {
    pub width: f64,
    pub height: f64,
}
/// A whole click: the cursor path (positions relative to the element centre), the pause between the last
/// move and the press, the hold of the button, and the chosen point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClickPlan {
    pub moves: Vec<PointerStep>,
    pub hover_ms: u64,
    pub press_ms: u64,
    pub end: (i32, i32),
}
/// A point inside the element: normal around the centre (sigma 15 % of each side), cut to the inner 70 %.
/// Elements under 16 px keep to the centre plus or minus 2 px.
fn click_point(rng: &mut Rng, element: ElementBox) -> (f64, f64) {
    let axis = |rng: &mut Rng, side: f64| {
        if side < 16.0 {
            rng.range(-2.0, 2.0)
        } else {
            (rng.normal() * 0.15 * side).clamp(-0.35 * side, 0.35 * side)
        }
    };
    (axis(rng, element.width), axis(rng, element.height))
}
fn clamp_to(point: (f64, f64), reach: Reach) -> (i32, i32) {
    (
        (point.0.round() as i32).clamp(-reach.left.max(0), reach.right.max(0)),
        (point.1.round() as i32).clamp(-reach.up.max(0), reach.down.max(0)),
    )
}
/// The cursor's way to a point of the element, as a human moves a mouse. It starts at `from` (where the cursor
/// was left, relative to this element's centre; `None` for the first move of a session, which starts 180-420 px
/// away), follows an eased cubic Bezier with jitter, takes as long as Fitts' law says (250 + 120 log2(D/W + 1)
/// ms, plus or minus 20 %) in unequal steps, and in about 15 % of the clicks overshoots by 3-8 px and returns.
/// Then it hovers 15-90 ms (people click almost as soon as the cursor stops) and holds the button 50-140 ms. The browser's own pointer input performs it, so
/// every move reaches the page as a trusted `mousemove`.
pub fn click_plan(seed: u64, reach: Reach, element: ElementBox, from: Option<(i32, i32)>) -> ClickPlan {
    let mut rng = Rng::new(seed);
    let end = clamp_to(click_point(&mut rng, element), reach);
    let start = match from {
        Some(point) => clamp_to((point.0 as f64, point.1 as f64), reach),
        None => {
            let (distance, angle) = (rng.range(180.0, 420.0), rng.range(0.0, std::f64::consts::TAU));
            clamp_to((end.0 as f64 + angle.cos() * distance, end.1 as f64 + angle.sin() * distance), reach)
        }
    };
    let hover_ms = rng.ms(POINTER_BOUNDS.hover_ms);
    let press_ms = rng.ms(POINTER_BOUNDS.press_ms);
    let (dx, dy) = ((end.0 - start.0) as f64, (end.1 - start.1) as f64);
    let distance = dx.hypot(dy);
    if distance < 3.0 {
        let ms = rng.ms((60, 120));
        return ClickPlan { moves: vec![PointerStep { x: end.0, y: end.1, ms }], hover_ms, press_ms, end };
    }
    let overshoot = distance > 40.0 && rng.unit() < 0.15;
    let aim = if overshoot {
        let extra = rng.range(3.0, 8.0);
        clamp_to((end.0 as f64 + dx / distance * extra, end.1 as f64 + dy / distance * extra), reach)
    } else {
        end
    };
    let width = element.width.min(element.height).max(8.0);
    let fitts = (250.0 + 120.0 * (distance / width + 1.0).log2()) * rng.range(0.8, 1.2);
    let steps = ((fitts / 16.0).round() as usize).clamp(8, 64);
    // Unequal step times that add up to the Fitts time.
    let weights: Vec<f64> = (0..steps).map(|_| rng.range(0.7, 1.3)).collect();
    let weight_sum: f64 = weights.iter().sum();
    let (px, py) = (-dy / distance, dx / distance);
    let bulge1 = (rng.unit() - 0.5) * distance * 0.5;
    let bulge2 = (rng.unit() - 0.5) * distance * 0.35;
    let (sx, sy) = (start.0 as f64, start.1 as f64);
    let (ax, ay) = (aim.0 as f64, aim.1 as f64);
    let (ex, ey) = (ax - sx, ay - sy);
    let c1 = (sx + ex * 0.28 + px * bulge1, sy + ey * 0.28 + py * bulge1);
    let c2 = (sx + ex * 0.66 + px * bulge2, sy + ey * 0.66 + py * bulge2);
    let mut moves = Vec::with_capacity(steps + 1);
    for (i, weight) in weights.iter().enumerate() {
        let t = (i + 1) as f64 / steps as f64;
        // Slow at both ends, fast in the middle.
        let eased = t * t * (3.0 - 2.0 * t);
        let u = 1.0 - eased;
        let mut x = u.powi(3) * sx + 3.0 * u * u * eased * c1.0 + 3.0 * u * eased * eased * c2.0 + eased.powi(3) * ax;
        let mut y = u.powi(3) * sy + 3.0 * u * u * eased * c1.1 + 3.0 * u * eased * eased * c2.1 + eased.powi(3) * ay;
        if i + 1 < steps {
            x += (rng.unit() - 0.5) * 3.0;
            y += (rng.unit() - 0.5) * 3.0;
        } else {
            (x, y) = (ax, ay);
        }
        let ms = ((fitts * weight / weight_sum).round() as u64).max(1);
        let (x, y) = clamp_to((x, y), reach);
        moves.push(PointerStep { x, y, ms });
    }
    if overshoot && aim != end {
        let ms = rng.ms((70, 140));
        moves.push(PointerStep { x: end.0, y: end.1, ms });
    } else if let Some(last) = moves.last_mut() {
        (last.x, last.y) = end;
    }
    ClickPlan { moves, hover_ms, press_ms, end }
}

/// One typed key: how long it is held and the pause before the next one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keystroke {
    pub hold_ms: u64,
    pub pause_ms: u64,
}
/// The rhythm of typing a field: the wait after the field takes focus and a hold and a pause for every key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypingRhythm {
    pub first_ms: u64,
    pub keys: Vec<Keystroke>,
}
/// The whole field takes the profile's typing delay times the number of keys (what a fixed pause between keys
/// used to take) times 1.25 .. 1.65: never more than 70 % longer, whatever the seed. The time is shared out
/// among the keys with a log-normal spread, and about one key in twenty-five is a hesitation of two to three
/// times the usual interval. Every key is held for 45-75 % of its own interval (12-130 ms, so a fast setting
/// holds the key shorter) and the pause is the rest of it; the first key waits 120-320 ms, at most a quarter of
/// the field's time. A delay of 0 (or nothing to type) adds no time at all.
pub fn typing_rhythm(seed: u64, median_ms: u64, count: usize) -> TypingRhythm {
    let mut rng = Rng::new(seed ^ 0x7479_7069_6E67);
    if median_ms == 0 || count == 0 {
        return TypingRhythm { first_ms: 0, keys: vec![Keystroke { hold_ms: 0, pause_ms: 0 }; count] };
    }
    let total = count as f64 * median_ms as f64 * rng.range(1.25, 1.65);
    let first_ms = (rng.ms((120, 320)) as f64).min(total * 0.25).round() as u64;
    let budget = total - first_ms as f64;
    let weights: Vec<f64> = (0..count)
        .map(|_| {
            let weight = (0.4 * rng.normal()).exp().clamp(0.4, 2.2);
            if rng.unit() < 0.04 { weight * rng.range(2.0, 3.5) } else { weight }
        })
        .collect();
    let sum: f64 = weights.iter().sum();
    let keys = weights
        .iter()
        .map(|weight| {
            let interval = (budget * weight / sum).round().max(8.0);
            let hold = (interval * rng.range(0.45, 0.75)).clamp(12.0, 130.0).min(interval - 6.0).max(1.0);
            Keystroke { hold_ms: hold.round() as u64, pause_ms: (interval - hold.round()) as u64 }
        })
        .collect();
    TypingRhythm { first_ms, keys }
}

/// How long a person who knows the form looks at a page that has just loaded before the first action on it:
/// 0.4-1.0 s plus about 8 ms for every visible word (at most 1.5 s for the words). Added to the profile's action delay.
pub fn reading_pause_ms(seed: u64, visible_words: usize) -> u64 {
    let mut rng = Rng::new(seed ^ 0x7265_6164);
    rng.ms((400, 1000)) + (visible_words as u64 * 8).min(1500)
}

/// How far to scroll (positive: down) so that an element is comfortably in view, from its centre height
/// `centre_y` and `height` inside a viewport `viewport_height` high. 0 when it is fully inside with a 60 px
/// margin; otherwise the distance that brings its centre to 35-60 % of the viewport height (not to the edge).
pub fn scroll_needed(seed: u64, centre_y: f64, height: f64, viewport_height: f64) -> i32 {
    const MARGIN: f64 = 60.0;
    if centre_y - height / 2.0 >= MARGIN && centre_y + height / 2.0 <= viewport_height - MARGIN {
        return 0;
    }
    let mut rng = Rng::new(seed ^ 0x7363_726F_6C6C);
    (centre_y - viewport_height * rng.range(0.35, 0.6)).round() as i32
}
/// One notch of the mouse wheel and the pause after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WheelNotch {
    pub delta: i32,
    pub pause_ms: u64,
}
/// A scroll of `distance` px (negative: up) as wheel notches of 80-120 px with 40-120 ms between them; the last
/// notch is cut so the sum is exactly the distance. A person turns a wheel, a script jumps.
pub fn wheel_plan(seed: u64, distance: i32) -> Vec<WheelNotch> {
    let mut rng = Rng::new(seed ^ 0x0077_6865_656C);
    let sign = distance.signum();
    let mut left = distance.abs();
    let mut notches = Vec::new();
    while left > 0 {
        let delta = (rng.ms((80, 120)) as i32).min(left);
        left -= delta;
        notches.push(WheelNotch { delta: delta * sign, pause_ms: rng.ms((40, 120)) });
    }
    notches
}

/// A few small cursor movements for a page that is only being waited on: the next ones come in 2-8 s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdleMotion {
    pub next_in_ms: u64,
    /// Offsets from the current cursor position.
    pub moves: Vec<PointerStep>,
}
pub fn idle_motion(seed: u64) -> IdleMotion {
    let mut rng = Rng::new(seed ^ 0x6964_6C65);
    let count = 1 + (rng.unit() * 3.0) as usize % 3;
    let moves = (0..count)
        .map(|_| loop {
            let (x, y) = (rng.range(-14.0, 14.0).round() as i32, rng.range(-14.0, 14.0).round() as i32);
            if (x, y) != (0, 0) {
                break PointerStep { x, y, ms: rng.ms((80, 250)) };
            }
        })
        .collect();
    IdleMotion { next_in_ms: rng.ms((2000, 8000)), moves }
}
/// The cursor is only moved on its own while the app drives the page (not handed over, not paused) and when
/// it is known where the pointer is.
pub fn idle_motion_allowed(mode: crate::control::ControlMode, pointer_known: bool) -> bool {
    mode == crate::control::ControlMode::Automatic && pointer_known
}

/// Cuts idle moves (offsets from the current position) so every destination stays 2 px inside the viewport, drops
/// the moves that would change nothing, and returns the offsets to send with the position the cursor ends at.
pub fn clamp_idle_moves(start: (f64, f64), moves: &[PointerStep], viewport: (f64, f64)) -> (Vec<PointerStep>, (f64, f64)) {
    let (mut x, mut y) = start;
    let mut sent = Vec::with_capacity(moves.len());
    for step in moves {
        let next = (
            (x + f64::from(step.x)).clamp(2.0, (viewport.0 - 2.0).max(2.0)),
            (y + f64::from(step.y)).clamp(2.0, (viewport.1 - 2.0).max(2.0)),
        );
        let (dx, dy) = ((next.0 - x).round() as i32, (next.1 - y).round() as i32);
        if (dx, dy) != (0, 0) {
            sent.push(PointerStep { x: dx, y: dy, ms: step.ms });
            (x, y) = (x + f64::from(dx), y + f64::from(dy));
        }
    }
    (sent, (x, y))
}
