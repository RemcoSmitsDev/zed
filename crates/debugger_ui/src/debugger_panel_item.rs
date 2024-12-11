use crate::console::Console;
use crate::debugger_panel::{DebugPanel, DebugPanelEvent, ThreadState};
use crate::loaded_source_list::LoadedSourceList;
use crate::module_list::ModuleList;
use crate::stack_frame_list::{StackFrameList, StackFrameListEvent};
use crate::variable_list::VariableList;

use dap::{
    client::{DebugAdapterClientId, ThreadStatus},
    debugger_settings::DebuggerSettings,
    Capabilities, ContinuedEvent, LoadedSourceEvent, ModuleEvent, OutputEvent, OutputEventCategory,
    StoppedEvent, ThreadEvent,
};
use editor::Editor;
use gpui::{
    AnyElement, AppContext, EventEmitter, FocusHandle, FocusableView, Model, Subscription, Task,
    View, WeakView,
};
use project::dap_store::DapStore;
use rpc::proto::{self, PeerId};
use settings::Settings;
use ui::{prelude::*, Indicator, Tooltip, WindowContext};
use workspace::item;
use workspace::{
    item::{Item, ItemEvent},
    FollowableItem, ItemHandle, ViewId, Workspace,
};

#[derive(Debug)]
pub enum DebugPanelItemEvent {
    Close,
    Stopped { go_to_stack_frame: bool },
}

#[derive(Clone, PartialEq, Eq)]
enum ThreadItem {
    Console,
    LoadedSource,
    Modules,
    Output,
    Variables,
}

impl ThreadItem {
    fn to_proto(&self) -> proto::DebuggerThreadItem {
        match self {
            ThreadItem::Console => proto::DebuggerThreadItem::Console,
            ThreadItem::LoadedSource => proto::DebuggerThreadItem::LoadedSource,
            ThreadItem::Modules => proto::DebuggerThreadItem::Modules,
            ThreadItem::Output => proto::DebuggerThreadItem::Output,
            ThreadItem::Variables => proto::DebuggerThreadItem::Variables,
        }
    }

    fn from_proto(active_thread_item: proto::DebuggerThreadItem) -> Self {
        match active_thread_item {
            proto::DebuggerThreadItem::Console => ThreadItem::Console,
            proto::DebuggerThreadItem::LoadedSource => ThreadItem::LoadedSource,
            proto::DebuggerThreadItem::Modules => ThreadItem::Modules,
            proto::DebuggerThreadItem::Output => ThreadItem::Output,
            proto::DebuggerThreadItem::Variables => ThreadItem::Variables,
        }
    }
}

pub struct DebugPanelItem {
    thread_id: u64,
    remote_id: Option<ViewId>,
    console: View<Console>,
    show_console_indicator: bool,
    focus_handle: FocusHandle,
    dap_store: Model<DapStore>,
    output_editor: View<Editor>,
    module_list: View<ModuleList>,
    client_name: SharedString,
    active_thread_item: ThreadItem,
    workspace: WeakView<Workspace>,
    client_id: DebugAdapterClientId,
    thread_state: Model<ThreadState>,
    variable_list: View<VariableList>,
    _subscriptions: Vec<Subscription>,
    stack_frame_list: View<StackFrameList>,
    loaded_source_list: View<LoadedSourceList>,
}

impl DebugPanelItem {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        debug_panel: View<DebugPanel>,
        workspace: WeakView<Workspace>,
        dap_store: Model<DapStore>,
        thread_state: Model<ThreadState>,
        client_id: &DebugAdapterClientId,
        client_name: SharedString,
        thread_id: u64,
        cx: &mut ViewContext<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let this = cx.view().clone();
        let stack_frame_list = cx.new_view(|cx| {
            StackFrameList::new(&workspace, &this, &dap_store, client_id, thread_id, cx)
        });

        let variable_list = cx
            .new_view(|cx| VariableList::new(&stack_frame_list, dap_store.clone(), &client_id, cx));

        let module_list = cx.new_view(|cx| ModuleList::new(dap_store.clone(), &client_id, cx));

