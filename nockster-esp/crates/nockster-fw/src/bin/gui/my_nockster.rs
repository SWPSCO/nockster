//! A tiny, deliberately low-stakes virtual pet hidden in the About screen.
//!
//! The state is RAM-only and has no connection to wallet storage, signing, or
//! entropy generation. Needs decay only while this screen is open.

use embedded_graphics::mono_font::{ascii::FONT_6X10, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Alignment, Text};

use super::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};
use super::layout::header_height;
use super::palette;
use super::render::render_header_with_back;
use super::state::{Button, ButtonHit};
use super::time::{Duration, Instant};
use super::GuiDisplay;

const MAX_NEED: u8 = 5;
const FATAL_OVERFEED: u8 = 8;
const DECAY_INTERVAL: Duration = Duration::from_secs(24);
const ANIMATION_INTERVAL: Duration = Duration::from_millis(900);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NocksterButton {
    Feed,
    Play,
    Nap,
    Pet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pose {
    Idle,
    Eating,
    Playing,
    Sleeping,
    Petted,
    Dead,
}

pub struct NocksterState {
    food: u8,
    fun: u8,
    energy: u8,
    overfed: u8,
    decay_cursor: u8,
    pose: Pose,
    frame: bool,
    last_decay: Instant,
    last_frame: Instant,
}

impl NocksterState {
    pub fn new(now: Instant) -> Self {
        Self {
            food: 4,
            fun: 4,
            energy: 4,
            overfed: 0,
            decay_cursor: 0,
            pose: Pose::Idle,
            frame: false,
            last_decay: now,
            last_frame: now,
        }
    }

    /// Resume without charging the pet for time spent outside the easter egg.
    pub fn resume(&mut self, now: Instant) {
        self.last_decay = now;
        self.last_frame = now;
        if self.pose != Pose::Dead {
            self.pose = Pose::Idle;
        }
    }

    /// Advance animation and at most one need. Returns true when a redraw is due.
    pub fn advance(&mut self, now: Instant) -> bool {
        let mut dirty = false;
        if now - self.last_frame >= ANIMATION_INTERVAL {
            self.last_frame = now;
            self.frame = !self.frame;
            if self.pose != Pose::Idle && self.pose != Pose::Dead {
                self.pose = Pose::Idle;
            }
            dirty = true;
        }
        if now - self.last_decay >= DECAY_INTERVAL {
            self.last_decay = now;
            match self.decay_cursor % 3 {
                0 => self.food = self.food.saturating_sub(1),
                1 => self.fun = self.fun.saturating_sub(1),
                _ => self.energy = self.energy.saturating_sub(1),
            }
            self.decay_cursor = self.decay_cursor.wrapping_add(1);
            dirty = true;
        }
        dirty
    }

    pub fn act(&mut self, button: NocksterButton, now: Instant) {
        if self.pose == Pose::Dead {
            if button == NocksterButton::Pet {
                *self = Self::new(now);
                self.pose = Pose::Petted;
            }
            return;
        }
        match button {
            NocksterButton::Feed => {
                if self.food == MAX_NEED {
                    self.overfed = self.overfed.saturating_add(1);
                }
                self.food = self.food.saturating_add(2).min(MAX_NEED);
                self.fun = self.fun.saturating_sub(1);
                self.pose = if self.overfed >= FATAL_OVERFEED {
                    Pose::Dead
                } else {
                    Pose::Eating
                };
            }
            NocksterButton::Play => {
                self.fun = self.fun.saturating_add(2).min(MAX_NEED);
                self.food = self.food.saturating_sub(1);
                self.energy = self.energy.saturating_sub(1);
                self.overfed = self.overfed.saturating_sub(1);
                self.pose = Pose::Playing;
            }
            NocksterButton::Nap => {
                self.energy = self.energy.saturating_add(2).min(MAX_NEED);
                self.food = self.food.saturating_sub(1);
                self.pose = Pose::Sleeping;
            }
            NocksterButton::Pet => {
                self.fun = self.fun.saturating_add(1).min(MAX_NEED);
                self.pose = Pose::Petted;
            }
        }
        self.last_frame = now;
    }

    fn message(&self) -> &'static str {
        match self.pose {
            Pose::Eating => "yum!",
            Pose::Playing => "wheee!",
            Pose::Sleeping => "z z z...",
            Pose::Petted => "happy!",
            Pose::Dead => "tap RIP to restart",
            Pose::Idle if self.overfed >= 5 => "too full...",
            Pose::Idle if self.overfed >= 2 => "getting round...",
            Pose::Idle if self.food == 0 => "i'm hungry",
            Pose::Idle if self.energy == 0 => "i'm sleepy",
            Pose::Idle if self.fun == 0 => "let's play",
            Pose::Idle
                if self.food == MAX_NEED && self.fun == MAX_NEED && self.energy == MAX_NEED =>
            {
                "doing great!"
            }
            Pose::Idle => "hello!",
        }
    }
}

