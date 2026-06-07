pub mod time;

#[path = "../../nockster-fw/src/bin/gui/constants.rs"]
pub mod constants;
#[path = "../../nockster-fw/src/bin/gui/demo.rs"]
pub mod demo;
#[path = "../../nockster-fw/src/bin/gui/label.rs"]
pub mod label;
#[path = "../../nockster-fw/src/bin/gui/layout.rs"]
pub mod layout;
#[path = "../../nockster-fw/src/bin/gui/menu.rs"]
pub mod menu;
#[path = "../../nockster-fw/src/bin/gui/palette.rs"]
pub mod palette;
#[path = "../../nockster-fw/src/bin/gui/render.rs"]
pub mod render;
#[path = "../../nockster-fw/src/bin/gui/scroll.rs"]
pub mod scroll;
#[path = "../../nockster-fw/src/bin/gui/seed.rs"]
pub mod seed;
#[path = "../../nockster-fw/src/bin/gui/state.rs"]
pub mod state;
#[path = "../../nockster-fw/src/bin/gui/touch.rs"]
pub mod touch;

pub type GuiDisplay<'d> = crate::display::WasmDisplay;
pub use touch::ScreenPoint;
