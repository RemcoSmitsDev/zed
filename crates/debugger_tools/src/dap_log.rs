use dap::client::IoKind;
use dap::{
    adapters::DebugAdapterName,
    client::{DebugAdapterClient, DebugAdapterClientId},
};
use editor::{Editor, EditorEvent};
use futures::StreamExt;
use gpui::{
    actions, div, AnchorCorner, AppContext, Context, EventEmitter, FocusHandle, FocusableView,
    IntoElement, Model, ModelContext, ParentElement, Render, SharedString, Styled, Subscription,
    View, ViewContext, VisualContext, WeakModel, WindowContext,
};
use project::{search::SearchQuery, Project};
use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use workspace::{
    item::Item,
    searchable::{SearchEvent, SearchableItem, SearchableItemHandle},
    ui::{h_flex, Button, Checkbox, Clickable, ContextMenu, Label, PopoverMenu, Selection},
    ToolbarItemEvent, ToolbarItemView, Workspace,
};

struct DapLogView {
    pub(crate) editor: View<Editor>,
    focus_handle: FocusHandle,
    log_store: Model<LogStore>,
    editor_subscriptions: Vec<Subscription>,
    current_view: Option<(DebugAdapterClientId, LogKind)>,
    project: Model<Project>,
    _subscriptions: Vec<Subscription>,
}

struct LogStore {
    projects: HashMap<WeakModel<Project>, ProjectState>,
    debug_clients: HashMap<DebugAdapterClientId, DebugAdapterState>,
    io_tx: futures::channel::mpsc::UnboundedSender<(DebugAdapterClientId, IoKind, String)>,
}

struct ProjectState {
    _subscriptions: [gpui::Subscription; 2],
}

struct DebugAdapterState {
    // log_messages: VecDeque<String>,
    rpc_messages: Option<RpcMessages>,
}

struct RpcMessages {
    messages: VecDeque<String>,
    last_message_kind: Option<MessageKind>,
}

impl RpcMessages {
    fn new() -> Self {
        Self {
            messages: VecDeque::with_capacity(255),
            last_message_kind: None,
        }
    }
}

const SEND: &str = "// Send";
const RECEIVE: &str = "// Receive";

#[derive(Clone, Copy, PartialEq, Eq)]
enum MessageKind {
    Send,
    Receive,
}

impl MessageKind {
    fn label(&self) -> &'static str {
        match self {
            Self::Send => SEND,
            Self::Receive => RECEIVE,
        }
    }
}

impl DebugAdapterState {
    fn new() -> Self {
        Self {
            // log_messages: VecDeque::new(),
            rpc_messages: None,
        }
    }
}

