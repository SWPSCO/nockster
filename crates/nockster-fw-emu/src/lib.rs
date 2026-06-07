#[allow(dead_code)]
mod display;
#[allow(dead_code)]
mod gui;
#[allow(dead_code)]
#[path = "../../nockster-fw/src/bin/static_slot/mod.rs"]
mod static_slot;

use core::fmt::Write as _;

use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::Text;
use heapless::{String as HString, Vec as HVec};
use nockster_core::{BuildInfo, UpdateTrust, MAX_SEED_LABEL_LEN, MAX_SEED_SLOTS, PROTO_V1};
use wasm_bindgen::prelude::*;

use display::WasmDisplay;
use gui::constants::{
    IDLE_OVERLAY_HEIGHT, IDLE_OVERLAY_MARGIN, MAX_PIN_DIGITS, PIN_BUFFER_LEN, SCREEN_HEIGHT,
    SCREEN_WIDTH,
};
use gui::label::{LabelButton, LabelEntryContext};
use gui::menu::{WalletRow, WalletRows};
use gui::palette;
use gui::seed::{SeedButton, SeedEntryState};
use gui::state::{Button, ButtonHit, GuiMode, MenuItem};
use gui::time::{Duration, Instant};

const SPLASH_DURATION: Duration = Duration::from_millis(1_200);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveTarget {
    HeaderLock,
    HeaderMenu,
    Calibration,
    Diagnostics,
    Button { mode: GuiMode, hit: ButtonHit },
}

const CALIBRATION_POINTS: [gui::ScreenPoint; 4] = [
    gui::ScreenPoint { x: 18, y: 58 },
    gui::ScreenPoint {
        x: SCREEN_WIDTH - 19,
        y: 58,
    },
    gui::ScreenPoint {
        x: SCREEN_WIDTH - 19,
        y: SCREEN_HEIGHT - 19,
    },
    gui::ScreenPoint {
        x: 18,
        y: SCREEN_HEIGHT - 19,
    },
];

fn copied_string<const N: usize>(value: &str) -> HString<N> {
    let mut out = HString::new();
    for ch in value.chars() {
        if out.push(ch).is_err() {
            break;
        }
    }
    out
}

fn demo_about_info() -> gui::menu::AboutInfo {
    gui::menu::AboutInfo {
        fw_major: 0,
        fw_minor: 1,
        release_version: 0,
        build: BuildInfo {
            git_commit: copied_string("browser-demo"),
            git_dirty: false,
            build_profile: copied_string("wasm"),
            protocol_v: PROTO_V1,
            tx_types_rev: copied_string("shared-gui"),
        },
        trust: UpdateTrust {
            configured: false,
            pubkey_sha256: [0u8; 32],
        },
    }
}

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub struct SigerGui {
    display: WasmDisplay,
    mode: GuiMode,
    pin_entered: HVec<u8, PIN_BUFFER_LEN>,
    seed_entry_state: SeedEntryState,
    seed_flow_is_add: bool,
    menu_scroll: gui::scroll::ScrollState,
    wallet_rows: WalletRows,
    wallets_scroll: gui::scroll::ScrollState,
    label_entry_state: gui::label::LabelEntryState,
    unlock_demo_state: Option<gui::demo::AnimationState>,
    unlock_demo_last_frame_start: Option<Instant>,
    idle_message: HString<48>,
    active_target: Option<ActiveTarget>,
    splash_started_at: Option<Instant>,
    calibration_step: usize,
}

#[wasm_bindgen]
impl SigerGui {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<SigerGui, JsValue> {
        let display = WasmDisplay::new(canvas_id, SCREEN_WIDTH.into(), SCREEN_HEIGHT.into())?;
        let mut gui = Self {
            display,
            mode: GuiMode::Splash,
            pin_entered: HVec::new(),
            seed_entry_state: SeedEntryState::new(),
            seed_flow_is_add: false,
            menu_scroll: gui::scroll::ScrollState::new(gui::menu::menu_viewport()),
            wallet_rows: HVec::new(),
            wallets_scroll: gui::scroll::ScrollState::new(gui::menu::wallets_viewport()),
            label_entry_state: gui::label::LabelEntryState::new(),
            unlock_demo_state: None,
            unlock_demo_last_frame_start: None,
            idle_message: HString::new(),
            active_target: None,
            splash_started_at: None,
            calibration_step: 0,
        };
        gui.seed_demo_wallets();
        gui.show_splash();
        Ok(gui)
    }