pub fn render(display: &mut GuiDisplay<'_>, state: &NocksterState) {
    let _ = display.clear(palette::background());
    render_header_with_back(display, "My Nockster", palette::surface_high(), false);
    draw_needs(display, state);
    draw_room(display, state);
    for hit in action_buttons() {
        draw_button(display, hit, false);
    }
}

/// Redraw the animated room without touching the header or action buttons.
pub fn render_room(display: &mut GuiDisplay<'_>, state: &NocksterState) {
    draw_room(display, state);
    draw_needs(display, state);
}

pub fn button_from_point(point: Point) -> Option<ButtonHit> {
    action_buttons()
        .into_iter()
        .find(|hit| expanded_contains(*hit, point, 5))
        .or_else(|| {
            let pet = pet_hit();
            pet_rect().contains(point).then_some(pet)
        })
}

pub fn draw_button(display: &mut GuiDisplay<'_>, hit: ButtonHit, active: bool) {
    let Button::Nockster(button) = hit.button else {
        return;
    };
    if button == NocksterButton::Pet {
        return;
    }
    let fill = if active {
        palette::keypad_active()
    } else {
        palette::keypad_idle()
    };
    let rect = Rectangle::new(hit.top_left, hit.size);
    let _ = rect
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(fill)
                .stroke_color(if active {
                    palette::keypad_active_light()
                } else {
                    palette::divider()
                })
                .stroke_width(2)
                .build(),
        )
        .draw(display);
    let label = match button {
        NocksterButton::Feed => "FEED",
        NocksterButton::Play => "PLAY",
        NocksterButton::Nap => "NAP",
        NocksterButton::Pet => "",
    };
    let style = MonoTextStyle::new(&FONT_6X10, palette::text());
    let baseline = hit.top_left.y + hit.size.height as i32 / 2 + 4;
    let _ = Text::with_alignment(
        label,
        Point::new(hit.top_left.x + hit.size.width as i32 / 2, baseline),
        style,
        Alignment::Center,
    )
    .draw(display);
}

fn draw_needs(display: &mut GuiDisplay<'_>, state: &NocksterState) {
    let top = header_height() + 6;
    draw_need(display, 6, top, "FOOD", state.food, Rgb565::new(29, 35, 5));
    draw_need(display, 61, top, "FUN", state.fun, Rgb565::new(31, 18, 12));
    draw_need(
        display,
        116,
        top,
        "ZZZ",
        state.energy,
        Rgb565::new(7, 38, 29),
    );
}

fn draw_need(display: &mut GuiDisplay<'_>, x: i32, y: i32, label: &str, value: u8, color: Rgb565) {
    let clear = Rectangle::new(Point::new(x, y), Size::new(50, 29));
    let _ = clear
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::background())
                .build(),
        )
        .draw(display);
    let label_style = MonoTextStyle::new(&FONT_6X10, palette::text_subtle());
    let _ = Text::new(label, Point::new(x, y + 9), label_style).draw(display);
    for index in 0..MAX_NEED {
        let filled = index < value;
        let pip = Rectangle::new(Point::new(x + index as i32 * 10, y + 16), Size::new(8, 8));
        let _ = pip
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(if filled {
                        color
                    } else {
                        palette::surface_low()
                    })
                    .stroke_color(palette::divider())
                    .stroke_width(1)
                    .build(),
            )
            .draw(display);
    }
}

fn draw_room(display: &mut GuiDisplay<'_>, state: &NocksterState) {
    let room = room_rect();
    let _ = room
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::surface_low())
                .stroke_color(palette::divider())
                .stroke_width(2)
                .build(),
        )
        .draw(display);

    // A chunky checkerboard floor makes the tiny screen read like a game.
    let floor_y = room.top_left.y + room.size.height as i32 - 32;
    for row in 0..2 {
        for col in 0..8 {
            if (row + col) % 2 == 0 {
                let tile = Rectangle::new(
                    Point::new(room.top_left.x + 2 + col * 19, floor_y + row * 15),
                    Size::new(19, 15),
                );
                let _ = tile
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(palette::panel_shadow())
                            .build(),
                    )
                    .draw(display);
            }
        }
    }

    draw_pixel_pet(display, state);
    let message_style = MonoTextStyle::new(&FONT_6X10, palette::text());
    let _ = Text::with_alignment(
        state.message(),
        Point::new(
            SCREEN_WIDTH as i32 / 2,
            room.top_left.y + room.size.height as i32 - 8,
        ),
        message_style,
        Alignment::Center,
    )
    .draw(display);
}