        let loaded_source_list =
            cx.new_view(|cx| LoadedSourceList::new(&this, dap_store.clone(), &client_id, cx));

        let console = cx.new_view(|cx| {
            Console::new(
                &stack_frame_list,
                client_id,
                variable_list.clone(),
                dap_store.clone(),
                cx,
            )
        });

        let _subscriptions = vec![
            cx.subscribe(&debug_panel, {
                move |this: &mut Self, _, event: &DebugPanelEvent, cx| {
                    match event {
                        DebugPanelEvent::Stopped {
                            client_id,
                            event,
                            go_to_stack_frame,
                        } => this.handle_stopped_event(client_id, event, *go_to_stack_frame, cx),
                        DebugPanelEvent::Thread((client_id, event)) => {
                            this.handle_thread_event(client_id, event, cx)
                        }
                        DebugPanelEvent::Output((client_id, event)) => {
                            this.handle_output_event(client_id, event, cx)
                        }
                        DebugPanelEvent::Module((client_id, event)) => {
                            this.handle_module_event(client_id, event, cx)
                        }
                        DebugPanelEvent::LoadedSource((client_id, event)) => {
                            this.handle_loaded_source_event(client_id, event, cx)
                        }
                        DebugPanelEvent::ClientStopped(client_id) => {
                            this.handle_client_stopped_event(client_id, cx)
                        }
                        DebugPanelEvent::Continued((client_id, event)) => {
                            this.handle_thread_continued_event(client_id, event, cx);
                        }
                        DebugPanelEvent::Exited(client_id)
                        | DebugPanelEvent::Terminated(client_id) => {
                            this.handle_client_exited_and_terminated_event(client_id, cx);
                        }
                        DebugPanelEvent::CapabilitiesChanged(client_id) => {
                            this.handle_capabilities_changed_event(client_id, cx);
                        }
                    };
                }
            }),
            cx.subscribe(
                &stack_frame_list,
                move |this: &mut Self, _, event: &StackFrameListEvent, cx| match event {
                    StackFrameListEvent::SelectedStackFrameChanged
                    | StackFrameListEvent::StackFramesUpdated => this.clear_highlights(cx),
                },
            ),
        ];

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
            console,
            thread_id,
            dap_store,
            workspace,
            client_name,
            module_list,
            thread_state,
            focus_handle,
            output_editor,
            variable_list,
            _subscriptions,
            remote_id: None,
            stack_frame_list,
            loaded_source_list,
            client_id: *client_id,
            show_console_indicator: false,
            active_thread_item: ThreadItem::Variables,
        }
    }

    pub(crate) fn set_from_proto(
        &mut self,
        state: &proto::view::DebugPanel,
        cx: &mut ViewContext<Self>,
    ) {
        self.active_thread_item = ThreadItem::from_proto(state.active_thread_item());

        if let Some(stack_frame_list) = state.stack_frames.as_ref() {
            self.stack_frame_list
                .update(cx, |this, _| this.set_from_proto(stack_frame_list.clone()));
        }

        if let Some(variable_list_state) = state.variable_list.as_ref() {
            self.variable_list
                .update(cx, |this, cx| this.set_from_proto(variable_list_state, cx));
        }

        self.module_list.update(cx, |this, cx| {
            this.set_from_proto(state.modules.clone(), cx)
        });

        cx.notify();
    }

    pub fn update_thread_state_status(&mut self, status: ThreadStatus, cx: &mut ViewContext<Self>) {
        self.thread_state.update(cx, |thread_state, cx| {
            thread_state.status = status;

            cx.notify();
        });

        if status == ThreadStatus::Exited
            || status == ThreadStatus::Ended
            || status == ThreadStatus::Stopped
        {
            self.clear_highlights(cx);
        }
    }

    fn should_skip_event(&self, client_id: &DebugAdapterClientId, thread_id: u64) -> bool {
        thread_id != self.thread_id || *client_id != self.client_id
    }

    fn handle_thread_continued_event(
        &mut self,
        client_id: &DebugAdapterClientId,
        event: &ContinuedEvent,
        cx: &mut ViewContext<Self>,
    ) {
        if self.should_skip_event(client_id, event.thread_id) {
            return;
        }

        self.update_thread_state_status(ThreadStatus::Running, cx);
    }

    fn handle_stopped_event(
        &mut self,
        client_id: &DebugAdapterClientId,
        event: &StoppedEvent,
        go_to_stack_frame: bool,
        cx: &mut ViewContext<Self>,
    ) {
        if self.should_skip_event(client_id, event.thread_id.unwrap_or(self.thread_id)) {
            return;
        }

        cx.emit(DebugPanelItemEvent::Stopped { go_to_stack_frame });
    }

    fn handle_thread_event(
        &mut self,
        client_id: &DebugAdapterClientId,
        event: &ThreadEvent,
        cx: &mut ViewContext<Self>,
    ) {
        if self.should_skip_event(client_id, event.thread_id) {
            return;
        }

        self.update_thread_state_status(ThreadStatus::Running, cx);
    }

    fn handle_output_event(
        &mut self,
        client_id: &DebugAdapterClientId,
        event: &OutputEvent,
        cx: &mut ViewContext<Self>,
    ) {
        if self.should_skip_event(client_id, self.thread_id) {
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
                self.console.update(cx, |console, cx| {
                    console.add_message(&event.output, cx);
                });

                if !matches!(self.active_thread_item, ThreadItem::Console) {
                    self.show_console_indicator = true;
                }
            }
            _ => {
                self.output_editor.update(cx, |editor, cx| {
                    editor.set_read_only(false);
                    editor.move_to_end(&editor::actions::MoveToEnd, cx);
                    editor.insert(format!("{}\n", &event.output.trim_end()).as_str(), cx);
                    editor.set_read_only(true);
                });
            }
        }
    }

    fn handle_module_event(
        &mut self,
        client_id: &DebugAdapterClientId,
        event: &ModuleEvent,
        cx: &mut ViewContext<Self>,
    ) {
        if self.should_skip_event(client_id, self.thread_id) {
            return;
        }

        self.module_list.update(cx, |variable_list, cx| {
            variable_list.on_module_event(event, cx);
        });
    }

    fn handle_loaded_source_event(
        &mut self,
        client_id: &DebugAdapterClientId,
        event: &LoadedSourceEvent,
        cx: &mut ViewContext<Self>,
    ) {
        if self.should_skip_event(client_id, self.thread_id) {
            return;
        }

        self.loaded_source_list
            .update(cx, |loaded_source_list, cx| {
                loaded_source_list.on_loaded_source_event(event, cx);
            });
    }

    fn handle_client_stopped_event(
        &mut self,
        client_id: &DebugAdapterClientId,
        cx: &mut ViewContext<Self>,
    ) {
        if self.should_skip_event(client_id, self.thread_id) {
            return;
        }

        self.update_thread_state_status(ThreadStatus::Stopped, cx);

        self.dap_store.update(cx, |store, cx| {
            store.remove_active_debug_line_for_client(client_id, cx);
        });

        cx.emit(DebugPanelItemEvent::Close);
    }

    fn handle_client_exited_and_terminated_event(
        &mut self,
        client_id: &DebugAdapterClientId,
        cx: &mut ViewContext<Self>,
    ) {
        if Self::should_skip_event(self, client_id, self.thread_id) {
            return;
        }

        self.update_thread_state_status(ThreadStatus::Exited, cx);

        self.dap_store.update(cx, |store, cx| {
            store.remove_active_debug_line_for_client(client_id, cx);
        });

        cx.emit(DebugPanelItemEvent::Close);
    }

    fn handle_capabilities_changed_event(
        &mut self,
        client_id: &DebugAdapterClientId,
        cx: &mut ViewContext<Self>,
    ) {
        if Self::should_skip_event(self, client_id, self.thread_id) {
            return;
        }

        cx.notify();
    }

    pub fn client_id(&self) -> DebugAdapterClientId {
        self.client_id
    }

    pub fn thread_id(&self) -> u64 {
        self.thread_id
    }

    pub fn capabilities(&self, cx: &mut ViewContext<Self>) -> Capabilities {
        self.dap_store
            .read_with(cx, |store, _| store.capabilities_by_id(&self.client_id))
    }

    fn clear_highlights(&self, cx: &mut ViewContext<Self>) {
        if let Some((_, project_path, _)) = self.dap_store.read(cx).active_debug_line() {
            self.workspace
                .update(cx, |workspace, cx| {
                    let editor = workspace
                        .items_of_type::<Editor>(cx)
                        .find(|editor| Some(project_path.clone()) == editor.project_path(cx));

                    if let Some(editor) = editor {
                        editor.update(cx, |editor, cx| {
                            editor.clear_row_highlights::<editor::DebugCurrentRowHighlight>();

                            cx.notify();
                        });
                    }
                })
                .ok();
        }
    }

    pub fn go_to_current_stack_frame(&self, cx: &mut ViewContext<Self>) {
        self.stack_frame_list.update(cx, |stack_frame_list, cx| {
            if let Some(stack_frame) = stack_frame_list
                .stack_frames()
                .iter()
                .find(|frame| frame.id == stack_frame_list.current_stack_frame_id())
                .cloned()
            {
                stack_frame_list
                    .select_stack_frame(&stack_frame, true, cx)
                    .detach_and_log_err(cx);
            }
        });
    }

    fn render_entry_button(
        &self,
        label: &SharedString,
        thread_item: ThreadItem,
        cx: &mut ViewContext<Self>,
    ) -> AnyElement {
        let has_indicator =
            matches!(thread_item, ThreadItem::Console) && self.show_console_indicator;

        div()
            .id(label.clone())
            .px_2()
            .py_1()
            .cursor_pointer()
            .border_b_2()
            .when(self.active_thread_item == thread_item, |this| {
                this.border_color(cx.theme().colors().border)
            })
            .child(
                h_flex()
                    .child(Button::new(label.clone(), label.clone()))
                    .when(has_indicator, |this| this.child(Indicator::dot())),
            )
            .on_click(cx.listener(move |this, _, cx| {
                this.active_thread_item = thread_item.clone();

                if matches!(this.active_thread_item, ThreadItem::Console) {
                    this.show_console_indicator = false;
                }

                cx.notify();
            }))
            .into_any_element()
    }

    pub fn continue_thread(&mut self, cx: &mut ViewContext<Self>) {
        self.update_thread_state_status(ThreadStatus::Running, cx);

        self.dap_store.update(cx, |store, cx| {
            store
                .continue_thread(&self.client_id, self.thread_id, cx)
                .detach_and_log_err(cx);
        });
    }

    pub fn step_over(&mut self, cx: &mut ViewContext<Self>) {
        self.update_thread_state_status(ThreadStatus::Running, cx);

        let granularity = DebuggerSettings::get_global(cx).stepping_granularity;

        self.dap_store.update(cx, |store, cx| {
            store
                .step_over(&self.client_id, self.thread_id, granularity, cx)
                .detach_and_log_err(cx);
        });
    }

    pub fn step_in(&mut self, cx: &mut ViewContext<Self>) {
        self.update_thread_state_status(ThreadStatus::Running, cx);

        let granularity = DebuggerSettings::get_global(cx).stepping_granularity;

        self.dap_store.update(cx, |store, cx| {
            store
                .step_in(&self.client_id, self.thread_id, granularity, cx)
                .detach_and_log_err(cx);
        });
    }

    pub fn step_out(&mut self, cx: &mut ViewContext<Self>) {
        self.update_thread_state_status(ThreadStatus::Running, cx);

        let granularity = DebuggerSettings::get_global(cx).stepping_granularity;

        self.dap_store.update(cx, |store, cx| {
            store
                .step_out(&self.client_id, self.thread_id, granularity, cx)
                .detach_and_log_err(cx);
        });
    }

    pub fn restart_client(&self, cx: &mut ViewContext<Self>) {
        self.dap_store.update(cx, |store, cx| {
            store
                .restart(&self.client_id, None, cx)
                .detach_and_log_err(cx);
        });
    }

    pub fn pause_thread(&self, cx: &mut ViewContext<Self>) {
        self.dap_store.update(cx, |store, cx| {
            store
                .pause_thread(&self.client_id, self.thread_id, cx)
                .detach_and_log_err(cx)
        });
    }

    pub fn stop_thread(&self, cx: &mut ViewContext<Self>) {
        self.dap_store.update(cx, |store, cx| {
            store
                .terminate_threads(&self.client_id, Some(vec![self.thread_id; 1]), cx)
                .detach_and_log_err(cx)
        });
    }

    pub fn disconnect_client(&self, cx: &mut ViewContext<Self>) {
        self.dap_store.update(cx, |store, cx| {
            store
                .disconnect_client(&self.client_id, cx)
                .detach_and_log_err(cx);
        });
    }

    pub fn toggle_ignore_breakpoints(&mut self, cx: &mut ViewContext<Self>) {
        self.workspace
            .update(cx, |workspace, cx| {
                workspace.project().update(cx, |project, cx| {
                    project
                        .toggle_ignore_breakpoints(&self.client_id, cx)
                        .detach_and_log_err(cx);
                })
            })
            .ok();
    }
}