    pub fn tick(&mut self) {
        self.advance_splash();
        self.advance_unlock_animation();
    }

    pub fn open_menu(&mut self) {
        self.show_menu();
    }

    pub fn show_boot(&mut self) {
        self.show_splash();
    }

    pub fn release_touch(&mut self) {
        self.menu_scroll.drag_end();
        self.wallets_scroll.drag_end();
        let active = self.active_target.take();
        if let Some(active) = active {
            self.draw_active_target(active, false);
            self.activate_target(active);
        }
    }

    pub fn handle_touch(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
        let point = Point::new(clamp_coord(x, SCREEN_WIDTH), clamp_coord(y, SCREEN_HEIGHT));
        if self.mode == GuiMode::Splash {
            self.show_unlocked("");
            return Ok(());
        }

        if self.mode == GuiMode::Wallets
            && self.wallets_scroll.contains(point)
            && self.wallets_scroll.drag_to(point.y)
        {
            self.clear_active_target();
            gui::menu::render_wallets_viewport(
                &mut self.display,
                &self.wallet_rows,
                &mut self.wallets_scroll,
            );
            return Ok(());
        }

        if self.mode == GuiMode::Menu
            && self.menu_scroll.contains(point)
            && self.menu_scroll.drag_to(point.y)
        {
            self.clear_active_target();
            gui::menu::render_menu_viewport(&mut self.display, &mut self.menu_scroll);
            return Ok(());
        }

        let target = self.target_from_point(point);
        self.set_active_target(target);
        Ok(())
    }
}

impl SigerGui {
    pub fn show_splash(&mut self) {
        self.stop_unlock_animation();
        self.clear_active_target();
        self.mode = GuiMode::Splash;
        self.splash_started_at = Some(Instant::now());
        gui::render::blit_boot_logo(&mut self.display);
    }

    fn show_unlocked(&mut self, message: &str) {
        self.clear_active_target();
        self.splash_started_at = None;
        self.mode = GuiMode::Unlocked;
        self.unlock_demo_state = Some(gui::demo::AnimationState::new());
        self.unlock_demo_last_frame_start = None;
        self.idle_message.clear();
        let _ = self.idle_message.push_str(message);
        let _ = self.display.clear(palette::background());
        gui::render::draw_unlock_header(&mut self.display, false);
        if !message.is_empty() {
            gui::render::render_idle_overlay(&mut self.display, message);
        }
    }

    fn show_locked(&mut self, header: &str) {
        self.stop_unlock_animation();
        self.clear_active_target();
        self.mode = GuiMode::Locked;
        gui::render::draw_keypad(&mut self.display);
        gui::render::render_header(&mut self.display, header, palette::surface_high());
    }

    fn show_menu(&mut self) {
        self.stop_unlock_animation();
        self.clear_active_target();
        self.menu_scroll.reset();
        self.mode = GuiMode::Menu;
        gui::menu::render_menu(&mut self.display, &mut self.menu_scroll);
    }

    fn show_about(&mut self) {
        self.stop_unlock_animation();
        self.clear_active_target();
        self.mode = GuiMode::About;
        gui::menu::render_about(&mut self.display, &demo_about_info());
    }

    fn show_themes(&mut self) {
        self.stop_unlock_animation();
        self.clear_active_target();
        self.mode = GuiMode::Themes;
        gui::menu::render_themes(&mut self.display);
    }

    fn show_wallets(&mut self) {
        self.stop_unlock_animation();
        self.clear_active_target();
        self.mode = GuiMode::Wallets;
        self.wallets_scroll.reset();
        gui::menu::render_wallets(
            &mut self.display,
            &self.wallet_rows,
            &mut self.wallets_scroll,
        );
    }

    fn show_add_seed(&mut self) {
        self.stop_unlock_animation();
        self.clear_active_target();
        self.seed_entry_state.reset();
        self.seed_flow_is_add = true;
        self.mode = GuiMode::SeedFirstBoot;
        gui::seed::render_seed_setup(&mut self.display, "Add seed", true);
    }

