use anyhow::Result;
use dap::client::DebugAdapterClientId;
use dap::StackFrame;
use gpui::{list, AnyElement, EventEmitter, FocusHandle, ListState, Subscription, Task, View};
use gpui::{FocusableView, Model};
use project::dap_store::DapStore;
use ui::ViewContext;
use ui::{prelude::*, Tooltip};

use crate::debugger_panel_item::Event::Stopped;
use crate::debugger_panel_item::{self, DebugPanelItem};

#[derive(Debug)]
pub enum Event {
    ChangedStackFrame(u64),
}

pub struct StackFrameList {
    thread_id: u64,
    list: ListState,
    focus_handle: FocusHandle,
    dap_store: Model<DapStore>,
    current_stack_frame_id: u64,
    stack_frames: Vec<StackFrame>,
    client_id: DebugAdapterClientId,
    _subscriptions: Vec<Subscription>,
}

impl StackFrameList {
    pub fn new(
        debug_panel_item: &View<DebugPanelItem>,
        dap_store: &Model<DapStore>,
        client_id: &DebugAdapterClientId,
        thread_id: u64,
        cx: &mut ViewContext<Self>,
    ) -> Self {
        let weakview = cx.view().downgrade();
        let focus_handle = cx.focus_handle();

        let list = ListState::new(0, gpui::ListAlignment::Top, px(1000.), move |ix, cx| {
            weakview
                .upgrade()
                .map(|view| view.update(cx, |this, cx| this.render_entry(ix, cx)))
                .unwrap_or(div().into_any())
        });

        let _subscriptions =
            vec![cx.subscribe(debug_panel_item, Self::handle_debug_panel_item_event)];

        Self {
            list,
            thread_id,
            focus_handle,
            _subscriptions,
            client_id: *client_id,
            dap_store: dap_store.clone(),
            stack_frames: Default::default(),
            current_stack_frame_id: Default::default(),
        }
    }

    fn handle_debug_panel_item_event(
        &mut self,
        _: View<DebugPanelItem>,
        event: &debugger_panel_item::Event,
        cx: &mut ViewContext<Self>,
    ) {
        match event {
            Stopped { go_to_stack_frame } => {
                self.fetch_stack_frames(cx).detach_and_log_err(cx);
            }
            _ => {}
        }
    }

    fn fetch_stack_frames(&self, cx: &mut ViewContext<Self>) -> Task<Result<()>> {
        let task = self.dap_store.update(cx, |store, cx| {
            store.stack_frames(&self.client_id, self.thread_id, cx)
        });

        cx.spawn(|this, mut cx| async move {
            let mut stack_frames = task.await?;

            this.update(&mut cx, |this, cx| {
                std::mem::swap(&mut this.stack_frames, &mut stack_frames);

                if let Some(stack_frame) = this.stack_frames.first() {
                    this.current_stack_frame_id = stack_frame.id;
                    cx.emit(Event::ChangedStackFrame(stack_frame.id));
                }

                this.list.reset(this.stack_frames.len());
                cx.notify();
            })
        })
    }

    fn render_entry(&self, ix: usize, cx: &mut ViewContext<Self>) -> AnyElement {
        let stack_frame = &self.stack_frames[ix];

        let source = stack_frame.source.clone();
        let is_selected_frame = stack_frame.id == self.current_stack_frame_id;

        let formatted_path = format!(
            "{}:{}",
            source.clone().and_then(|s| s.name).unwrap_or_default(),
            stack_frame.line,
        );

        v_flex()
            .rounded_md()
            .w_full()
            .group("")
            .id(("stack-frame", stack_frame.id))
            .tooltip({
                let formatted_path = formatted_path.clone();
                move |cx| Tooltip::text(formatted_path.clone(), cx)
            })
            .p_1()
            .when(is_selected_frame, |this| {
                this.bg(cx.theme().colors().element_hover)
            })
            .on_click(cx.listener({
                let stack_frame_id = stack_frame.id;
                move |this, _, cx| {
                    this.current_stack_frame_id = stack_frame_id;

                    cx.notify();

                    cx.emit(Event::ChangedStackFrame(stack_frame_id));
                }
            }))
            .hover(|s| s.bg(cx.theme().colors().element_hover).cursor_pointer())
            .child(
                h_flex()
                    .gap_0p5()
                    .text_ui_sm(cx)
                    .child(stack_frame.name.clone())
                    .child(formatted_path),
            )
            .child(
                h_flex()
                    .text_ui_xs(cx)
                    .text_color(cx.theme().colors().text_muted)
                    .when_some(source.and_then(|s| s.path), |this, path| this.child(path)),
            )
            .into_any()
    }
}

impl Render for StackFrameList {
    fn render(&mut self, _: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p_1()
            .child(list(self.list.clone()).size_full())
    }
}

impl FocusableView for StackFrameList {
    fn focus_handle(&self, _: &gpui::AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<Event> for StackFrameList {}