impl LogStore {
    fn new(cx: &ModelContext<Self>) -> Self {
        let (io_tx, mut io_rx) =
            futures::channel::mpsc::unbounded::<(DebugAdapterClientId, IoKind, String)>();
        cx.spawn(|this, mut cx| async move {
            while let Some((server_id, io_kind, message)) = io_rx.next().await {
                if let Some(this) = this.upgrade() {
                    this.update(&mut cx, |this, cx| {
                        this.on_io(server_id, io_kind, &message, cx);
                    })?;
                }
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
        Self {
            projects: HashMap::new(),
            debug_clients: HashMap::new(),
            io_tx,
        }
    }

    fn on_io(
        &mut self,
        client_id: DebugAdapterClientId,
        io_kind: IoKind,
        message: &str,
        cx: &mut ModelContext<Self>,
    ) -> Option<()> {
        self.add_debug_client_message(client_id, io_kind, message.to_string(), cx);

        Some(())
    }

    pub fn add_project(&mut self, project: &Model<Project>, cx: &mut ModelContext<Self>) {
        let weak_project = project.downgrade();
        self.projects.insert(
            project.downgrade(),
            ProjectState {
                _subscriptions: [
                    cx.observe_release(project, move |this, _, _| {
                        this.projects.remove(&weak_project);
                    }),
                    cx.subscribe(project, |this, project, event, cx| match event {
                        project::Event::DebugClientStarted(id) => {
                            let read_project = project.read(cx);
                            if let Some(client) = read_project.debug_client_for_id(*id, cx) {
                                this.add_debug_client(client.id(), Some(client), cx);
                            }
                        }
                        project::Event::DebugClientStopped(id) => {
                            this.remove_debug_client(*id, cx);
                        }

                        _ => {}
                    }),
                ],
            },
        );
    }

    fn get_debug_adapter_state(
        &mut self,
        id: DebugAdapterClientId,
    ) -> Option<&mut DebugAdapterState> {
        self.debug_clients.get_mut(&id)
    }

    fn add_debug_client_message(
        &mut self,
        id: DebugAdapterClientId,
        io_kind: IoKind,
        message: String,
        cx: &mut ModelContext<Self>,
    ) -> Option<()> {
        let debug_client_state = self.get_debug_adapter_state(id)?;

        let kind = match io_kind {
            IoKind::StdOut | IoKind::StdErr => MessageKind::Receive,
            IoKind::StdIn => MessageKind::Send,
        };

        let rpc_messages = &mut debug_client_state.rpc_messages;
        if let Some(rpc_messages) = rpc_messages {
            if rpc_messages.last_message_kind != Some(kind) {
                Self::add_debug_client_log(
                    &mut rpc_messages.messages,
                    id,
                    kind.label().to_string(),
                    LogKind::Rpc,
                    cx,
                );
                rpc_messages.last_message_kind = Some(kind);
            }
            Self::add_debug_client_log(&mut rpc_messages.messages, id, message, LogKind::Rpc, cx);
        }

        Some(())
    }

    fn add_debug_client_log(
        log_lines: &mut VecDeque<String>,
        id: DebugAdapterClientId,
        message: String,
        kind: LogKind,
        cx: &mut ModelContext<Self>,
    ) {
        while log_lines.len() >= 255 {
            log_lines.pop_front();
        }
        let entry: &str = message.as_ref();
        let entry = entry.to_string();
        log_lines.push_back(message);

        cx.emit(Event::NewLogEntry { id, entry, kind });
        cx.notify();
    }

    fn add_debug_client(
        &mut self,
        client_id: DebugAdapterClientId,
        client: Option<Arc<DebugAdapterClient>>,
        cx: &mut ModelContext<Self>,
    ) -> Option<&mut DebugAdapterState> {
        let client_state = self.debug_clients.entry(client_id).or_insert_with(|| {
            cx.notify();
            DebugAdapterState::new()
        });

        if let Some(client) = client {
            let io_tx = self.io_tx.clone();
            client.on_io(move |io_kind, message| {
                io_tx
                    .unbounded_send((client_id, io_kind, message.to_string()))
                    .ok();
            });
        }

        Some(client_state)
    }

    fn remove_debug_client(&mut self, id: DebugAdapterClientId, cx: &mut ModelContext<Self>) {
        self.debug_clients.remove(&id);
        cx.notify();
    }

    fn rpc_logging_enabled_for_client(&self, client_id: DebugAdapterClientId) -> bool {
        self.debug_clients
            .get(&client_id)
            .is_some_and(|client| client.rpc_messages.is_some())
    }

    fn enable_rpc_trace_for_debug_client(
        &mut self,
        client_id: DebugAdapterClientId,
    ) -> Option<&mut VecDeque<String>> {
        let rpc_state = self
            .debug_clients
            .get_mut(&client_id)?
            .rpc_messages
            .get_or_insert_with(|| RpcMessages::new());
        Some(&mut rpc_state.messages)
    }

    pub fn disable_rpc_trace_for_debug_client(
        &mut self,
        client_id: DebugAdapterClientId,
    ) -> Option<()> {
        self.debug_clients.get_mut(&client_id)?.rpc_messages.take();
        Some(())
    }

    // fn clients(&self) -> impl Iterator<Item = (&DebugAdapterClientId, &DebugAdapterState)> {
    //     self.debug_clients.iter()
    // }
}

pub struct DapLogToolbarItemView {
    log_view: Option<View<DapLogView>>,
}

impl DapLogToolbarItemView {
    pub fn new() -> Self {
        Self { log_view: None }
    }

    fn toggle_rpc_logging_for_client(
        &mut self,
        id: DebugAdapterClientId,
        enabled: bool,
        cx: &mut ViewContext<Self>,
    ) {
        if let Some(log_view) = &self.log_view {
            log_view.update(cx, |log_view, cx| {
                log_view.toggle_rpc_trace_for_server(id, enabled, cx);
                if enabled {
                    log_view.show_rpc_trace_for_server(id, cx);
                    cx.notify();
                }
                cx.focus(&log_view.focus_handle);
            });
        }
        cx.notify();
    }
}

impl Render for DapLogToolbarItemView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let Some(log_view) = self.log_view.clone() else {
            return div();
        };

        let (menu_rows, current_client_id) = log_view.update(cx, |log_view, cx| {
            (
                log_view.menu_items(cx).unwrap_or_default(),
                log_view.current_view.map(|(client_id, _)| client_id),
            )
        });

        let current_client = current_client_id.and_then(|current_client_id| {
            if let Ok(ix) = menu_rows.binary_search_by_key(&current_client_id, |e| e.client_id) {
                Some(menu_rows[ix].clone())
            } else {
                None
            }
        });

        let log_toolbar_view = cx.view().clone();
        let dap_menu: PopoverMenu<_> = PopoverMenu::new("DapLogView")
            .anchor(AnchorCorner::TopLeft)
            .trigger(Button::new(
                "debug_server_menu_header",
                current_client
                    .map(|row| {
                        Cow::Owned(format!(
                            "{} ({}) - {}",
                            row.client_name,
                            row.worktree_root_name,
                            row.selected_entry.label()
                        ))
                    })
                    .unwrap_or_else(|| "No server selected".into()),
            ))
            .menu({
                let _log_view = log_view.clone();
                let log_toolbar_view = log_toolbar_view.clone();
                move |cx| {
                    let log_view = log_view.clone();
                    let menu_rows = menu_rows.clone();
                    let log_toolbar_view = log_toolbar_view.clone();
                    ContextMenu::build(cx, move |mut menu, cx| {
                        for (ix, row) in menu_rows.into_iter().enumerate() {
                            // let server_selected = Some(row.client_id) == current_client_id;

                            // menu = menu
                            //     .header(format!(
                            //         "{} ({})",
                            //         row.client_name.0, row.worktree_root_name
                            //     ))
                            //     .entry(
                            //         CLIENT_LOGS,
                            //         None,
                            //         cx.handler_for(&log_view, move |view, cx| {
                            //             // view show_logs_for_server(row.server_id, cx);
                            //         }),
                            //     );
                            menu = menu.custom_entry(
                                {
                                    let log_toolbar_view = log_toolbar_view.clone();
                                    move |cx| {
                                        h_flex()
                                            .w_full()
                                            .justify_between()
                                            .child(Label::new(RPC_MESSAGES))
                                            .child(
                                                div().child(
                                                    Checkbox::new(
                                                        ix,
                                                        if row.rpc_trace_enabled {
                                                            Selection::Selected
                                                        } else {
                                                            Selection::Unselected
                                                        },
                                                    )
                                                    .on_click(cx.listener_for(
                                                        &log_toolbar_view,
                                                        move |view, selection, cx| {
                                                            let enabled = matches!(
                                                                selection,
                                                                Selection::Selected
                                                            );
                                                            view.toggle_rpc_logging_for_client(
                                                                row.client_id,
                                                                enabled,
                                                                cx,
                                                            );
                                                            cx.stop_propagation();
                                                        },
                                                    )),
                                                ),
                                            )
                                            .into_any_element()
                                    }
                                },
                                cx.handler_for(&log_view, move |view, cx| {
                                    view.show_rpc_trace_for_server(row.client_id, cx);
                                }),
                            );
                        }
                        menu
                    })
                    .into()
                }
            });

        h_flex().size_full().child(dap_menu).child(
            div()
                .child(
                    Button::new("clear_log_button", "Clear").on_click(cx.listener(
                        |this, _, cx| {
                            if let Some(log_view) = this.log_view.as_ref() {
                                log_view.update(cx, |log_view, cx| {
                                    log_view.editor.update(cx, |editor, cx| {
                                        editor.set_read_only(false);
                                        editor.clear(cx);
                                        editor.set_read_only(true);
                                    });
                                })
                            }
                        },
                    )),
                )
                .ml_2(),
        )
    }
}

impl EventEmitter<ToolbarItemEvent> for DapLogToolbarItemView {}

impl ToolbarItemView for DapLogToolbarItemView {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn workspace::item::ItemHandle>,
        _cx: &mut ViewContext<Self>, // TODO
    ) -> workspace::ToolbarItemLocation {
        if let Some(item) = active_pane_item {
            if let Some(log_view) = item.downcast::<DapLogView>() {
                self.log_view = Some(log_view.clone());
                return workspace::ToolbarItemLocation::PrimaryLeft;
            }
        }
        self.log_view = None;
        workspace::ToolbarItemLocation::Hidden
    }
}