impl EventEmitter<DebugPanelItemEvent> for DebugPanelItem {}

impl FocusableView for DebugPanelItem {
    fn focus_handle(&self, _: &AppContext) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for DebugPanelItem {
    type Event = DebugPanelItemEvent;

    fn tab_content(
        &self,
        params: workspace::item::TabContentParams,
        _: &WindowContext,
    ) -> AnyElement {
        Label::new(format!("{} - Thread {}", self.client_name, self.thread_id))
            .color(if params.selected {
                Color::Default
            } else {
                Color::Muted
            })
            .into_any_element()
    }

    fn tab_tooltip_text(&self, cx: &AppContext) -> Option<SharedString> {
        Some(SharedString::from(format!(
            "{} Thread {} - {:?}",
            self.client_name,
            self.thread_id,
            self.thread_state.read(cx).status,
        )))
    }

    fn to_item_events(event: &Self::Event, mut f: impl FnMut(ItemEvent)) {
        match event {
            DebugPanelItemEvent::Close => f(ItemEvent::CloseItem),
            DebugPanelItemEvent::Stopped { .. } => {}
        }
    }
}

impl FollowableItem for DebugPanelItem {
    fn remote_id(&self) -> Option<workspace::ViewId> {
        self.remote_id
    }

    fn to_state_proto(&self, cx: &WindowContext) -> Option<proto::view::Variant> {
        let thread_state = Some(self.thread_state.read_with(cx, |this, _| this.to_proto()));
        let modules = self.module_list.read(cx).to_proto();
        let variable_list = Some(self.variable_list.read(cx).to_proto());
        let stack_frames = Some(self.stack_frame_list.read(cx).to_proto());

        Some(proto::view::Variant::DebugPanel(proto::view::DebugPanel {
            project_id: 1,
            client_id: self.client_id.to_proto(),
            thread_id: self.thread_id,
            console: None,
            modules,
            active_thread_item: self.active_thread_item.to_proto().into(),
            thread_state,
            variable_list,
            stack_frames,
        }))
    }