    fn show_seed_entry(&mut self) {
        self.stop_unlock_animation();
        self.clear_active_target();
        self.mode = GuiMode::SeedEntry;
        gui::seed::render_seed_entry(&mut self.display, &self.seed_entry_state);
    }

    fn show_seed_confirm(&mut self) {
        self.stop_unlock_animation();
        self.clear_active_target();
        self.mode = GuiMode::SeedConfirm;
        gui::seed::render_seed_confirm(&mut self.display, &self.seed_entry_state);
    }

    fn show_label_entry(&mut self, slot: u8, current: &str, context: LabelEntryContext) {
        self.stop_unlock_animation();
        self.clear_active_target();
        self.mode = GuiMode::LabelEntry;
        self.label_entry_state.begin(slot, current, context);
        gui::label::render_label_entry(&mut self.display, &self.label_entry_state);
    }

    fn target_from_point(&self, point: Point) -> Option<ActiveTarget> {
        match self.mode {
            GuiMode::Unlocked => {
                if gui::layout::lock_button_rect().contains(point) {
                    Some(ActiveTarget::HeaderLock)
                } else if gui::layout::point_in_header_settings_menu(point) {
                    Some(ActiveTarget::HeaderMenu)
                } else {
                    None
                }
            }
            GuiMode::Locked => {
                gui::layout::button_from_point_keypad(point).map(|hit| ActiveTarget::Button {
                    mode: self.mode,
                    hit,
                })
            }
            GuiMode::Menu => {
                gui::menu::button_from_point_menu(point, &self.menu_scroll).map(|hit| {
                    ActiveTarget::Button {
                        mode: self.mode,
                        hit,
                    }
                })
            }
            GuiMode::About => {
                gui::menu::button_from_point_about(point).map(|hit| ActiveTarget::Button {
                    mode: self.mode,
                    hit,
                })
            }
            GuiMode::Themes => {
                gui::menu::button_from_point_themes(point).map(|hit| ActiveTarget::Button {
                    mode: self.mode,
                    hit,
                })
            }
            GuiMode::Wallets => {
                gui::menu::button_from_point_wallets(point, &self.wallet_rows, &self.wallets_scroll)
                    .map(|hit| ActiveTarget::Button {
                        mode: self.mode,
                        hit,
                    })
            }
            GuiMode::LabelEntry => {
                gui::label::button_from_point_label_entry(point).map(|hit| ActiveTarget::Button {
                    mode: self.mode,
                    hit,
                })
            }
            GuiMode::SeedFirstBoot => {
                gui::seed::button_from_point_seed_setup(point, self.seed_flow_is_add).map(|hit| {
                    ActiveTarget::Button {
                        mode: self.mode,
                        hit,
                    }
                })
            }
            GuiMode::SeedEntry => {
                gui::seed::button_from_point_seed_entry(point).map(|hit| ActiveTarget::Button {
                    mode: self.mode,
                    hit,
                })
            }
            GuiMode::SeedConfirm => {
                gui::seed::button_from_point_seed_confirm(point).map(|hit| ActiveTarget::Button {
                    mode: self.mode,
                    hit,
                })
            }
            GuiMode::TouchCalibration => Some(ActiveTarget::Calibration),
            GuiMode::Diagnostics => Some(ActiveTarget::Diagnostics),
            _ => None,
        }
    }

    fn set_active_target(&mut self, target: Option<ActiveTarget>) {
        if self.active_target == target {
            return;
        }
        if let Some(old) = self.active_target.take() {
            self.draw_active_target(old, false);
        }
        self.active_target = target;
        if let Some(new) = self.active_target {
            self.draw_active_target(new, true);
        }
    }

    fn clear_active_target(&mut self) {
        if let Some(old) = self.active_target.take() {
            self.draw_active_target(old, false);
        }
    }

