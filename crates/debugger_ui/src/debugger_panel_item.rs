use crate::debugger_panel::{DebugPanel, DebugPanelEvent};
use anyhow::Result;
use dap::client::{
    DebugAdapterClient, DebugAdapterClientId, ThreadEntry, ThreadState, ThreadStatus,
};
use dap::{
    OutputEvent, OutputEventCategory, Scope, StackFrame, StoppedEvent, ThreadEvent, Variable,
};
use editor::Editor;
use gpui::{
    actions, list, AnyElement, AppContext, AsyncWindowContext, EventEmitter, FocusHandle,
    FocusableView, ListState, Subscription, View, WeakView,
};
use std::sync::Arc;
use ui::{prelude::*, Tooltip};
use ui::{ListItem, WindowContext};
use workspace::item::{Item, ItemEvent};

#[derive(PartialEq, Eq)]
enum ThreadItem {
    Variables,
    Console,
    Output,
}

pub struct DebugPanelItem {
    thread_id: u64,
    variable_list: ListState,
    focus_handle: FocusHandle,
    stack_frame_list: ListState,
    output_editor: View<Editor>,
    collapsed_variables: Vec<SharedString>,
    active_thread_item: ThreadItem,
    client: Arc<DebugAdapterClient>,
    _subscriptions: Vec<Subscription>,
}

actions!(
    debug_panel_item,
    [Continue, StepOver, StepIn, StepOut, Restart, Pause, Stop, Disconnect]
);

