mod dap_log;
pub use dap_log::*;

use gpui::{App, AppContext};

pub fn init(cx: &mut App) {
    dap_log::init(cx);
}
