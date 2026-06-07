use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::OriginDimensions;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};

use super::constants::SCREEN_WIDTH;
use super::palette;
use super::GuiDisplay;

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

    let thumb_h = ((track_h * track_h) / content_h).clamp(12, track_h);
    let travel = (track_h - thumb_h).max(1);
    let thumb_y = viewport.top_left.y + (scroll.offset_y * travel) / scroll.max_offset_y.max(1);
    let x = SCREEN_WIDTH as i32 - 4;
    let track = Rectangle::new(
        Point::new(x, viewport.top_left.y),
        Size::new(1, track_h as u32),
    );
    let _ = track
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::divider())
                .stroke_width(0)
                .build(),
        )
        .draw(display);
    let thumb = Rectangle::new(Point::new(x, thumb_y), Size::new(2, thumb_h as u32));
    let _ = thumb
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(palette::text_subtle())
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