    fn from_state_proto(
        workspace: View<Workspace>,
        remote_id: ViewId,
        state: &mut Option<proto::view::Variant>,
        cx: &mut WindowContext,
    ) -> Option<gpui::Task<gpui::Result<View<Self>>>> {
        let proto::view::Variant::DebugPanel(_) = state.as_ref()? else {
            return None;
        };
        let Some(proto::view::Variant::DebugPanel(state)) = state.take() else {
            unreachable!()
        };

        let (_project, debug_panel) = workspace.update(cx, |workspace, cx| {
            Some((
                workspace.project().clone(),
                workspace.panel::<DebugPanel>(cx)?,
            ))
        })?;

        let debug_panel_item = debug_panel.update(cx, |this, cx| {
            this.open_remote_debug_panel_item(
                DebugAdapterClientId::from_proto(state.client_id),
                state.thread_id,
                cx,
            )
        });

        debug_panel_item.update(cx, |debug_panel_item, cx| {
            debug_panel_item.remote_id = Some(remote_id);
            debug_panel_item.set_from_proto(&state, cx);
        });

        Some(Task::ready(Ok(debug_panel_item)))
    }

    fn add_event_to_update_proto(
        &self,
        _event: &Self::Event,
        update: &mut Option<proto::update_view::Variant>,
        _cx: &WindowContext,
    ) -> bool {
        update.get_or_insert_with(|| proto::update_view::Variant::DebugPanel(Default::default()));

        true
    }