impl DapLogView {
    pub fn new(
        project: Model<Project>,
        log_store: Model<LogStore>,
        cx: &mut ViewContext<Self>,
    ) -> Self {
        let (editor, editor_subscriptions) = Self::editor_for_logs(String::new(), cx);

        let focus_handle = cx.focus_handle();

        let events_subscriptions = cx.subscribe(&log_store, |log_view, _, e, cx| match e {
            Event::NewLogEntry { id, entry, kind } => {
                if log_view.current_view == Some((*id, *kind)) {
                    log_view.editor.update(cx, |editor, cx| {
                        editor.set_read_only(false);
                        let last_point = editor.buffer().read(cx).len(cx);
                        editor.edit(
                            vec![
                                (last_point..last_point, entry.trim()),
                                (last_point..last_point, "\n"),
                            ],
                            cx,
                        );
                        editor.set_read_only(true);
                    });
                }
            }
        });

        Self {
            editor,
            focus_handle,
            project,
            log_store,
            editor_subscriptions,
            current_view: None,
            _subscriptions: vec![events_subscriptions],
        }
    }

    fn editor_for_logs(
        log_contents: String,
        cx: &mut ViewContext<Self>,
    ) -> (View<Editor>, Vec<Subscription>) {
        let editor = cx.new_view(|cx| {
            let mut editor = Editor::multi_line(cx);
            editor.set_text(log_contents, cx);
            editor.move_to_end(&editor::actions::MoveToEnd, cx);
            editor.set_read_only(true);
            editor.set_show_inline_completions(Some(false), cx);
            editor
        });
        let editor_subscription = cx.subscribe(
            &editor,
            |_, _, event: &EditorEvent, cx: &mut ViewContext<'_, DapLogView>| {
                cx.emit(event.clone())
            },
        );
        let search_subscription = cx.subscribe(
            &editor,
            |_, _, event: &SearchEvent, cx: &mut ViewContext<'_, DapLogView>| {
                cx.emit(event.clone())
            },
        );
        (editor, vec![editor_subscription, search_subscription])
    }

    fn menu_items<'a>(&self, cx: &'a AppContext) -> Option<Vec<DapMenuItem>> {
        let log_store = self.log_store.read(cx);

        let mut rows = self
            .project
            .read(cx)
            .debug_clients(cx)
            .filter_map(|client| {
                Some(DapMenuItem {
                    client_id: client.id(),
                    client_name: "TODO".into(),
                    worktree_root_name: String::new(),
                    selected_entry: LogKind::Rpc,
                    rpc_trace_enabled: log_store.rpc_logging_enabled_for_client(client.id()),
                })
            })
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| row.client_id);
        rows.dedup_by_key(|row| row.client_id);
        Some(rows)
    }

    fn show_rpc_trace_for_server(
        &mut self,
        client_id: DebugAdapterClientId,
        cx: &mut ViewContext<Self>,
    ) {
        let rpc_log = self.log_store.update(cx, |log_store, _| {
            log_store
                .enable_rpc_trace_for_debug_client(client_id)
                .map(|state| log_contents(&state))
        });
        if let Some(rpc_log) = rpc_log {
            self.current_view = Some((client_id, LogKind::Rpc));
            let (editor, editor_subscriptions) = Self::editor_for_logs(rpc_log, cx);
            let language = self.project.read(cx).languages().language_for_name("JSON");
            editor
                .read(cx)
                .buffer()
                .read(cx)
                .as_singleton()
                .expect("log buffer should be a singleton")
                .update(cx, |_, cx| {
                    cx.spawn({
                        let buffer = cx.handle();
                        |_, mut cx| async move {
                            let language = language.await.ok();
                            buffer.update(&mut cx, |buffer, cx| {
                                buffer.set_language(language, cx);
                            })
                        }
                    })
                    .detach_and_log_err(cx);
                });

            self.editor = editor;
            self.editor_subscriptions = editor_subscriptions;
            cx.notify();
        }

        cx.focus(&self.focus_handle);
    }

    fn toggle_rpc_trace_for_server(
        &mut self,
        client_id: DebugAdapterClientId,
        enabled: bool,
        cx: &mut ViewContext<Self>,
    ) {
        self.log_store.update(cx, |log_store, _| {
            if enabled {
                println!("rpc enabled");
                log_store.enable_rpc_trace_for_debug_client(client_id);
            } else {
                println!("rpc disabled");
                log_store.disable_rpc_trace_for_debug_client(client_id);
            }
        });
        if !enabled && Some(client_id) == self.current_view.map(|(client_id, _)| client_id) {
            // self.show(client_id, cx);
            cx.notify();
        }
    }
}