fn draw_pixel_pet(display: &mut GuiDisplay<'_>, state: &NocksterState) {
    if state.pose == Pose::Dead {
        draw_tombstone(display);
        return;
    }

    // Original 15x15 sprite. Each character becomes one 4x4 display pixel.
    const SPRITE: [&str; 15] = [
        "...###...###...",
        "..#ooo#.#ooo#..",
        "..#ooo#.#ooo#..",
        "..#ooo###ooo#..",
        "...#ooooooo#...",
        "..#ooooooooo#..",
        ".#ooooooooooo#.",
        "#ooeoooooooeoo#",
        "#ooooooooooooo#",
        "#obooooooooobo#",
        "#ooooooeoooooo#",
        ".#ooooeoeoooo#.",
        "..#ooooooooo#..",
        "...#ooooooo#...",
        "...###...###...",
    ];
    let scale_y = 4;
    let scale_x = match state.overfed {
        0..=1 => 4,
        2..=4 => 5,
        _ => 6,
    };
    let bounce = if state.frame && state.pose != Pose::Sleeping {
        -2
    } else {
        0
    };
    let origin = Point::new(
        (SCREEN_WIDTH as i32 - 15 * scale_x) / 2,
        room_rect().top_left.y + 17 + bounce,
    );
    let outline = palette::text();
    let body = Rgb565::new(31, 43, 19);
    let blush = Rgb565::new(31, 18, 19);
    let face = palette::background();

    for (row, line) in SPRITE.iter().enumerate() {
        for (col, pixel) in line.bytes().enumerate() {
            let color = match pixel {
                b'#' => Some(outline),
                b'o' => Some(body),
                b'b' => Some(blush),
                b'e' => Some(face),
                _ => None,
            };
            if let Some(color) = color {
                let block = Rectangle::new(
                    Point::new(
                        origin.x + col as i32 * scale_x,
                        origin.y + row as i32 * scale_y,
                    ),
                    Size::new(scale_x as u32, scale_y as u32),
                );
                let _ = block
                    .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
                    .draw(display);
            }
        }
    }

    if state.pose == Pose::Sleeping {
        for col in [3, 11] {
            let open_eye = Rectangle::new(
                Point::new(origin.x + col * scale_x, origin.y + 7 * scale_y),
                Size::new(scale_x as u32, scale_y as u32),
            );
            let _ = open_eye
                .into_styled(PrimitiveStyleBuilder::new().fill_color(body).build())
                .draw(display);
            let eye = Rectangle::new(
                Point::new(
                    origin.x + col * scale_x,
                    origin.y + 7 * scale_y + scale_y / 2,
                ),
                Size::new(scale_x as u32, 2),
            );
            let _ = eye
                .into_styled(PrimitiveStyleBuilder::new().fill_color(face).build())
                .draw(display);
        }
        let z_style = MonoTextStyle::new(&FONT_6X10, palette::text());
        let _ = Text::new("Z", Point::new(origin.x + 61, origin.y + 10), z_style).draw(display);
        let _ = Text::new("z", Point::new(origin.x + 70, origin.y + 2), z_style).draw(display);
    } else if state.pose == Pose::Eating {
        draw_n_treat(display, Point::new(origin.x - 27, origin.y + 34), 2);
        draw_n_treat(display, Point::new(origin.x - 8, origin.y + 39), 1);
    } else if state.pose == Pose::Petted {
        draw_pixel_heart(
            display,
            Point::new(origin.x + 15 * scale_x + 5, origin.y + 8),
        );
    }
}

/// The bundled bitmap font doesn't contain mathematical double-struck `𝕟`,
/// so feeding uses a small hand-drawn, double-stemmed lowercase n.
fn draw_n_treat(display: &mut GuiDisplay<'_>, origin: Point, scale: i32) {
    const GLYPH: [&str; 7] = [
        ".......", "##.###.", "###..##", "##...##", "##...##", "##...##", "##...##",
    ];
    let color = Rgb565::new(31, 37, 7);
    for (row, line) in GLYPH.iter().enumerate() {
        for (col, pixel) in line.bytes().enumerate() {
            if pixel != b'#' {
                continue;
            }
            let _ = Rectangle::new(
                Point::new(origin.x + col as i32 * scale, origin.y + row as i32 * scale),
                Size::new(scale as u32, scale as u32),
            )
            .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
            .draw(display);
        }
    }
}

