use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::OriginDimensions;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};

use super::constants::SCREEN_WIDTH;
use super::palette;
use super::GuiDisplay;

const DRAG_THRESHOLD: i32 = 6;

pub trait ScrollContent {
    fn content_height(&self) -> i32;

    fn draw_content<D>(&self, target: &mut D)
    where
        D: DrawTarget<Color = Rgb565>;
}

pub struct ScrollState {
    viewport: Rectangle,
    offset_y: i32,
    max_offset_y: i32,
    last_drag_y: Option<i32>,
}

impl ScrollState {
    pub fn new(viewport: Rectangle) -> Self {
        Self {
            viewport,
            offset_y: 0,
            max_offset_y: 0,
            last_drag_y: None,
        }
    }

    pub fn reset(&mut self) {
        self.offset_y = 0;
        self.max_offset_y = 0;
        self.last_drag_y = None;
    }

    pub fn contains(&self, point: Point) -> bool {
        self.viewport.contains(point)
    }

    pub fn offset_y(&self) -> i32 {
        self.offset_y
    }

    pub fn viewport(&self) -> Rectangle {
        self.viewport
    }

    pub fn drag_to(&mut self, y: i32) -> bool {
        let Some(last_y) = self.last_drag_y else {
            self.last_drag_y = Some(y);
            return false;
        };
        self.last_drag_y = Some(y);
        let old = self.offset_y;
        let delta = last_y - y;
        self.offset_y = self
            .offset_y
            .saturating_add(delta)
            .clamp(0, self.max_offset_y);
        old != self.offset_y
    }

    pub fn drag_end(&mut self) {
        self.last_drag_y = None;
    }

    pub fn begin_drag(&mut self, y: i32) {
        self.last_drag_y = Some(y);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragUpdate {
    Outside,
    Tracking,
    Dragging { moved: bool },
}

pub struct DragState {
    start: Option<Point>,
    active: bool,
}

impl DragState {
    pub fn new() -> Self {
        Self {
            start: None,
            active: false,
        }
    }

    pub fn reset(&mut self) {
        self.start = None;
        self.active = false;
    }

    pub fn update(&mut self, point: Point, scroll: &mut ScrollState) -> DragUpdate {
        if !scroll.contains(point) {
            self.reset();
            scroll.drag_end();
            return DragUpdate::Outside;
        }

        let Some(start) = self.start else {
            self.start = Some(point);
            self.active = false;
            scroll.begin_drag(point.y);
            return DragUpdate::Tracking;
        };

        if !self.active {
            let dx = point.x - start.x;
            let dy = point.y - start.y;
            if dx.abs() <= DRAG_THRESHOLD && dy.abs() <= DRAG_THRESHOLD {
                return DragUpdate::Tracking;
            }
            self.active = true;
            scroll.begin_drag(start.y);
        }

        DragUpdate::Dragging {
            moved: scroll.drag_to(point.y),
        }
    }
}

impl Default for DragState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render<C>(display: &mut GuiDisplay<'_>, scroll: &mut ScrollState, content: &C)
where
    C: ScrollContent,
{
    let viewport = scroll.viewport;
    let viewport_h = viewport.size.height as i32;
    let content_h = content.content_height().max(0);
    scroll.max_offset_y = content_h.saturating_sub(viewport_h).max(0);
    scroll.offset_y = scroll.offset_y.clamp(0, scroll.max_offset_y);

    let _ = viewport
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::background())
                .stroke_width(0)
                .build(),
        )
        .draw(display);

    let mut target = ScrolledTarget {
        target: display,
        viewport,
        offset_y: scroll.offset_y,
    };
    content.draw_content(&mut target);
    draw_scrollbar(display, scroll, content_h);
}

fn draw_scrollbar(display: &mut GuiDisplay<'_>, scroll: &ScrollState, content_h: i32) {
    if scroll.max_offset_y <= 0 || content_h <= 0 {
        return;
    }
    let viewport = scroll.viewport;
    let track_h = viewport.size.height as i32;
    if track_h <= 8 {
        return;
    }

    let thumb_h = ((track_h * track_h) / content_h).clamp(18, track_h);
    let travel = (track_h - thumb_h).max(1);
    let thumb_y = viewport.top_left.y + (scroll.offset_y * travel) / scroll.max_offset_y.max(1);
    let x = SCREEN_WIDTH as i32 - 7;
    let track = Rectangle::new(
        Point::new(x + 2, viewport.top_left.y + 6),
        Size::new(2, track_h.saturating_sub(12) as u32),
    );
    let _ = track
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::divider())
                .stroke_width(0)
                .build(),
        )
        .draw(display);
    let thumb = Rectangle::new(Point::new(x, thumb_y), Size::new(5, thumb_h as u32));
    let _ = thumb
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::panel_shadow())
                .stroke_color(palette::keypad_active_light())
                .stroke_width(1)
                .build(),
        )
        .draw(display);
    let highlight_h = thumb_h.saturating_sub(4).max(1);
    let highlight = Rectangle::new(
        Point::new(x + 1, thumb_y + 2),
        Size::new(1, highlight_h as u32),
    );
    let _ = highlight
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::panel_highlight())
                .stroke_width(0)
                .build(),
        )
        .draw(display);
}

struct ScrolledTarget<'a, D>
where
    D: DrawTarget<Color = Rgb565>,
{
    target: &'a mut D,
    viewport: Rectangle,
    offset_y: i32,
}

impl<D> ScrolledTarget<'_, D>
where
    D: DrawTarget<Color = Rgb565>,
{
    fn translate(&self, area: &Rectangle) -> Rectangle {
        Rectangle::new(
            Point::new(
                self.viewport.top_left.x + area.top_left.x,
                self.viewport.top_left.y + area.top_left.y - self.offset_y,
            ),
            area.size,
        )
    }
}

impl<D> DrawTarget for ScrolledTarget<'_, D>
where
    D: DrawTarget<Color = Rgb565>,
{
    type Color = Rgb565;
    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let viewport = self.viewport;
        let origin = viewport.top_left;
        let offset_y = self.offset_y;
        self.target
            .draw_iter(pixels.into_iter().filter_map(move |Pixel(point, color)| {
                let mapped = Point::new(origin.x + point.x, origin.y + point.y - offset_y);
                viewport.contains(mapped).then_some(Pixel(mapped, color))
            }))
    }

    // Forward rectangle fills as rectangle fills. The DrawTarget defaults
    // degrade them to draw_iter, which the SPI display services one pixel
    // (one address-window transaction) at a time — slow enough that each
    // scroll step visibly paints top-to-bottom.
    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let clipped = self.translate(area).intersection(&self.viewport);
        if clipped.size.width == 0 || clipped.size.height == 0 {
            return Ok(());
        }
        self.target.fill_solid(&clipped, color)
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let translated = self.translate(area);
        let clipped = translated.intersection(&self.viewport);
        if clipped.size.width == 0 || clipped.size.height == 0 {
            return Ok(());
        }
        if clipped == translated {
            return self.target.fill_contiguous(&translated, colors);
        }
        // Partially visible at a viewport edge: clip per pixel.
        let viewport = self.viewport;
        self.target.draw_iter(
            translated
                .points()
                .zip(colors)
                .filter_map(move |(point, color)| {
                    viewport.contains(point).then_some(Pixel(point, color))
                }),
        )
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.viewport
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(color)
                    .stroke_width(0)
                    .build(),
            )
            .draw(self.target)
    }
}

impl<D> OriginDimensions for ScrolledTarget<'_, D>
where
    D: DrawTarget<Color = Rgb565>,
{
    fn size(&self) -> Size {
        self.viewport.size
    }
}