impl DebugPanelItem {
    pub fn new(
        debug_panel: View<DebugPanel>,
        client: Arc<DebugAdapterClient>,
        thread_id: u64,
        cx: &mut ViewContext<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let weakview = cx.view().downgrade();
        let variable_list =
            ListState::new(0, gpui::ListAlignment::Top, px(1000.), move |ix, cx| {
                if let Some(view) = weakview.upgrade() {
                    view.update(cx, |view, cx| view.render_variable_list_entry(ix, cx))
                } else {
                    div().into_any()
                }
            });

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
                };
            }
        })];

        let output_editor = cx.new_view(|cx| {
            let mut editor = Editor::multi_line(cx);
            editor.set_placeholder_text("Debug adapter and script output", cx);
            editor.set_read_only(true);
            editor.set_show_inline_completions(false);
            editor.set_searchable(true);
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
            focus_handle,
            variable_list,
            output_editor,
            _subscriptions,
            stack_frame_list,
            collapsed_variables: Default::default(),
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
        if let Some(stack_frame_id) = thread_state.stack_frames.first().map(|s| s.id) {
            this.update_stack_frame_id(stack_frame_id);
            this.build_variable_list_entries(stack_frame_id);
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

        if event
            .category
            .as_ref()
            .map(|c| *c == OutputEventCategory::Telemetry)
            .unwrap_or(false)
        {
            return;
        }

        this.output_editor.update(cx, |editor, cx| {
            editor.set_read_only(false);
            editor.move_to_end(&editor::actions::MoveToEnd, cx);
            editor.insert(format!("{}\n", &event.output.trim_end()).as_str(), cx);
            editor.set_read_only(true);

            cx.notify();
        });
    }
}

impl EventEmitter<ItemEvent> for DebugPanelItem {}

impl FocusableView for DebugPanelItem {
    fn focus_handle(&self, _: &AppContext) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for DebugPanelItem {
    type Event = ItemEvent;

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
}

impl DebugPanelItem {
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

    fn current_thread_state(&self) -> ThreadState {
        self.client
            .thread_states()
            .get(&self.thread_id)
            .cloned()
            .unwrap()
    }

    fn update_stack_frame_id(&self, stack_frame_id: u64) {
        let mut thread_state = self.client.thread_states();
        let Some(thread_state) = thread_state.get_mut(&self.thread_id) else {
            return;
        };

        thread_state.current_stack_frame_id = stack_frame_id;
    }

    pub fn render_variable_list_entry(
        &mut self,
        ix: usize,
        cx: &mut ViewContext<Self>,
    ) -> AnyElement {
        let thread_state = self.current_thread_state();
        let Some(entries) = thread_state
            .stack_frame_entries
            .get(&thread_state.current_stack_frame_id)
        else {
            return div().into_any_element();
        };

        match &entries[ix] {
            ThreadEntry::Scope(scope) => self.render_scope(scope, cx),
            ThreadEntry::Variable {
                depth,
                variable,
                has_children,
                ..
            } => self.render_variable(variable.clone(), *depth, *has_children, cx),
        }
    }

    fn render_scope(&self, scope: &Scope, cx: &mut ViewContext<Self>) -> AnyElement {
        let element_id = scope.variables_reference;

        let scope_id = SharedString::from(format!("scope-{}", element_id));
        let disclosed = self
            .collapsed_variables
            .binary_search(&scope_id.clone())
            .is_err();

        div()
            .id(element_id as usize)
            .group("")
            .flex()
            .w_full()
            .h_full()
            .child(
                ListItem::new(element_id as usize)
                    .indent_level(1)
                    .toggle(disclosed)
                    .on_toggle(
                        cx.listener(move |this, _, cx| {
                            this.toggle_variable_collapsed(&scope_id, cx)
                        }),
                    )
                    .child(div().text_ui(cx).h_6().w_full().child(scope.name.clone())),
            )
            .into_any()
    }

    fn render_variable(
        &self,
        variable: Variable,
        depth: usize,
        has_children: bool,
        cx: &mut ViewContext<Self>,
    ) -> AnyElement {
        let variable_id = SharedString::from(format!("variable-{}", variable.name));
        let disclosed = has_children.then(|| {
            self.collapsed_variables
                .binary_search(&variable_id)
                .is_err()
        });

        let element_id = SharedString::from(format!("{}-{}", variable.name, variable_id));

        div()
            .id(element_id.clone())
            .group("")
            .h_4()
            .size_full()
            .child(
                ListItem::new(element_id)
                    .indent_level(depth + 1)
                    .indent_step_size(px(20.))
                    .toggle(disclosed)
                    .on_toggle(cx.listener(move |this, _, cx| {
                        this.toggle_variable_collapsed(&variable_id, cx)
                    }))
                    .child(
                        h_flex()
                            .gap_1()
                            .text_ui_sm(cx)
                            .child(variable.name.clone())
                            .child(
                                div()
                                    .text_ui_xs(cx)
                                    .text_color(cx.theme().colors().text_muted)
                                    .child(variable.value.clone()),
                            ),
                    ),
            )
            .into_any()
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
                let stack_frame = stack_frame.clone();
                move |this, _, cx| {
                    this.update_stack_frame_id(stack_frame.id);

                    cx.notify();

                    // let client = this.client();
                    // DebugPanel::go_to_stack_frame(&stack_frame, client, true, cx)
                    //     .detach_and_log_err(cx);

                    // TODO:
                    // this.go_to_stack_frame(&stack_frame, this.client.clone(), false, cx)
                    //     .detach_and_log_err(cx);
                    // cx.notify();
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

    pub fn build_variable_list_entries(&mut self, stack_frame_id: u64) {
        let thread_state = self.current_thread_state();
        let Some(entries) = thread_state.stack_frame_entries.get(&stack_frame_id) else {
            return;
        };

        self.variable_list.reset(entries.len());
    }

    // fn render_scopes(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
    //     let thread_state = self.current_thread_state();
    //     let Some(scopes) = thread_state
    //         .current_stack_frame_id
    //         .and_then(|id| thread_state.scopes.get(&id))
    //     else {
    //         return div().child("No scopes for this thread yet").into_any();
    //     };

    //     div()
    //         .w_3_4()
    //         .gap_3()
    //         .text_ui_sm(cx)
    //         .children(
    //             scopes
    //                 .iter()
    //                 .map(|scope| self.render_scope(&thread_state, scope, cx)),
    //         )
    //         .into_any()
    // }

    // fn render_scope(
    //     &self,
    //     thread_state: &ThreadState,
    //     scope: &Scope,
    //     cx: &mut ViewContext<Self>,
    // ) -> impl IntoElement {
    //     div()
    //         .id(("scope", scope.variables_reference))
    //         .p_1()
    //         .text_ui_sm(cx)
    //         .hover(|s| s.bg(cx.theme().colors().element_hover).cursor_pointer())
    //         .child(scope.name.clone())
    //         .child(
    //             div()
    //                 .ml_2()
    //                 .child(self.render_variables(thread_state, scope, cx)),
    //         )
    //         .into_any()
    // }

    // fn render_variables(
    //     &self,
    //     thread_state: &ThreadState,
    //     scope: &Scope,
    //     cx: &mut ViewContext<Self>,
    // ) -> impl IntoElement {
    //     let Some(variables) = thread_state.variables.get(&scope.variables_reference) else {
    //         return div().child("No variables for this thread yet").into_any();
    //     };

    //     div()
    //         .gap_3()
    //         .text_ui_sm(cx)
    //         .children(
    //             variables
    //                 .iter()
    //                 .map(|variable| self.render_variable(variable, cx)),
    //         )
    //         .into_any()
    // }

    // fn render_variable(&self, variable: &Variable, cx: &mut ViewContext<Self>) -> impl IntoElement {
    //     h_flex()
    //         .id(("variable", variable.variables_reference))
    //         .p_1()
    //         .gap_1()
    //         .text_ui_sm(cx)
    //         .hover(|s| s.bg(cx.theme().colors().element_hover).cursor_pointer())
    //         .child(variable.name.clone())
    //         .child(
    //             div()
    //                 .text_ui_xs(cx)
    //                 .text_color(cx.theme().colors().text_muted)
    //                 .child(variable.value.clone()),
    //         )
    //         .into_any()
    // }

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

    fn handle_continue_action(&mut self, _: &Continue, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        let thread_id = self.thread_id;
        let previous_status = self.current_thread_state().status;

        cx.spawn(|this, cx| async move {
            let response = client.resume(thread_id).await?;

            Self::update_thread_state(this, previous_status, response.all_threads_continued, cx)
        })
        .detach_and_log_err(cx);
    }

    fn handle_step_over_action(&mut self, _: &StepOver, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        let thread_id = self.thread_id;
        let previous_status = self.current_thread_state().status;

        cx.spawn(|this, cx| async move {
            client.step_over(thread_id).await?;

            Self::update_thread_state(this, previous_status, None, cx)
        })
        .detach_and_log_err(cx);
    }

    fn handle_step_in_action(&mut self, _: &StepIn, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        let thread_id = self.thread_id;
        let previous_status = self.current_thread_state().status;

        cx.spawn(|this, cx| async move {
            client.step_in(thread_id).await?;

            Self::update_thread_state(this, previous_status, None, cx)
        })
        .detach_and_log_err(cx);
    }

    fn handle_step_out_action(&mut self, _: &StepOut, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        let thread_id = self.thread_id;
        let previous_status = self.current_thread_state().status;

        cx.spawn(|this, cx| async move {
            client.step_out(thread_id).await?;

            Self::update_thread_state(this, previous_status, None, cx)
        })
        .detach_and_log_err(cx);
    }

    fn handle_restart_action(&mut self, _: &Restart, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();

        cx.background_executor()
            .spawn(async move { client.restart().await })
            .detach_and_log_err(cx);
    }

    fn handle_pause_action(&mut self, _: &Pause, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        let thread_id = self.thread_id;
        cx.background_executor()
            .spawn(async move { client.pause(thread_id).await })
            .detach_and_log_err(cx);
    }

    fn handle_stop_action(&mut self, _: &Stop, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        let thread_ids = vec![self.thread_id; 1];

        cx.background_executor()
            .spawn(async move { client.terminate_threads(Some(thread_ids)).await })
            .detach_and_log_err(cx);
    }

    fn handle_disconnect_action(&mut self, _: &Disconnect, cx: &mut ViewContext<Self>) {
        let client = self.client.clone();
        cx.background_executor()
            .spawn(async move { client.disconnect(None, Some(true), None).await })
            .detach_and_log_err(cx);
    }

    fn toggle_variable_collapsed(
        &mut self,
        variable_id: &SharedString,
        cx: &mut ViewContext<Self>,
    ) {
        match self.collapsed_variables.binary_search(&variable_id) {
            Ok(ix) => {
                self.collapsed_variables.remove(ix);
            }
            Err(ix) => {
                self.collapsed_variables.insert(ix, variable_id.clone());
            }
        };

        self.build_variable_list_entries(self.current_thread_state().current_stack_frame_id);

        cx.notify();
    }
}

impl Render for DebugPanelItem {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let thread_status = self.current_thread_state().status;
        let active_thread_item = &self.active_thread_item;

        h_flex()
            .key_context("DebugPanelItem")
            .track_focus(&self.focus_handle)
            .capture_action(cx.listener(Self::handle_continue_action))
            .capture_action(cx.listener(Self::handle_step_over_action))
            .capture_action(cx.listener(Self::handle_step_in_action))
            .capture_action(cx.listener(Self::handle_step_out_action))
            .capture_action(cx.listener(Self::handle_restart_action))
            .capture_action(cx.listener(Self::handle_pause_action))
            .capture_action(cx.listener(Self::handle_stop_action))
            .capture_action(cx.listener(Self::handle_disconnect_action))
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
                                                cx.dispatch_action(Box::new(Pause))
                                            }))
                                            .tooltip(move |cx| Tooltip::text("Pause program", cx)),
                                    )
                                } else {
                                    this.child(
                                        IconButton::new("debug-continue", IconName::DebugContinue)
                                            .on_click(cx.listener(|_, _, cx| {
                                                cx.dispatch_action(Box::new(Continue))
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
                                        cx.dispatch_action(Box::new(StepOver))
                                    }))
                                    .disabled(thread_status != ThreadStatus::Stopped)
                                    .tooltip(move |cx| Tooltip::text("Step over", cx)),
                            )
                            .child(
                                IconButton::new("debug-step-in", IconName::DebugStepInto)
                                    .on_click(
                                        cx.listener(|_, _, cx| {
                                            cx.dispatch_action(Box::new(StepIn))
                                        }),
                                    )
                                    .disabled(thread_status != ThreadStatus::Stopped)
                                    .tooltip(move |cx| Tooltip::text("Step in", cx)),
                            )
                            .child(
                                IconButton::new("debug-step-out", IconName::DebugStepOut)
                                    .on_click(
                                        cx.listener(|_, _, cx| {
                                            cx.dispatch_action(Box::new(StepOut))
                                        }),
                                    )
                                    .disabled(thread_status != ThreadStatus::Stopped)
                                    .tooltip(move |cx| Tooltip::text("Step out", cx)),
                            )
                            .child(
                                IconButton::new("debug-restart", IconName::DebugRestart)
                                    .on_click(
                                        cx.listener(|_, _, cx| {
                                            cx.dispatch_action(Box::new(Restart))
                                        }),
                                    )
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
                                    .on_click(
                                        cx.listener(|_, _, cx| cx.dispatch_action(Box::new(Stop))),
                                    )
                                    .disabled(
                                        thread_status != ThreadStatus::Stopped
                                            && thread_status != ThreadStatus::Running,
                                    )
                                    .tooltip(move |cx| Tooltip::text("Stop", cx)),
                            )
                            .child(
                                IconButton::new("debug-disconnect", IconName::DebugDisconnect)
                                    .on_click(cx.listener(|_, _, cx| {
                                        cx.dispatch_action(Box::new(Disconnect))
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
                        this.size_full()
                            .child(list(self.variable_list.clone()).gap_1_5().size_full())
                    })
                    .when(*active_thread_item == ThreadItem::Output, |this| {
                        this.child(self.output_editor.clone())
                    }),
            )
            .into_any()
    }
}
