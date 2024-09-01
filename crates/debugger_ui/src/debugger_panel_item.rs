use crate::console::Console;
use crate::debugger_panel::{DebugPanel, DebugPanelEvent};
use crate::variable_list::VariableList;

use anyhow::Result;
use dap::client::{DebugAdapterClient, DebugAdapterClientId, ThreadState, ThreadStatus};
use dap::debugger_settings::DebuggerSettings;
use dap::{OutputEvent, OutputEventCategory, StackFrame, StoppedEvent, ThreadEvent};
use editor::Editor;
use gpui::{
    impl_actions, list, AnyElement, AppContext, AsyncWindowContext, EventEmitter, FocusHandle,
    FocusableView, ListState, Subscription, View, WeakView,
};
use serde::Deserialize;
use settings::Settings;
use std::sync::Arc;
use ui::WindowContext;
use ui::{prelude::*, Tooltip};
use workspace::dock::Panel;
use workspace::item::{Item, ItemEvent};
use workspace::Workspace;

pub enum Event {
    Close,
}

#[derive(PartialEq, Eq)]
enum ThreadItem {
    Variables,
    Console,
    Output,
}

pub struct DebugPanelItem {
    thread_id: u64,
    variable_list: View<VariableList>,
    console: View<Console>,
    focus_handle: FocusHandle,
    stack_frame_list: ListState,
    output_editor: View<Editor>,
    active_thread_item: ThreadItem,
    client: Arc<DebugAdapterClient>,
    _subscriptions: Vec<Subscription>,
    workspace: WeakView<Workspace>,
}

impl_actions!(debug_panel_item, [DebugItemAction]);

/// This struct is for actions that should be triggered even when
/// the debug pane is not in focus. This is done by setting workspace
/// as the action listener then having workspace call `handle_workspace_action`
#[derive(Clone, Deserialize, PartialEq, Default)]
pub struct DebugItemAction {
    kind: DebugPanelItemActionKind,
}

/// Actions that can be sent to workspace
/// currently all of these are button toggles
#[derive(Deserialize, PartialEq, Default, Clone, Debug)]
enum DebugPanelItemActionKind {
    #[default]
    Continue,
    StepOver,
    StepIn,
    StepOut,
    Restart,
    Pause,
    Stop,
    Disconnect,
}

impl DebugPanelItem {
    pub fn new(
        debug_panel: View<DebugPanel>,
        workspace: WeakView<Workspace>,
        client: Arc<DebugAdapterClient>,
        thread_id: u64,
        cx: &mut ViewContext<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let model = cx.model().clone();
        let variable_list = cx.new_view(|cx| VariableList::new(model, cx));
        let console = cx.new_view(|cx| Console::new(cx));

        let weakview = cx.view().downgrade();
        let stack_frame_list =
            ListState::new(0, gpui::ListAlignment::Top, px(1000.), move |ix, cx| {
                if let Some(view) = weakview.upgrade() {
                    view.update(cx, |view, cx| {
                        view.render_stack_frame(ix, cx).into_any_element()
                    })
                } else {
                    div().into_any()
                }
            });

        let _subscriptions = vec![cx.subscribe(&debug_panel, {
            move |this: &mut Self, _, event: &DebugPanelEvent, cx| {
                match event {
                    DebugPanelEvent::Stopped((client_id, event)) => {
                        Self::handle_stopped_event(this, client_id, event, cx)
                    }
                    DebugPanelEvent::Thread((client_id, event)) => {
                        Self::handle_thread_event(this, client_id, event, cx)
                    }
                    DebugPanelEvent::Output((client_id, event)) => {
                        Self::handle_output_event(this, client_id, event, cx)
                    }
                    DebugPanelEvent::ClientStopped(client_id) => {
                        Self::handle_client_stopped_event(this, client_id, cx)
                    }
                };
            }
        })];

        let output_editor = cx.new_view(|cx| {
            let mut editor = Editor::multi_line(cx);
            editor.set_placeholder_text("Debug adapter and script output", cx);
            editor.set_read_only(true);
            editor.set_show_inline_completions(Some(false), cx);
            editor.set_searchable(false);
            editor.set_auto_replace_emoji_shortcode(false);
            editor.set_show_indent_guides(false, cx);
            editor.set_autoindent(false);
            editor.set_show_gutter(false, cx);
            editor.set_show_line_numbers(false, cx);
            editor
        });

        Self {
            client,
            thread_id,
            workspace,
            focus_handle,
            variable_list,
            console,
            output_editor,
            _subscriptions,
            stack_frame_list,
            active_thread_item: ThreadItem::Variables,
        }
    }

