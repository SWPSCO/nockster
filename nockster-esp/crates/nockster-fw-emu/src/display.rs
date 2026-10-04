use core::convert::Infallible;

use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::{prelude::*, Clamped};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

pub struct WasmDisplay {
    ctx: CanvasRenderingContext2d,
    width: u32,
    height: u32,
}

impl WasmDisplay {
    pub fn new(canvas_id: &str, width: u32, height: u32) -> Result<Self, JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
        let document = window
            .document()
            .ok_or_else(|| JsValue::from_str("no document"))?;
        let canvas = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| JsValue::from_str("canvas not found"))?
            .dyn_into::<HtmlCanvasElement>()?;

        canvas.set_width(width);
        canvas.set_height(height);

        let ctx = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("no 2d context"))?
            .dyn_into::<CanvasRenderingContext2d>()?;

        Ok(Self { ctx, width, height })
    }

    pub fn set_pixels<I>(
        &mut self,
        x0: u16,
        y0: u16,
        x1: u16,
        y1: u16,
        pixels: I,
    ) -> Result<(), Infallible>
    where
        I: IntoIterator<Item = Rgb565>,
    {
        let width = u32::from(x1.saturating_sub(x0)) + 1;
        let height = u32::from(y1.saturating_sub(y0)) + 1;
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);

        for color in pixels.into_iter() {
            let (r, g, b) = rgb565_to_rgb888(color);
            rgba.extend_from_slice(&[r, g, b, 255]);
        }

        if rgba.len() == width as usize * height as usize * 4 {
            if let Ok(image) =
                ImageData::new_with_u8_clamped_array_and_sh(Clamped(&rgba), width, height)
            {
                let _ = self
                    .ctx
                    .put_image_data(&image, f64::from(x0), f64::from(y0));
                return Ok(());
            }
        }

        let mut iter = rgba.chunks_exact(4);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let Some(color) = iter.next() else {
                    return Ok(());
                };
                self.ctx
                    .set_fill_style_str(&format!("rgb({},{},{})", color[0], color[1], color[2]));
                self.ctx.fill_rect(x as f64, y as f64, 1.0, 1.0);
            }
        }
        Ok(())
    }

    fn draw_pixel(&mut self, x: i32, y: i32, color: Rgb565) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let (r, g, b) = rgb565_to_rgb888(color);
        self.ctx
            .set_fill_style_str(&format!("rgb({},{},{})", r, g, b));
        self.ctx.fill_rect(x as f64, y as f64, 1.0, 1.0);
    }
}

impl OriginDimensions for WasmDisplay {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}

impl DrawTarget for WasmDisplay {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            self.draw_pixel(point.x, point.y, color);
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let (r, g, b) = rgb565_to_rgb888(color);
        self.ctx
            .set_fill_style_str(&format!("rgb({},{},{})", r, g, b));
        self.ctx
            .fill_rect(0.0, 0.0, self.width as f64, self.height as f64);
        Ok(())
    }
}

fn rgb565_to_rgb888(color: Rgb565) -> (u8, u8, u8) {
    let r = color.r();
    let g = color.g();
    let b = color.b();
    (
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    )
}