fn draw_pixel_heart(display: &mut GuiDisplay<'_>, origin: Point) {
    let pink = Rgb565::new(31, 16, 19);
    for (x, y) in [
        (0, 0),
        (2, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
        (2, 1),
        (3, 1),
        (0, 2),
        (1, 2),
        (2, 2),
        (1, 3),
    ] {
        let _ = Rectangle::new(
            Point::new(origin.x + x * 3, origin.y + y * 3),
            Size::new(3, 3),
        )
        .into_styled(PrimitiveStyleBuilder::new().fill_color(pink).build())
        .draw(display);
    }
}

fn draw_tombstone(display: &mut GuiDisplay<'_>) {
    let room = room_rect();
    let x = room.top_left.x + room.size.width as i32 / 2 - 32;
    let y = room.top_left.y + 21;
    let stone = palette::text_subtle();
    let outline = palette::text();

    // Square pixels keep the grave in the same visual language as the pet.
    for (px, py, width, height) in [
        (x + 12, y, 40, 4),
        (x + 4, y + 4, 56, 4),
        (x, y + 8, 64, 48),
        (x - 8, y + 56, 80, 8),
    ] {
        let _ = Rectangle::new(Point::new(px, py), Size::new(width, height))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(stone)
                    .stroke_color(outline)
                    .stroke_width(if height > 8 { 2 } else { 0 })
                    .build(),
            )
            .draw(display);
    }
    let rip = MonoTextStyle::new(&FONT_6X10, palette::background());
    let _ = Text::with_alignment(
        "RIP",
        Point::new(SCREEN_WIDTH as i32 / 2, y + 35),
        rip,
        Alignment::Center,
    )
    .draw(display);
}

fn room_rect() -> Rectangle {
    Rectangle::new(Point::new(7, header_height() + 38), Size::new(158, 145))
}

fn pet_rect() -> Rectangle {
    Rectangle::new(Point::new(38, header_height() + 45), Size::new(96, 90))
}

fn pet_hit() -> ButtonHit {
    let rect = pet_rect();
    ButtonHit {
        button: Button::Nockster(NocksterButton::Pet),
        top_left: rect.top_left,
        size: rect.size,
    }
}

fn action_buttons() -> [ButtonHit; 3] {
    let margin = 7;
    let gap = 5;
    let width = (SCREEN_WIDTH as i32 - margin * 2 - gap * 2) / 3;
    let top = SCREEN_HEIGHT as i32 - 47;
    [
        action_hit(NocksterButton::Feed, margin, top, width),
        action_hit(NocksterButton::Play, margin + width + gap, top, width),
        action_hit(NocksterButton::Nap, margin + (width + gap) * 2, top, width),
    ]
}

fn action_hit(button: NocksterButton, x: i32, y: i32, width: i32) -> ButtonHit {
    ButtonHit {
        button: Button::Nockster(button),
        top_left: Point::new(x, y),
        size: Size::new(width as u32, 40),
    }
}

fn expanded_contains(hit: ButtonHit, point: Point, slack: i32) -> bool {
    point.x >= hit.top_left.x - slack
        && point.x < hit.top_left.x + hit.size.width as i32 + slack
        && point.y >= hit.top_left.y - slack
        && point.y < hit.top_left.y + hit.size.height as i32 + slack
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_have_deliberate_tradeoffs_and_stay_bounded() {
        let now = Instant::now();
        let mut pet = NocksterState::new(now);
        pet.act(NocksterButton::Play, now);
        assert_eq!((pet.food, pet.fun, pet.energy), (3, 5, 3));
        for _ in 0..8 {
            pet.act(NocksterButton::Feed, now);
        }
        assert_eq!(pet.food, MAX_NEED);
        assert_eq!(pet.fun, 0);
    }

    #[test]
    fn repeated_overfeeding_gets_wider_then_kills_and_pet_restarts() {
        let now = Instant::now();
        let mut pet = NocksterState::new(now);
        pet.act(NocksterButton::Feed, now);
        for _ in 0..FATAL_OVERFEED {
            pet.act(NocksterButton::Feed, now);
        }
        assert_eq!(pet.pose, Pose::Dead);
        pet.act(NocksterButton::Pet, now);
        assert_eq!(pet.pose, Pose::Petted);
        assert_eq!(pet.overfed, 0);
    }
}