    fn should_skip_event(
        this: &mut Self,
        client_id: &DebugAdapterClientId,
        thread_id: u64,
    ) -> bool {
        thread_id != this.thread_id || *client_id != this.client.id()
    }

    fn handle_stopped_event(
        this: &mut Self,
        client_id: &DebugAdapterClientId,
        event: &StoppedEvent,
        cx: &mut ViewContext<Self>,
    ) {
        if Self::should_skip_event(this, client_id, event.thread_id.unwrap_or_default()) {
            return;
        }

        let thread_state = this.current_thread_state();

        this.stack_frame_list.reset(thread_state.stack_frames.len());
        if let Some(stack_frame) = thread_state.stack_frames.first() {
            this.update_stack_frame_id(stack_frame.id, cx);
        };

        cx.notify();
    }

    fn handle_thread_event(
        this: &mut Self,
        client_id: &DebugAdapterClientId,
        event: &ThreadEvent,
        _: &mut ViewContext<Self>,
    ) {
        if Self::should_skip_event(this, client_id, event.thread_id) {
            return;
        }

        // TODO: handle thread event
    }

    fn handle_output_event(
        this: &mut Self,
        client_id: &DebugAdapterClientId,
        event: &OutputEvent,
        cx: &mut ViewContext<Self>,
    ) {
        if Self::should_skip_event(this, client_id, this.thread_id) {
            return;
        }

        // The default value of an event category is console
        // so we assume that is the output type if it doesn't exist
        let output_category = event
            .category
            .as_ref()
            .unwrap_or(&OutputEventCategory::Console);

        match output_category {
            OutputEventCategory::Console => {
                this.console.update(cx, |console, cx| {
                    console.add_message(&event.output, cx);
                });
            }
            // OutputEventCategory::Stderr => {}
            OutputEventCategory::Stdout => {
                this.output_editor.update(cx, |editor, cx| {
                    editor.set_read_only(false);
                    editor.move_to_end(&editor::actions::MoveToEnd, cx);
                    editor.insert(format!("{}\n", &event.output.trim_end()).as_str(), cx);
                    editor.set_read_only(true);

                    cx.notify();
                });
            }
            // OutputEventCategory::Unknown => {}
            // OutputEventCategory::Important => {}
            OutputEventCategory::Telemetry => {}
            _ => {
                this.output_editor.update(cx, |editor, cx| {
                    editor.set_read_only(false);
                    editor.move_to_end(&editor::actions::MoveToEnd, cx);
                    editor.insert(format!("{}\n", &event.output.trim_end()).as_str(), cx);
                    editor.set_read_only(true);

                    cx.notify();
                });
            }
        }
    }

    fn handle_client_stopped_event(
        this: &mut Self,
        client_id: &DebugAdapterClientId,
        cx: &mut ViewContext<Self>,
    ) {
        if Self::should_skip_event(this, client_id, this.thread_id) {
            return;
        }

        cx.emit(Event::Close);
    }

    pub fn client(&self) -> Arc<DebugAdapterClient> {
        self.client.clone()
    }

    pub fn thread_id(&self) -> u64 {
        self.thread_id
    }

    fn stack_frame_for_index(&self, ix: usize) -> StackFrame {
        self.client
            .thread_state_by_id(self.thread_id)
            .stack_frames
            .get(ix)
            .cloned()
            .unwrap()
    }

    pub fn current_thread_state(&self) -> ThreadState {
        self.client
            .thread_states()
            .get(&self.thread_id)
            .cloned()
            .unwrap()
    }

    fn update_stack_frame_id(&mut self, stack_frame_id: u64, cx: &mut ViewContext<Self>) {
        self.client
            .update_current_stack_frame(self.thread_id, stack_frame_id);

        let thread_state = self.current_thread_state();

        self.variable_list.update(cx, |variable_list, cx| {
            variable_list.build_entries(thread_state, true);
            cx.notify();
        });
    }

    fn render_stack_frames(&self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .size_full()
            .child(list(self.stack_frame_list.clone()).size_full())
            .into_any()
    }

