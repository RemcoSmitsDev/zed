use crate::debugger_panel_item::{DebugPanelItem, DebugPanelItemEvent};
use gpui::{list, ListState, Model};
use ui::prelude::*;

pub struct VariableList {
    pub list: ListState,
}

impl VariableList {
    pub fn new(debug_panel_item: Model<DebugPanelItem>, cx: &mut ViewContext<Self>) -> Self {
        let list = ListState::new(0, gpui::ListAlignment::Top, px(1000.), move |ix, cx| {
            div().into_any_element()
        });

        cx.subscribe(&debug_panel_item, Self::handle_events);

        Self { list }
    }

    pub fn handle_events(
        &mut self,
        _debug_panel_item: Model<DebugPanelItem>,
        _event: &DebugPanelItemEvent,
        _cx: &mut ViewContext<Self>,
    ) {
    }
}

impl Render for VariableList {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        list(self.list.clone()).gap_1_5().size_full()
    }
}