    fn draw_active_target(&mut self, target: ActiveTarget, active: bool) {
        match target {
            ActiveTarget::HeaderLock => {
                gui::render::draw_unlock_header_with_menu(&mut self.display, active, false);
            }
            ActiveTarget::HeaderMenu => {
                gui::render::draw_unlock_header_with_menu(&mut self.display, false, active);
            }
            ActiveTarget::Calibration => {}
            ActiveTarget::Diagnostics => {}
            ActiveTarget::Button { mode, hit } => match mode {
                GuiMode::Locked => {
                    gui::render::draw_button(&mut self.display, GuiMode::Locked, hit, active)
                }
                GuiMode::Menu => gui::menu::draw_menu_button(&mut self.display, hit, active),
                GuiMode::About => gui::menu::draw_about_button(&mut self.display, active),
                GuiMode::Themes => gui::menu::draw_theme_button(&mut self.display, hit, active),
                GuiMode::Wallets => self.draw_wallet_active_target(hit, active),
                GuiMode::LabelEntry => gui::label::draw_label_button(
                    &mut self.display,
                    hit,
                    &self.label_entry_state,
                    active,
                ),
                GuiMode::SeedFirstBoot | GuiMode::SeedEntry | GuiMode::SeedConfirm => {
                    gui::seed::draw_seed_button(
                        &mut self.display,
                        mode,
                        hit,
                        Some(&self.seed_entry_state),
                        active,
                    );
                }
                _ => {}
            },
        }
    }

