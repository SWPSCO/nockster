use embedded_graphics::{prelude::{Point, Size}, primitives::Rectangle};

use super::constants::{BOOT_LOGO_HEIGHT, SCREEN_HEIGHT, SCREEN_WIDTH};

use super::state::{Button, ButtonHit};

const KEYPAD_PADDING: i32 = 6;

pub(crate) fn keypad_button_hit(row: usize, col: usize) -> ButtonHit {
    let width = SCREEN_WIDTH as i32;
    let row_height = row_height();
    let header_h = header_height();
    let padding = KEYPAD_PADDING;
    let button_width = (width - padding * 4) / 3;
    let top_margin = if row == 0 { padding } else { padding / 2 };
    let mut bottom_margin = if row == 3 { padding / 4 } else { padding / 2 };
    if bottom_margin == 0 {
        bottom_margin = 1;
    }
    let mut button_height = (row_height - top_margin - bottom_margin).max(8);
    let x = padding + col as i32 * (button_width + padding);
    let mut y = header_h + row as i32 * row_height + top_margin;
    let row_bottom = header_h + (row as i32 + 1) * row_height;
    if y + button_height > row_bottom {
        button_height = row_bottom - y;
    }
    if row == 3 {
        let extra = padding * 2;
        button_height += extra;
        let max_y = SCREEN_HEIGHT as i32 - padding;
        if y + button_height > max_y {
            button_height = max_y.saturating_sub(y);
        }
    }
    if row == 0 && y < header_h + 1 {
        y = header_h + 1;
    }
    ButtonHit {
        button: keypad_grid()[row][col],
        top_left: Point::new(x, y),
        size: Size::new(button_width as u32, button_height as u32),
    }
}

pub(crate) fn confirm_buttons() -> [ButtonHit; 2] {
    let margin = 6;
    let gap = 6;
    let header_h = header_height();
    let width = (SCREEN_WIDTH as i32 - margin * 2).max(80);
    let button_h = 44;
    let top = (SCREEN_HEIGHT as i32 - margin - button_h).max(header_h + margin);
    let button_w = ((width - gap) / 2).max(32);

    let reject = ButtonHit {
        button: Button::Clear,
        top_left: Point::new(margin, top),
        size: Size::new(button_w as u32, button_h as u32),
    };
    let approve = ButtonHit {
        button: Button::Ok,
        top_left: Point::new(margin + button_w + gap, top),
        size: Size::new(button_w as u32, button_h as u32),
    };

    [reject, approve]
}

pub(crate) fn button_from_point_keypad(point: Point) -> Option<ButtonHit> {
    let slack: i32 = 16;
    for row in 0..4 {
        for col in 0..3 {
            let hit = keypad_button_hit(row, col);
            let left = hit.top_left.x - slack;
            let right = hit.top_left.x + hit.size.width as i32 + slack;
            let mut top = hit.top_left.y - slack;
            if row == 0 {
                top = top.min(header_height());
            }
            let extra_bottom = if row == 3 { slack * 2 } else { slack };
            let mut bottom = hit.top_left.y + hit.size.height as i32 + extra_bottom;
            let screen_bottom = SCREEN_HEIGHT as i32;
            if bottom > screen_bottom {
                bottom = screen_bottom;
            }
            if point.x >= left && point.x < right && point.y >= top && point.y < bottom {
                return Some(hit);
            }
        }
    }
    None
}

pub(crate) fn button_from_point_confirm(point: Point) -> Option<ButtonHit> {
    let bottom_slack: i32 = 16;
    for hit in confirm_buttons() {
        let within_x =
            point.x >= hit.top_left.x && point.x < hit.top_left.x + hit.size.width as i32;
        let bottom = (hit.top_left.y + hit.size.height as i32 + bottom_slack)
            .min(SCREEN_HEIGHT as i32);
        let within_y = point.y >= hit.top_left.y && point.y < bottom;
        if within_x && within_y {
            return Some(hit);
        }
    }
    None
}

pub(crate) fn tx_review_buttons() -> [ButtonHit; 2] {
    let margin = 6;
    let gap = 6;
    let header_h = header_height();
    let width = (SCREEN_WIDTH as i32 - margin * 2).max(80);
    let button_h = 44;
    let top = (SCREEN_HEIGHT as i32 - margin - button_h).max(header_h + margin);

    let button_w = ((width - gap) / 2).max(32);

    let deny = ButtonHit {
        button: Button::Clear,
        top_left: Point::new(margin, top),
        size: Size::new(button_w as u32, button_h as u32),
    };
    let confirm = ButtonHit {
        button: Button::Ok,
        top_left: Point::new(margin + button_w + gap, top),
        size: Size::new(button_w as u32, button_h as u32),
    };

    [deny, confirm]
}

pub(crate) fn tx_review_list_rect() -> Rectangle {
    let margin = 6;
    let header_h = header_height();
    let top = header_h + margin;
    let buttons = tx_review_buttons();
    let buttons_top = buttons[0].top_left.y;
    let bottom = (buttons_top - margin).max(top + 24);

    Rectangle::new(
        Point::new(margin, top),
        Size::new(
            (SCREEN_WIDTH as i32 - margin * 2).max(40) as u32,
            (bottom - top).max(0) as u32,
        ),
    )
}

pub(crate) fn tx_review_detail_rect() -> Rectangle {
    let list = tx_review_list_rect();
    let inset: i32 = 10;
    if list.size.width <= (inset as u32 * 2) || list.size.height <= (inset as u32 * 2) {
        return list;
    }
    Rectangle::new(
        Point::new(list.top_left.x + inset, list.top_left.y + inset),
        Size::new(
            list.size.width.saturating_sub((inset as u32) * 2),
            list.size.height.saturating_sub((inset as u32) * 2),
        ),
    )
}

pub(crate) fn button_from_point_tx_review(point: Point) -> Option<ButtonHit> {
    let bottom_slack: i32 = 16;
    for hit in tx_review_buttons() {
        let within_x =
            point.x >= hit.top_left.x && point.x < hit.top_left.x + hit.size.width as i32;
        let bottom = (hit.top_left.y + hit.size.height as i32 + bottom_slack)
            .min(SCREEN_HEIGHT as i32);
        let within_y = point.y >= hit.top_left.y && point.y < bottom;
        if within_x && within_y {
            return Some(hit);
        }
    }
    None
}

pub(crate) fn keypad_grid() -> [[Button; 3]; 4] {
    [
        [Button::Digit(1), Button::Digit(2), Button::Digit(3)],
        [Button::Digit(4), Button::Digit(5), Button::Digit(6)],
        [Button::Digit(7), Button::Digit(8), Button::Digit(9)],
        [Button::Clear, Button::Digit(0), Button::Ok],
    ]
}

pub(crate) fn header_height() -> i32 {
    row_height()
}

pub(crate) fn row_height() -> i32 {
    BOOT_LOGO_HEIGHT as i32 / 5
}

pub(crate) fn lock_button_rect() -> Rectangle {
    Rectangle::new(
        Point::new(0, 0),
        Size::new(SCREEN_WIDTH.into(), header_height() as u32),
    )
}