    fn apply_update_proto(
        &mut self,
        _project: &Model<project::Project>,
        _message: proto::update_view::Variant,
        _cx: &mut ViewContext<Self>,
    ) -> gpui::Task<gpui::Result<()>> {
        dbg!("apply update from proto");

        Task::ready(Ok(()))
    }

    fn set_leader_peer_id(&mut self, _leader_peer_id: Option<PeerId>, _cx: &mut ViewContext<Self>) {
    }

    fn to_follow_event(_event: &Self::Event) -> Option<workspace::item::FollowEvent> {
        None
    }

    fn dedup(&self, existing: &Self, _cx: &WindowContext) -> Option<workspace::item::Dedup> {
        if existing.client_id == self.client_id && existing.thread_id == self.thread_id {
            Some(item::Dedup::KeepExisting)
        } else {
            None
        }
    }

    fn is_project_item(&self, _cx: &WindowContext) -> bool {
        true
    }
}

impl Render for DebugPanelItem {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let thread_status = self.thread_state.read(cx).status;
        let active_thread_item = &self.active_thread_item;

        let capabilities = self.capabilities(cx);

        h_flex()
            .key_context("DebugPanelItem")
            .track_focus(&self.focus_handle)
            .size_full()
            .items_start()
            .child(
                v_flex()
                    .size_full()
                    .items_start()
                    .child(
                        h_flex()
                            .p_1()
                            .border_b_1()
                            .w_full()
                            .border_color(cx.theme().colors().border_variant)
                            .gap_2()
                            .map(|this| {
                                if thread_status == ThreadStatus::Running {
                                    this.child(
                                        IconButton::new("debug-pause", IconName::DebugPause)
                                            .icon_size(IconSize::Small)
                                            .on_click(cx.listener(|this, _, cx| {
                                                this.pause_thread(cx);
                                            }))
                                            .tooltip(move |cx| Tooltip::text("Pause program", cx)),
                                    )
                                } else {
                                    this.child(
                                        IconButton::new("debug-continue", IconName::DebugContinue)
                                            .icon_size(IconSize::Small)
                                            .on_click(
                                                cx.listener(|this, _, cx| this.continue_thread(cx)),
                                            )
                                            .disabled(thread_status != ThreadStatus::Stopped)
                                            .tooltip(move |cx| {
                                                Tooltip::text("Continue program", cx)
                                            }),
                                    )
                                }
                            })
                            .child(
                                IconButton::new("debug-step-over", IconName::DebugStepOver)
                                    .icon_size(IconSize::Small)
                                    .on_click(cx.listener(|this, _, cx| {
                                        this.step_over(cx);
                                    }))
                                    .disabled(thread_status != ThreadStatus::Stopped)
                                    .tooltip(move |cx| Tooltip::text("Step over", cx)),
                            )
                            .child(
                                IconButton::new("debug-step-in", IconName::DebugStepInto)
                                    .icon_size(IconSize::Small)
                                    .on_click(cx.listener(|this, _, cx| {
                                        this.step_in(cx);
                                    }))
                                    .disabled(thread_status != ThreadStatus::Stopped)
                                    .tooltip(move |cx| Tooltip::text("Step in", cx)),
                            )
                            .child(
                                IconButton::new("debug-step-out", IconName::DebugStepOut)
                                    .icon_size(IconSize::Small)
                                    .on_click(cx.listener(|this, _, cx| {
                                        this.step_out(cx);
                                    }))
                                    .disabled(thread_status != ThreadStatus::Stopped)
                                    .tooltip(move |cx| Tooltip::text("Step out", cx)),
                            )
                            .child(
                                IconButton::new("debug-restart", IconName::DebugRestart)
                                    .icon_size(IconSize::Small)
                                    .on_click(cx.listener(|this, _, cx| {
                                        this.restart_client(cx);
                                    }))
                                    .disabled(
                                        !capabilities.supports_restart_request.unwrap_or_default(),
                                    )
                                    .tooltip(move |cx| Tooltip::text("Restart", cx)),
                            )
                            .child(
                                IconButton::new("debug-stop", IconName::DebugStop)
                                    .icon_size(IconSize::Small)
                                    .on_click(cx.listener(|this, _, cx| {
                                        this.stop_thread(cx);
                                    }))
                                    .disabled(
                                        thread_status != ThreadStatus::Stopped
                                            && thread_status != ThreadStatus::Running,
                                    )
                                    .tooltip(move |cx| Tooltip::text("Stop", cx)),
                            )
                            .child(
                                IconButton::new("debug-disconnect", IconName::DebugDisconnect)
                                    .icon_size(IconSize::Small)
                                    .on_click(cx.listener(|this, _, cx| {
                                        this.disconnect_client(cx);
                                    }))
                                    .disabled(
                                        thread_status == ThreadStatus::Exited
                                            || thread_status == ThreadStatus::Ended,
                                    )
                                    .tooltip(move |cx| Tooltip::text("Disconnect", cx)),
                            )
                            .child(
                                IconButton::new(
                                    "debug-ignore-breakpoints",
                                    if self.dap_store.read(cx).ignore_breakpoints(&self.client_id) {
                                        IconName::DebugIgnoreBreakpoints
                                    } else {
                                        IconName::DebugBreakpoint
                                    },
                                )
                                .icon_size(IconSize::Small)
                                .on_click(cx.listener(|this, _, cx| {
                                    this.toggle_ignore_breakpoints(cx);
                                }))
                                .disabled(
                                    thread_status == ThreadStatus::Exited
                                        || thread_status == ThreadStatus::Ended,
                                )
                                .tooltip(move |cx| Tooltip::text("Ignore breakpoints", cx)),
                            ),
                    )
                    .child(
                        h_flex()
                            .size_full()
                            .items_start()
                            .p_1()
                            .gap_4()
                            .child(self.stack_frame_list.clone()),
                    ),
            )
            .child(
                v_flex()
                    .border_l_1()
                    .border_color(cx.theme().colors().border_variant)
                    .size_full()
                    .items_start()
                    .child(
                        h_flex()
                            .border_b_1()
                            .w_full()
                            .border_color(cx.theme().colors().border_variant)
                            .child(self.render_entry_button(
                                &SharedString::from("Variables"),
                                ThreadItem::Variables,
                                cx,
                            ))
                            .when(
                                capabilities.supports_modules_request.unwrap_or_default(),
                                |this| {
                                    this.child(self.render_entry_button(
                                        &SharedString::from("Modules"),
                                        ThreadItem::Modules,
                                        cx,
                                    ))
                                },
                            )
                            .when(
                                capabilities
                                    .supports_loaded_sources_request
                                    .unwrap_or_default(),
                                |this| {
                                    this.child(self.render_entry_button(
                                        &SharedString::from("Loaded Sources"),
                                        ThreadItem::LoadedSource,
                                        cx,
                                    ))
                                },
                            )
                            .child(self.render_entry_button(
                                &SharedString::from("Console"),
                                ThreadItem::Console,
                                cx,
                            ))
                            .child(self.render_entry_button(
                                &SharedString::from("Output"),
                                ThreadItem::Output,
                                cx,
                            )),
                    )
                    .when(*active_thread_item == ThreadItem::Variables, |this| {
                        this.size_full().child(self.variable_list.clone())
                    })
                    .when(*active_thread_item == ThreadItem::Modules, |this| {
                        this.size_full().child(self.module_list.clone())
                    })
                    .when(*active_thread_item == ThreadItem::LoadedSource, |this| {
                        this.size_full().child(self.loaded_source_list.clone())
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