    fn draw_wallet_active_target(&mut self, hit: ButtonHit, active: bool) {
        match hit.button {
            Button::Menu(MenuItem::Back) => gui::menu::draw_wallets_back(&mut self.display, active),
            Button::WalletRow(_) if active => {
                let rect = Rectangle::new(hit.top_left, hit.size);
                let _ = rect
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .stroke_color(palette::keypad_active_light())
                            .stroke_width(2)
                            .build(),
                    )
                    .draw(&mut self.display);
            }
            Button::WalletRow(_) => gui::menu::render_wallets_viewport(
                &mut self.display,
                &self.wallet_rows,
                &mut self.wallets_scroll,
            ),
            _ => {}
        }
    }

    fn activate_target(&mut self, target: ActiveTarget) {
        match target {
            ActiveTarget::HeaderLock => {
                self.pin_entered.clear();
                self.show_locked("PIN");
            }
            ActiveTarget::HeaderMenu => self.show_menu(),
            ActiveTarget::Calibration => self.advance_calibration(),
            ActiveTarget::Diagnostics => self.show_menu(),
            ActiveTarget::Button { mode, hit } => {
                let point = center_of_hit(hit);
                match mode {
                    GuiMode::Locked => self.handle_locked(point),
                    GuiMode::Menu => self.handle_menu(point),
                    GuiMode::About => self.handle_about(point),
                    GuiMode::Themes => self.handle_themes(point),
                    GuiMode::Wallets => self.handle_wallets(point),
                    GuiMode::LabelEntry => self.handle_label_entry(point),
                    GuiMode::SeedFirstBoot => self.handle_seed_setup(point),
                    GuiMode::SeedEntry => self.handle_seed_entry(point),
                    GuiMode::SeedConfirm => self.handle_seed_confirm(point),
                    _ => {}
                }
            }
        }
    }

    fn handle_locked(&mut self, point: Point) {
        let Some(hit) = gui::layout::button_from_point_keypad(point) else {
            return;
        };
        match hit.button {
            Button::Digit(digit) if self.pin_entered.len() < MAX_PIN_DIGITS => {
                let _ = self.pin_entered.push(digit);
                self.render_pin_header("PIN");
            }
            Button::Clear => {
                self.pin_entered.clear();
                self.render_pin_header("PIN");
            }
            Button::Ok => {
                if self.pin_entered.len() >= 4 {
                    self.pin_entered.clear();
                    self.show_unlocked("Unlocked");
                } else {
                    self.pin_entered.clear();
                    self.show_locked("PIN too short");
                }
            }
            _ => {}
        }
    }

    fn render_pin_header(&mut self, title: &str) {
        let mut header = HString::<24>::new();
        let _ = header.push_str(title);
        if !self.pin_entered.is_empty() {
            let _ = header.push(' ');
            for _ in 0..self.pin_entered.len() {
                let _ = header.push('*');
            }
        }
        gui::render::render_header(&mut self.display, header.as_str(), palette::surface_high());
    }

    fn handle_menu(&mut self, point: Point) {
        let Some(hit) = gui::menu::button_from_point_menu(point, &self.menu_scroll) else {
            return;
        };
        match hit.button {
            Button::Menu(MenuItem::Wallets) => self.show_wallets(),
            Button::Menu(MenuItem::AddSeed) => self.show_add_seed(),
            Button::Menu(MenuItem::Theme) => self.show_themes(),
            Button::Menu(MenuItem::About) => self.show_about(),
            Button::Menu(MenuItem::Diagnostics) => self.show_diagnostics(),
            Button::Menu(MenuItem::Calibrate) => self.show_calibration_demo(),
            Button::Menu(MenuItem::Back) => self.show_unlocked(""),
            _ => {}
        }
    }

    fn handle_about(&mut self, point: Point) {
        let Some(hit) = gui::menu::button_from_point_about(point) else {
            return;
        };
        if matches!(hit.button, Button::Menu(MenuItem::Back)) {
            self.show_menu();
        }
    }

    fn handle_themes(&mut self, point: Point) {
        let Some(hit) = gui::menu::button_from_point_themes(point) else {
            return;
        };
        match hit.button {
            Button::Theme(theme) => {
                gui::palette::set_theme(theme);
                gui::menu::render_themes(&mut self.display);
            }
            Button::Menu(MenuItem::Back) => self.show_menu(),
            _ => {}
        }
    }

    fn handle_wallets(&mut self, point: Point) {
        if self.wallets_scroll.contains(point) && self.wallets_scroll.drag_to(point.y) {
            gui::menu::render_wallets_viewport(
                &mut self.display,
                &self.wallet_rows,
                &mut self.wallets_scroll,
            );
            return;
        }

        let Some(hit) =
            gui::menu::button_from_point_wallets(point, &self.wallet_rows, &self.wallets_scroll)
        else {
            return;
        };
        match hit.button {
            Button::Menu(MenuItem::Back) => self.show_menu(),
            Button::WalletRow(slot) => {
                let mut current = HString::<MAX_SEED_LABEL_LEN>::new();
                if let Some(row) = self.wallet_rows.iter().find(|row| row.index == slot) {
                    let _ = current.push_str(row.label.as_str());
                }
                self.show_label_entry(slot, current.as_str(), LabelEntryContext::WalletMenu);
            }
            _ => {}
        }
    }

    fn handle_label_entry(&mut self, point: Point) {
        let Some(hit) = gui::label::button_from_point_label_entry(point) else {
            return;
        };
        let Button::Label(button) = hit.button else {
            return;
        };
        match button {
            LabelButton::Key(digit) => {
                if self.label_entry_state.push_key(digit, Instant::now()) {
                    gui::label::render_label_entry(&mut self.display, &self.label_entry_state);
                }
            }
            LabelButton::Backspace => {
                if self.label_entry_state.backspace() {
                    gui::label::render_label_entry(&mut self.display, &self.label_entry_state);
                }
            }
            LabelButton::Save => {
                self.label_entry_state.clear_multitap();
                let slot = self.label_entry_state.slot();
                let label = self.label_entry_state.label().clone();
                let context = self.label_entry_state.context();
                self.save_label(slot, label.as_str());
                match context {
                    LabelEntryContext::WalletMenu | LabelEntryContext::AddedSeed => {
                        self.show_wallets()
                    }
                    LabelEntryContext::FirstSeed => self.show_unlocked("Wallet named"),
                }
            }
            LabelButton::Cancel => {
                self.label_entry_state.clear_multitap();
                match self.label_entry_state.context() {
                    LabelEntryContext::WalletMenu | LabelEntryContext::AddedSeed => {
                        self.show_wallets()
                    }
                    LabelEntryContext::FirstSeed => self.show_unlocked("Wallet ready"),
                }
            }
        }
    }

    fn handle_seed_setup(&mut self, point: Point) {
        let Some(hit) = gui::seed::button_from_point_seed_setup(point, self.seed_flow_is_add)
        else {
            return;
        };
        let Button::Seed(button) = hit.button else {
            return;
        };
        match button {
            SeedButton::GenerateSeed => {
                if self.seed_entry_state.load_generated() {
                    self.show_seed_confirm();
                } else {
                    self.show_add_seed();
                }
            }
            SeedButton::EnterSeed => {
                self.seed_entry_state.reset();
                self.show_seed_entry();
            }
            SeedButton::Cancel => self.show_menu(),
            _ => {}
        }
    }

    fn handle_seed_entry(&mut self, point: Point) {
        let Some(hit) = gui::seed::button_from_point_seed_entry(point) else {
            return;
        };
        let Button::Seed(button) = hit.button else {
            return;
        };
        match button {
            SeedButton::Key(digit) => {
                if self.seed_entry_state.push_digit(digit) {
                    self.show_seed_entry();
                }
            }
            SeedButton::Backspace => {
                let _ = self.seed_entry_state.backspace();
                self.show_seed_entry();
            }
            SeedButton::NextSuggestion => {
                if self.seed_entry_state.next_suggestion() {
                    self.show_seed_entry();
                }
            }
            SeedButton::CommitWord => {
                if self.seed_entry_state.commit_current().is_some() {
                    if self.seed_entry_state.finish().is_some() {
                        self.show_seed_confirm();
                    } else {
                        self.show_seed_entry();
                    }
                }
            }
            SeedButton::Cancel => {
                if self.seed_flow_is_add {
                    self.show_menu();
                } else {
                    self.show_unlocked("Setup cancelled");
                }
            }
            _ => {}
        }
    }

    fn handle_seed_confirm(&mut self, point: Point) {
        let Some(hit) = gui::seed::button_from_point_seed_confirm(point) else {
            return;
        };
        let Button::Seed(button) = hit.button else {
            return;
        };
        match button {
            SeedButton::Finish => {
                if self.seed_entry_state.finish().is_some() {
                    let slot = self.add_demo_wallet();
                    let context = if self.seed_flow_is_add {
                        LabelEntryContext::AddedSeed
                    } else {
                        LabelEntryContext::FirstSeed
                    };
                    self.seed_entry_state.reset();
                    self.seed_flow_is_add = false;
                    self.show_label_entry(slot, "", context);
                }
            }
            SeedButton::Cancel => {
                if self.seed_entry_state.is_generated() {
                    if self.seed_flow_is_add {
                        self.show_add_seed();
                    } else {
                        self.show_unlocked("Setup cancelled");
                    }
                } else {
                    self.show_seed_entry();
                }
            }
            _ => {}
        }
    }

    fn show_diagnostics(&mut self) {
        self.stop_unlock_animation();
        self.clear_active_target();
        self.mode = GuiMode::Diagnostics;
        let _ = self.display.clear(palette::background());
        gui::render::render_header(&mut self.display, "Diagnostics", palette::surface_high());
        draw_left_lines(
            &mut self.display,
            &[
                "browser firmware demo",
                "shared GUI renderer",
                "touch: pointer events",
                "storage: mock seed slots",
            ],
        );
    }

    fn show_calibration_demo(&mut self) {
        self.stop_unlock_animation();
        self.mode = GuiMode::TouchCalibration;
        self.calibration_step = 0;
        self.render_calibration_target();
    }

    fn seed_demo_wallets(&mut self) {
        if !self.wallet_rows.is_empty() {
            return;
        }
        let mut primary = WalletRow {
            index: 0,
            active: true,
            label: HString::new(),
            pkh: HString::new(),
        };
        let _ = primary.label.push_str("Main");
        let _ = primary
            .pkh
            .push_str("nock1q6q4mw8r4mdx7tz9hs0t3ry4lq7avz77r2s2c");

        let mut travel = WalletRow {
            index: 1,
            active: false,
            label: HString::new(),
            pkh: HString::new(),
        };
        let _ = travel.label.push_str("Travel");
        let _ = travel
            .pkh
            .push_str("nock1q0qp9vx5d3kj5qf2d9ye7kkpvd3xm4qq8kqfj");

        let _ = self.wallet_rows.push(primary);
        let _ = self.wallet_rows.push(travel);
    }

    fn add_demo_wallet(&mut self) -> u8 {
        let slot = next_slot(&self.wallet_rows);
        let mut row = WalletRow {
            index: slot,
            active: false,
            label: HString::new(),
            pkh: HString::new(),
        };
        let _ = write!(row.pkh, "nock1qdemoslot{:02}p7w5jz7v4m90tksz6a0f", slot);
        if self.wallet_rows.len() < MAX_SEED_SLOTS {
            let _ = self.wallet_rows.push(row);
        }
        slot
    }

    fn save_label(&mut self, slot: u8, label: &str) {
        if let Some(row) = self.wallet_rows.iter_mut().find(|row| row.index == slot) {
            row.label.clear();
            let take = label.len().min(MAX_SEED_LABEL_LEN);
            let _ = row.label.push_str(&label[..take]);
        }
    }

    fn advance_unlock_animation(&mut self) {
        if self.mode != GuiMode::Unlocked {
            return;
        }

        let Some(state) = self.unlock_demo_state.as_mut() else {
            return;
        };

        if state.is_frame_start() {
            let now = Instant::now();
            if let Some(last) = self.unlock_demo_last_frame_start {
                if now - last < Duration::from_millis(33) {
                    return;
                }
            }
            self.unlock_demo_last_frame_start = Some(now);
        }

        let header_h = gui::layout::header_height().max(0) as u16;
        let bottom = if self.idle_message.is_empty() {
            SCREEN_HEIGHT
        } else {
            let overlay_top = SCREEN_HEIGHT as i32 - IDLE_OVERLAY_MARGIN - IDLE_OVERLAY_HEIGHT;
            overlay_top.clamp(header_h as i32, SCREEN_HEIGHT as i32) as u16
        };
        let clip_range = Some((header_h, bottom));
        loop {
            match gui::demo::render_next_chunk(&mut self.display, state, clip_range) {
                Ok(true) | Err(_) => break,
                Ok(false) => {}
            }
        }

        if !self.idle_message.is_empty() {
            gui::render::render_idle_overlay(&mut self.display, self.idle_message.as_str());
        }
    }

    fn advance_splash(&mut self) {
        if self.mode != GuiMode::Splash {
            return;
        }
        let Some(started) = self.splash_started_at else {
            return;
        };
        if Instant::now() - started >= SPLASH_DURATION {
            self.show_unlocked("");
        }
    }

    fn advance_calibration(&mut self) {
        if self.mode != GuiMode::TouchCalibration {
            return;
        }
        self.calibration_step = self.calibration_step.saturating_add(1);
        if self.calibration_step >= CALIBRATION_POINTS.len() {
            self.calibration_step = 0;
            self.show_menu();
        } else {
            self.render_calibration_target();
        }
    }

    fn render_calibration_target(&mut self) {
        let step = self
            .calibration_step
            .min(CALIBRATION_POINTS.len().saturating_sub(1));
        let _ = self.display.clear(palette::background());
        gui::render::render_header(
            &mut self.display,
            "Calibrate Touch",
            palette::surface_high(),
        );
        gui::render::render_touch_calibration_target(
            &mut self.display,
            step,
            CALIBRATION_POINTS.len(),
            CALIBRATION_POINTS[step],
        );
    }

    fn stop_unlock_animation(&mut self) {
        self.unlock_demo_state = None;
        self.unlock_demo_last_frame_start = None;
        self.idle_message.clear();
    }
}

fn clamp_coord(value: f64, limit: u16) -> i32 {
    let max = f64::from(limit.saturating_sub(1));
    value.round().clamp(0.0, max) as i32
}

fn center_of_hit(hit: ButtonHit) -> Point {
    Point::new(
        hit.top_left.x + hit.size.width as i32 / 2,
        hit.top_left.y + hit.size.height as i32 / 2,
    )
}

fn next_slot(rows: &[WalletRow]) -> u8 {
    for slot in 0..MAX_SEED_SLOTS as u8 {
        if rows.iter().all(|row| row.index != slot) {
            return slot;
        }
    }
    rows.len().saturating_sub(1) as u8
}

fn draw_left_lines(display: &mut WasmDisplay, lines: &[&str]) {
    let style = MonoTextStyle::new(&FONT_6X10, palette::text());
    let mut y = gui::layout::header_height() + 20;
    for line in lines {
        let _ = Text::new(line, Point::new(8, y), style).draw(display);
        y += 18;
    }
}