fn log_contents(lines: &VecDeque<String>) -> String {
    let (a, b) = lines.as_slices();
    let a = a.iter().map(move |v| v.as_ref());
    let b = b.iter().map(move |v| v.as_ref());
    a.chain(b).fold(String::new(), |mut acc, el| {
        acc.push_str(el);
        acc.push('\n');
        acc
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DapMenuItem {
    pub client_id: DebugAdapterClientId,
    pub client_name: String,
    pub worktree_root_name: String,
    pub rpc_trace_enabled: bool,
    pub selected_entry: LogKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LogKind {
    Rpc,
    #[default]
    Logs,
}

const CLIENT_LOGS: &str = "Client Logs";
const RPC_MESSAGES: &str = "RPC Messages";

impl LogKind {
    fn label(&self) -> &'static str {
        match self {
            LogKind::Rpc => RPC_MESSAGES,
            LogKind::Logs => CLIENT_LOGS,
        }
    }
}

impl Render for DapLogView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        self.editor
            .update(cx, |editor, cx| editor.render(cx).into_any_element())
    }
}

actions!(debug, [OpenDebuggerServerLogs]);

pub fn init(cx: &mut AppContext) {
    let log_store = cx.new_model(|cx| LogStore::new(cx));

    cx.observe_new_views(move |workspace: &mut Workspace, cx| {
        let project = workspace.project();
        if project.read(cx).is_local() {
            log_store.update(cx, |store, cx| {
                store.add_project(project, cx);
            });
        }

        let log_store = log_store.clone();
        workspace.register_action(move |workspace, _: &OpenDebuggerServerLogs, cx| {
            let project = workspace.project().read(cx);
            if project.is_local() {
                workspace.add_item_to_active_pane(
                    Box::new(cx.new_view(|cx| {
                        DapLogView::new(workspace.project().clone(), log_store.clone(), cx)
                    })),
                    None,
                    true,
                    cx,
                );
            }
        });
    })
    .detach();
}

impl Item for DapLogView {
    type Event = EditorEvent;

    fn to_item_events(event: &Self::Event, f: impl FnMut(workspace::item::ItemEvent)) {
        Editor::to_item_events(event, f)
    }

    fn tab_content_text(&self, _cx: &WindowContext) -> Option<SharedString> {
        Some("DAP Logs".into())
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        None
    }

    fn as_searchable(&self, handle: &View<Self>) -> Option<Box<dyn SearchableItemHandle>> {
        Some(Box::new(handle.clone()))
    }
}

impl SearchableItem for DapLogView {
    type Match = <Editor as SearchableItem>::Match;

    fn clear_matches(&mut self, cx: &mut ViewContext<Self>) {
        self.editor.update(cx, |e, cx| e.clear_matches(cx))
    }

    fn update_matches(&mut self, matches: &[Self::Match], cx: &mut ViewContext<Self>) {
        self.editor
            .update(cx, |e, cx| e.update_matches(matches, cx))
    }

    fn query_suggestion(&mut self, cx: &mut ViewContext<Self>) -> String {
        self.editor.update(cx, |e, cx| e.query_suggestion(cx))
    }

    fn activate_match(
        &mut self,
        index: usize,
        matches: &[Self::Match],
        cx: &mut ViewContext<Self>,
    ) {
        self.editor
            .update(cx, |e, cx| e.activate_match(index, matches, cx))
    }

    fn select_matches(&mut self, matches: &[Self::Match], cx: &mut ViewContext<Self>) {
        self.editor
            .update(cx, |e, cx| e.select_matches(matches, cx))
    }

    fn find_matches(
        &mut self,
        query: Arc<project::search::SearchQuery>,
        cx: &mut ViewContext<Self>,
    ) -> gpui::Task<Vec<Self::Match>> {
        self.editor.update(cx, |e, cx| e.find_matches(query, cx))
    }

    fn replace(&mut self, _: &Self::Match, _: &SearchQuery, _: &mut ViewContext<Self>) {
        // Since DAP Log is read-only, it doesn't make sense to support replace operation.
    }
    fn supported_options() -> workspace::searchable::SearchOptions {
        workspace::searchable::SearchOptions {
            case: true,
            word: true,
            regex: true,
            // DAP log is read-only.
            replacement: false,
            selection: false,
        }
    }
    fn active_match_index(
        &mut self,
        matches: &[Self::Match],
        cx: &mut ViewContext<Self>,
    ) -> Option<usize> {
        self.editor
            .update(cx, |e, cx| e.active_match_index(matches, cx))
    }
}

impl FocusableView for DapLogView {
    fn focus_handle(&self, _: &AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

pub enum Event {
    NewLogEntry {
        id: DebugAdapterClientId,
        entry: String,
        kind: LogKind,
    },
}

impl EventEmitter<Event> for LogStore {}
impl EventEmitter<Event> for DapLogView {}
impl EventEmitter<EditorEvent> for DapLogView {}
impl EventEmitter<SearchEvent> for DapLogView {}