    fn render_stack_frame(&self, ix: usize, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let stack_frame = self.stack_frame_for_index(ix);

        let source = stack_frame.source.clone();
        let is_selected_frame =
            stack_frame.id == self.current_thread_state().current_stack_frame_id;

        let formatted_path = format!(
            "{}:{}",
            source.clone().and_then(|s| s.name).unwrap_or_default(),
            stack_frame.line,
        );

        v_flex()
            .rounded_md()
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
                let stack_frame = stack_frame.clone();
                move |this, _, cx| {
                    this.update_stack_frame_id(stack_frame_id, cx);

                    let workspace = this.workspace.clone();
                    let stack_frame = stack_frame.clone();
                    cx.spawn(|_, cx| async move {
                        DebugPanel::go_to_stack_frame(workspace, stack_frame, true, cx).await
                    })
                    .detach_and_log_err(cx);

                    cx.notify();
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

    // if the debug adapter does not send the continued event,
    // and the status of the thread did not change we have to assume the thread is running
    // so we have to update the thread state status to running
    fn update_thread_state(
        this: WeakView<Self>,
        previous_status: ThreadStatus,
        all_threads_continued: Option<bool>,
        mut cx: AsyncWindowContext,
    ) -> Result<()> {
        this.update(&mut cx, |this, cx| {
            if previous_status == this.current_thread_state().status {
                if all_threads_continued.unwrap_or(false) {
                    for thread in this.client.thread_states().values_mut() {
                        thread.status = ThreadStatus::Running;
                    }
                } else {
                    this.client
                        .update_thread_state_status(this.thread_id, ThreadStatus::Running);
                }

                cx.notify();
            }
        })
    }

    /// Actions that should be handled even when Debug Panel is not in focus
    pub fn workspace_action_handler(
        workspace: &mut Workspace,
        action: &DebugItemAction,
        cx: &mut ViewContext<Workspace>,
    ) {
        let Some(pane) = workspace
            .panel::<DebugPanel>(cx)
            .and_then(|panel| panel.read(cx).pane())
        else {
            log::error!(
                    "Can't get Debug panel to handle Debug action: {:?}
                    This shouldn't happen because there has to be an Debug panel to click a button and trigger this action",
                    action.kind
                );
            return;
        };

        pane.update(cx, |this, cx| {
            let Some(active_item) = this
                .active_item()
                .and_then(|item| item.downcast::<DebugPanelItem>())
            else {
                return;
            };

            active_item.update(cx, |item, cx| match action.kind {
                DebugPanelItemActionKind::Stop => item.handle_stop_action(cx),
                DebugPanelItemActionKind::Continue => item.handle_continue_action(cx),
                DebugPanelItemActionKind::StepIn => item.handle_step_in_action(cx),
                DebugPanelItemActionKind::StepOut => item.handle_step_out_action(cx),
                DebugPanelItemActionKind::StepOver => item.handle_step_over_action(cx),
                DebugPanelItemActionKind::Pause => item.handle_pause_action(cx),
                DebugPanelItemActionKind::Disconnect => item.handle_disconnect_action(cx),
                DebugPanelItemActionKind::Restart => item.handle_restart_action(cx),
            });
        });
    }

    fn handle_continue_action(&mut self, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        let thread_id = self.thread_id;
        let previous_status = self.current_thread_state().status;

        cx.spawn(|this, cx| async move {
            let response = client.resume(thread_id).await?;

            Self::update_thread_state(this, previous_status, response.all_threads_continued, cx)
        })
        .detach_and_log_err(cx);
    }

    fn handle_step_over_action(&mut self, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        let thread_id = self.thread_id;
        let previous_status = self.current_thread_state().status;
        let granularity = DebuggerSettings::get_global(cx).stepping_granularity();

        cx.spawn(|this, cx| async move {
            client.step_over(thread_id, granularity).await?;

            Self::update_thread_state(this, previous_status, None, cx)
        })
        .detach_and_log_err(cx);
    }

    fn handle_step_in_action(&mut self, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        let thread_id = self.thread_id;
        let previous_status = self.current_thread_state().status;
        let granularity = DebuggerSettings::get_global(cx).stepping_granularity();

        cx.spawn(|this, cx| async move {
            client.step_in(thread_id, granularity).await?;

            Self::update_thread_state(this, previous_status, None, cx)
        })
        .detach_and_log_err(cx);
    }

    fn handle_step_out_action(&mut self, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        let thread_id = self.thread_id;
        let previous_status = self.current_thread_state().status;
        let granularity = DebuggerSettings::get_global(cx).stepping_granularity();

        cx.spawn(|this, cx| async move {
            client.step_out(thread_id, granularity).await?;

            Self::update_thread_state(this, previous_status, None, cx)
        })
        .detach_and_log_err(cx);
    }

    fn handle_restart_action(&mut self, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();

        cx.background_executor()
            .spawn(async move { client.restart().await })
            .detach_and_log_err(cx);
    }

    fn handle_pause_action(&mut self, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        let thread_id = self.thread_id;
        cx.background_executor()
            .spawn(async move { client.pause(thread_id).await })
            .detach_and_log_err(cx);
    }

    fn handle_stop_action(&mut self, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        let thread_ids = vec![self.thread_id; 1];

        cx.background_executor()
            .spawn(async move { client.terminate_threads(Some(thread_ids)).await })
            .detach_and_log_err(cx);
    }

    fn handle_disconnect_action(&mut self, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        cx.background_executor()
            .spawn(async move { client.disconnect(None, Some(true), None).await })
            .detach_and_log_err(cx);
    }
}

impl EventEmitter<Event> for DebugPanelItem {}

impl FocusableView for DebugPanelItem {
    fn focus_handle(&self, _: &AppContext) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for DebugPanelItem {
    type Event = Event;

    fn tab_content(
        &self,
        params: workspace::item::TabContentParams,
        _: &WindowContext,
    ) -> AnyElement {
        Label::new(format!(
            "{} - Thread {}",
            self.client.config().id,
            self.thread_id
        ))
        .color(if params.selected {
            Color::Default
        } else {
            Color::Muted
        })
        .into_any_element()
    }

    fn tab_tooltip_text(&self, _: &AppContext) -> Option<SharedString> {
        Some(SharedString::from(format!(
            "{} Thread {} - {:?}",
            self.client.config().id,
            self.thread_id,
            self.current_thread_state().status
        )))
    }

    fn to_item_events(event: &Self::Event, mut f: impl FnMut(ItemEvent)) {
        match event {
            Event::Close => f(ItemEvent::CloseItem),
        }
    }
}

impl Render for DebugPanelItem {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let thread_status = self.current_thread_state().status;
        let active_thread_item = &self.active_thread_item;

        h_flex()
            .key_context("DebugPanelItem")
            .track_focus(&self.focus_handle)
            .p_2()
            .size_full()
            .items_start()
            .child(
                v_flex()
                    .size_full()
                    .items_start()
                    .child(
                        h_flex()
                            .py_1()
                            .gap_2()
                            .map(|this| {
                                if thread_status == ThreadStatus::Running {
                                    this.child(
                                        IconButton::new("debug-pause", IconName::DebugPause)
                                            .on_click(cx.listener(|_, _, cx| {
                                                cx.dispatch_action(Box::new(DebugItemAction {
                                                    kind: DebugPanelItemActionKind::Pause,
                                                }))
                                            }))
                                            .tooltip(move |cx| Tooltip::text("Pause program", cx)),
                                    )
                                } else {
                                    this.child(
                                        IconButton::new("debug-continue", IconName::DebugContinue)
                                            .on_click(cx.listener(|_, _, cx| {
                                                cx.dispatch_action(Box::new(DebugItemAction {
                                                    kind: DebugPanelItemActionKind::Continue,
                                                }))
                                            }))
                                            .disabled(thread_status != ThreadStatus::Stopped)
                                            .tooltip(move |cx| {
                                                Tooltip::text("Continue program", cx)
                                            }),
                                    )
                                }
                            })
                            .child(
                                IconButton::new("debug-step-over", IconName::DebugStepOver)
                                    .on_click(cx.listener(|_, _, cx| {
                                        cx.dispatch_action(Box::new(DebugItemAction {
                                            kind: DebugPanelItemActionKind::StepOver,
                                        }))
                                    }))
                                    .disabled(thread_status != ThreadStatus::Stopped)
                                    .tooltip(move |cx| Tooltip::text("Step over", cx)),
                            )
                            .child(
                                IconButton::new("debug-step-in", IconName::DebugStepInto)
                                    .on_click(cx.listener(|_, _, cx| {
                                        cx.dispatch_action(Box::new(DebugItemAction {
                                            kind: DebugPanelItemActionKind::StepIn,
                                        }))
                                    }))
                                    .disabled(thread_status != ThreadStatus::Stopped)
                                    .tooltip(move |cx| Tooltip::text("Step in", cx)),
                            )
                            .child(
                                IconButton::new("debug-step-out", IconName::DebugStepOut)
                                    .on_click(cx.listener(|_, _, cx| {
                                        cx.dispatch_action(Box::new(DebugItemAction {
                                            kind: DebugPanelItemActionKind::StepOut,
                                        }))
                                    }))
                                    .disabled(thread_status != ThreadStatus::Stopped)
                                    .tooltip(move |cx| Tooltip::text("Step out", cx)),
                            )
                            .child(
                                IconButton::new("debug-restart", IconName::DebugRestart)
                                    .on_click(cx.listener(|_, _, cx| {
                                        cx.dispatch_action(Box::new(DebugItemAction {
                                            kind: DebugPanelItemActionKind::Restart,
                                        }))
                                    }))
                                    .disabled(
                                        !self
                                            .client
                                            .capabilities()
                                            .supports_restart_request
                                            .unwrap_or_default()
                                            || thread_status != ThreadStatus::Stopped
                                                && thread_status != ThreadStatus::Running,
                                    )
                                    .tooltip(move |cx| Tooltip::text("Restart", cx)),
                            )
                            .child(
                                IconButton::new("debug-stop", IconName::DebugStop)
                                    .on_click(cx.listener(|_, _, cx| {
                                        cx.dispatch_action(Box::new(DebugItemAction {
                                            kind: DebugPanelItemActionKind::Stop,
                                        }))
                                    }))
                                    .disabled(
                                        thread_status != ThreadStatus::Stopped
                                            && thread_status != ThreadStatus::Running,
                                    )
                                    .tooltip(move |cx| Tooltip::text("Stop", cx)),
                            )
                            .child(
                                IconButton::new("debug-disconnect", IconName::DebugDisconnect)
                                    .on_click(cx.listener(|_, _, cx| {
                                        cx.dispatch_action(Box::new(DebugItemAction {
                                            kind: DebugPanelItemActionKind::Disconnect,
                                        }))
                                    }))
                                    .disabled(
                                        thread_status == ThreadStatus::Exited
                                            || thread_status == ThreadStatus::Ended,
                                    )
                                    .tooltip(move |cx| Tooltip::text("Disconnect", cx)),
                            ),
                    )
                    .child(
                        h_flex()
                            .size_full()
                            .items_start()
                            .p_1()
                            .gap_4()
                            .child(self.render_stack_frames(cx)),
                    ),
            )
            .child(
                v_flex()
                    .size_full()
                    .items_start()
                    .child(
                        h_flex()
                            .child(
                                div()
                                    .id("variables")
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .border_b_2()
                                    .when(*active_thread_item == ThreadItem::Variables, |this| {
                                        this.border_color(cx.theme().colors().border)
                                    })
                                    .child(Label::new("Variables"))
                                    .on_click(cx.listener(|this, _, _| {
                                        this.active_thread_item = ThreadItem::Variables;
                                    })),
                            )
                            .child(
                                div()
                                    .id("console")
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .border_b_2()
                                    .when(*active_thread_item == ThreadItem::Console, |this| {
                                        this.border_color(cx.theme().colors().border)
                                    })
                                    .child(Label::new("Console"))
                                    .on_click(cx.listener(|this, _, _| {
                                        this.active_thread_item = ThreadItem::Console;
                                    })),
                            )
                            .child(
                                div()
                                    .id("output")
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .border_b_2()
                                    .when(*active_thread_item == ThreadItem::Output, |this| {
                                        this.border_color(cx.theme().colors().border)
                                    })
                                    .child(Label::new("Output"))
                                    .on_click(cx.listener(|this, _, _| {
                                        this.active_thread_item = ThreadItem::Output;
                                    })),
                            ),
                    )
                    .when(*active_thread_item == ThreadItem::Variables, |this| {
                        this.size_full().child(self.variable_list.clone())
                    })
                    .when(*active_thread_item == ThreadItem::Output, |this| {
                        this.child(self.output_editor.clone())
                    })
                    .when(*active_thread_item == ThreadItem::Console, |this| {
                        this.child(self.console.clone())
                    }),
            )
            .into_any()
    }
}