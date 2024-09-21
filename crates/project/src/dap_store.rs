use anyhow::{anyhow, Context as _, Result};
use collections::{HashMap, HashSet};
use dap::client::{DebugAdapterClient, DebugAdapterClientId};
use dap::messages::Message;
use dap::requests::{
    Attach, ConfigurationDone, Continue, Disconnect, Initialize, Launch, Next, Pause, StepIn,
    StepOut, TerminateThreads,
};
use dap::{
    AttachRequestArguments, Capabilities, ConfigurationDoneArguments, ContinueArguments,
    DisconnectArguments, InitializeRequestArguments, InitializeRequestArgumentsPathFormat,
    LaunchRequestArguments, NextArguments, PauseArguments, SourceBreakpoint, StepInArguments,
    StepOutArguments, SteppingGranularity, TerminateThreadsArguments,
};
use gpui::{AppContext, Context, EventEmitter, Global, Model, ModelContext, Task};
use language::{Buffer, BufferSnapshot};
use serde_json::Value;
use settings::WorktreeId;
use std::{
    collections::BTreeMap,
    future::Future,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering::SeqCst},
        Arc,
    },
};
use task::{DebugAdapterConfig, DebugRequestType};
use text::Point;
use util::ResultExt as _;

use crate::ProjectPath;

pub enum DapStoreEvent {
    DebugClientStarted(DebugAdapterClientId),
    DebugClientStopped(DebugAdapterClientId),
    DebugClientEvent {
        client_id: DebugAdapterClientId,
        message: Message,
    },
}

pub enum DebugAdapterClientState {
    Starting(Task<Option<Arc<DebugAdapterClient>>>),
    Running(Arc<DebugAdapterClient>),
}

pub struct DapStore {
    next_client_id: AtomicUsize,
    clients: HashMap<DebugAdapterClientId, DebugAdapterClientState>,
    breakpoints: BTreeMap<ProjectPath, HashSet<Breakpoint>>,
    capabilities: HashMap<DebugAdapterClientId, Capabilities>,
}

impl EventEmitter<DapStoreEvent> for DapStore {}

struct GlobalDapStore(Model<DapStore>);

impl Global for GlobalDapStore {}

pub fn init(cx: &mut AppContext) {
    let store = GlobalDapStore(cx.new_model(DapStore::new));
    cx.set_global(store);
}

impl DapStore {
    pub fn global(cx: &AppContext) -> Model<Self> {
        cx.global::<GlobalDapStore>().0.clone()
    }

    pub fn new(cx: &mut ModelContext<Self>) -> Self {
        cx.on_app_quit(Self::shutdown_clients).detach();

        Self {
            clients: Default::default(),
            capabilities: HashMap::default(),
            breakpoints: Default::default(),
            next_client_id: Default::default(),
        }
    }

    pub fn next_client_id(&self) -> DebugAdapterClientId {
        DebugAdapterClientId(self.next_client_id.fetch_add(1, SeqCst))
    }

    pub fn running_clients(&self) -> impl Iterator<Item = Arc<DebugAdapterClient>> + '_ {
        self.clients.values().filter_map(|state| match state {
            DebugAdapterClientState::Starting(_) => None,
            DebugAdapterClientState::Running(client) => Some(client.clone()),
        })
    }

    pub fn client_by_id(&self, id: &DebugAdapterClientId) -> Option<Arc<DebugAdapterClient>> {
        self.clients.get(id).and_then(|state| match state {
            DebugAdapterClientState::Starting(_) => None,
            DebugAdapterClientState::Running(client) => Some(client.clone()),
        })
    }

    pub fn capabilities_by_id(&self, client_id: &DebugAdapterClientId) -> Capabilities {
        self.capabilities
            .get(client_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn merge_capabilities_for_client(
        &mut self,
        client_id: &DebugAdapterClientId,
        other: &Capabilities,
    ) {
        if let Some(capabilities) = self.capabilities.get_mut(client_id) {
            *capabilities = capabilities.merge(other.clone());
        }
    }

    pub fn breakpoints(&self) -> &BTreeMap<ProjectPath, HashSet<Breakpoint>> {
        &self.breakpoints
    }

    pub fn set_active_breakpoints(&mut self, project_path: &ProjectPath, buffer: &Buffer) {
        let entry = self.breakpoints.remove(project_path).unwrap_or_default();
        let mut set_bp: HashSet<Breakpoint> = HashSet::default();

        for mut bp in entry.into_iter() {
            bp.set_active_position(&buffer);
            set_bp.insert(bp);
        }

        self.breakpoints.insert(project_path.clone(), set_bp);
    }

    pub fn deserialize_breakpoints(
        &mut self,
        worktree_id: WorktreeId,
        serialize_breakpoints: Vec<SerializedBreakpoint>,
    ) {
        for serialize_breakpoint in serialize_breakpoints {
            self.breakpoints
                .entry(ProjectPath {
                    worktree_id,
                    path: serialize_breakpoint.path.clone(),
                })
                .or_default()
                .insert(Breakpoint {
                    active_position: None,
                    cache_position: serialize_breakpoint.position.saturating_sub(1u32),
                });
        }
    }

    pub fn sync_open_breakpoints_to_closed_breakpoints(
        &mut self,
        project_path: &ProjectPath,
        buffer: &mut Buffer,
    ) {
        if let Some(breakpoint_set) = self.breakpoints.remove(project_path) {
            let breakpoint_iter = breakpoint_set.into_iter().map(|mut bp| {
                bp.cache_position = bp.point_for_buffer(&buffer).row;
                bp.active_position = None;
                bp
            });

            self.breakpoints.insert(
                project_path.clone(),
                breakpoint_iter.collect::<HashSet<_>>(),
            );
        }
    }

    pub fn start_client(&mut self, config: DebugAdapterConfig, cx: &mut ModelContext<Self>) {
        let client_id = self.next_client_id();

        let start_client_task = cx.spawn(|this, mut cx| async move {
            let dap_store = this.clone();
            let client = DebugAdapterClient::new(
                client_id,
                config,
                move |message, cx| {
                    dap_store
                        .update(cx, |_, cx| {
                            cx.emit(DapStoreEvent::DebugClientEvent { client_id, message })
                        })
                        .log_err();
                },
                &mut cx,
            )
            .await
            .log_err()?;

            this.update(&mut cx, |store, cx| {
                let handle = store
                    .clients
                    .get_mut(&client_id)
                    .with_context(|| "Failed to find starting debug client")?;

                *handle = DebugAdapterClientState::Running(client.clone());

                cx.emit(DapStoreEvent::DebugClientStarted(client_id));

                anyhow::Ok(())
            })
            .log_err();

            Some(client)
        });

        self.clients.insert(
            client_id,
            DebugAdapterClientState::Starting(start_client_task),
        );
    }

    pub fn initialize(
        &mut self,
        client_id: &DebugAdapterClientId,
        cx: &mut ModelContext<Self>,
    ) -> Task<Result<()>> {
        let Some(client) = self.client_by_id(client_id) else {
            return Task::ready(Err(anyhow!("Could not found client")));
        };

        cx.spawn(|this, mut cx| async move {
            let capabilities = client
                .request::<Initialize>(InitializeRequestArguments {
                    client_id: Some("zed".to_owned()),
                    client_name: Some("Zed".to_owned()),
                    adapter_id: client.adapter().id(),
                    locale: Some("en-US".to_owned()),
                    path_format: Some(InitializeRequestArgumentsPathFormat::Path),
                    supports_variable_type: Some(true),
                    supports_variable_paging: Some(false),
                    supports_run_in_terminal_request: Some(false),
                    supports_memory_references: Some(true),
                    supports_progress_reporting: Some(false),
                    supports_invalidated_event: Some(false),
                    lines_start_at1: Some(true),
                    columns_start_at1: Some(true),
                    supports_memory_event: Some(false),
                    supports_args_can_be_interpreted_by_shell: Some(true),
                    supports_start_debugging_request: Some(true),
                })
                .await?;

            this.update(&mut cx, |store, cx| {
                store.capabilities.insert(client.id(), capabilities);

                cx.notify();
            })?;

            // send correct request based on adapter config
            match client.config().request {
                DebugRequestType::Launch => {
                    client
                        .request::<Launch>(LaunchRequestArguments {
                            raw: client.request_args(),
                        })
                        .await?
                }
                DebugRequestType::Attach => {
                    client
                        .request::<Attach>(AttachRequestArguments {
                            raw: client.request_args(),
                        })
                        .await?
                }
            }

            anyhow::Ok(())
        })
    }

    pub fn send_configuration_done(
        &self,
        client_id: &DebugAdapterClientId,
        cx: &mut ModelContext<Self>,
    ) -> Task<Result<()>> {
        let Some(client) = self.client_by_id(client_id) else {
            return Task::ready(Err(anyhow!("Could not found client")));
        };

        let capabilities = self.capabilities_by_id(client_id);

        cx.spawn(|_, _| async move {
            let support_configuration_done_request = capabilities
                .supports_configuration_done_request
                .unwrap_or_default();

            if support_configuration_done_request {
                client
                    .request::<ConfigurationDone>(ConfigurationDoneArguments)
                    .await
            } else {
                Ok(())
            }
        })
    }

    pub fn continue_thread(
        &mut self,
        client_id: &DebugAdapterClientId,
        thread_id: u64,
        cx: &mut ModelContext<Self>,
    ) -> Task<Result<()>> {
        let Some(client) = self.client_by_id(client_id) else {
            return Task::ready(Err(anyhow!("Could not found client")));
        };

        cx.spawn(|_, _| async move {
            client
                .request::<Continue>(ContinueArguments {
                    thread_id,
                    single_thread: Some(true),
                })
                .await?;

            Ok(())
        })
    }

    pub fn step_over(
        &mut self,
        client_id: &DebugAdapterClientId,
        thread_id: u64,
        granularity: SteppingGranularity,
        cx: &mut ModelContext<Self>,
    ) -> Task<Result<()>> {
        let Some(client) = self.client_by_id(client_id) else {
            return Task::ready(Err(anyhow!("Could not found client")));
        };

        let capabilities = self.capabilities_by_id(client_id);

        let supports_single_thread_execution_requests = capabilities
            .supports_single_thread_execution_requests
            .unwrap_or_default();
        let supports_stepping_granularity = capabilities
            .supports_stepping_granularity
            .unwrap_or_default();

        cx.spawn(|_, _| async move {
            client
                .request::<Next>(NextArguments {
                    thread_id,
                    granularity: supports_stepping_granularity.then(|| granularity),
                    single_thread: supports_single_thread_execution_requests.then(|| true),
                })
                .await
        })
    }

    pub fn step_in(
        &mut self,
        client_id: &DebugAdapterClientId,
        thread_id: u64,
        granularity: SteppingGranularity,
        cx: &mut ModelContext<Self>,
    ) -> Task<Result<()>> {
        let Some(client) = self.client_by_id(client_id) else {
            return Task::ready(Err(anyhow!("Could not found client")));
        };

        let capabilities = self.capabilities_by_id(client_id);

        let supports_single_thread_execution_requests = capabilities
            .supports_single_thread_execution_requests
            .unwrap_or_default();
        let supports_stepping_granularity = capabilities
            .supports_stepping_granularity
            .unwrap_or_default();

        cx.spawn(|_, _| async move {
            client
                .request::<StepIn>(StepInArguments {
                    thread_id,
                    granularity: supports_stepping_granularity.then(|| granularity),
                    single_thread: supports_single_thread_execution_requests.then(|| true),
                    target_id: None,
                })
                .await
        })
    }

    pub fn step_out(
        &mut self,
        client_id: &DebugAdapterClientId,
        thread_id: u64,
        granularity: SteppingGranularity,
        cx: &mut ModelContext<Self>,
    ) -> Task<Result<()>> {
        let Some(client) = self.client_by_id(client_id) else {
            return Task::ready(Err(anyhow!("Could not found client")));
        };

        let capabilities = self.capabilities_by_id(client_id);

        let supports_single_thread_execution_requests = capabilities
            .supports_single_thread_execution_requests
            .unwrap_or_default();
        let supports_stepping_granularity = capabilities
            .supports_stepping_granularity
            .unwrap_or_default();

        cx.spawn(|_, _| async move {
            client
                .request::<StepOut>(StepOutArguments {
                    thread_id,
                    granularity: supports_stepping_granularity.then(|| granularity),
                    single_thread: supports_single_thread_execution_requests.then(|| true),
                })
                .await
        })
    }

    pub fn pause_thread(
        &mut self,
        client_id: &DebugAdapterClientId,
        thread_id: u64,
        cx: &mut ModelContext<Self>,
    ) -> Task<Result<()>> {
        let Some(client) = self.client_by_id(client_id) else {
            return Task::ready(Err(anyhow!("Could not found client")));
        };

        cx.spawn(|_, _| async move { client.request::<Pause>(PauseArguments { thread_id }).await })
    }

    pub fn terminate_threads(
        &mut self,
        client_id: &DebugAdapterClientId,
        thread_ids: Option<Vec<u64>>,
        cx: &mut ModelContext<Self>,
    ) -> Task<Result<()>> {
        let Some(client) = self.client_by_id(client_id) else {
            return Task::ready(Err(anyhow!("Could not found client")));
        };

        let capabilities = self.capabilities_by_id(client_id);

        if capabilities
            .supports_terminate_threads_request
            .unwrap_or_default()
        {
            cx.spawn(|_, _| async move {
                client
                    .request::<TerminateThreads>(TerminateThreadsArguments { thread_ids })
                    .await
            })
        } else {
            self.shutdown_client(client_id, cx)
        }
    }

    pub fn disconnect_client(
        &mut self,
        client_id: &DebugAdapterClientId,
        cx: &mut ModelContext<Self>,
    ) -> Task<Result<()>> {
        let Some(client) = self.client_by_id(client_id) else {
            return Task::ready(Err(anyhow!("Could not found client")));
        };

        cx.spawn(|_, _| async move {
            client
                .request::<Disconnect>(DisconnectArguments {
                    restart: Some(false),
                    terminate_debuggee: Some(true),
                    suspend_debuggee: Some(false),
                })
                .await
        })
    }

    pub fn restart(
        &mut self,
        client_id: &DebugAdapterClientId,
        args: Option<Value>,
        cx: &mut ModelContext<Self>,
    ) -> Task<Result<()>> {
        let Some(client) = self.client_by_id(client_id) else {
            return Task::ready(Err(anyhow!("Could not found client")));
        };

        let restart_args = args.unwrap_or(Value::Null);

        cx.spawn(|_, _| async move {
            client
                .request::<Disconnect>(DisconnectArguments {
                    restart: Some(true),
                    terminate_debuggee: Some(false),
                    suspend_debuggee: Some(false),
                })
                .await?;

            match client.request_type() {
                DebugRequestType::Launch => {
                    client
                        .request::<Launch>(LaunchRequestArguments { raw: restart_args })
                        .await?
                }
                DebugRequestType::Attach => {
                    client
                        .request::<Attach>(AttachRequestArguments { raw: restart_args })
                        .await?
                }
            }

            Ok(())
        })
    }

    fn shutdown_clients(&mut self, cx: &mut ModelContext<Self>) -> impl Future<Output = ()> {
        let mut tasks = Vec::new();

        let client_ids = self.clients.keys().cloned().collect::<Vec<_>>();
        for client_id in client_ids {
            tasks.push(self.shutdown_client(&client_id, cx));
        }

        async move {
            futures::future::join_all(tasks).await;
        }
    }

    pub fn shutdown_client(
        &mut self,
        client_id: &DebugAdapterClientId,
        cx: &mut ModelContext<Self>,
    ) -> Task<Result<()>> {
        let Some(debug_client) = self.clients.remove(&client_id) else {
            return Task::ready(Err(anyhow!("Could not found client")));
        };

        cx.emit(DapStoreEvent::DebugClientStopped(*client_id));

        self.capabilities.remove(client_id);

        cx.notify();

        cx.spawn(|_, _| async move {
            match debug_client {
                DebugAdapterClientState::Starting(task) => {
                    if let Some(client) = task.await {
                        client.shutdown().await?;
                    }
                }
                DebugAdapterClientState::Running(client) => client.shutdown().await?,
            };

            anyhow::Ok(())
        })
    }

    pub fn toggle_breakpoint_for_buffer(
        &mut self,
        project_path: &ProjectPath,
        breakpoint: Breakpoint,
        buffer_path: PathBuf,
        buffer_snapshot: BufferSnapshot,
        cx: &mut ModelContext<Self>,
    ) {
        let breakpoint_set = self.breakpoints.entry(project_path.clone()).or_default();

        if !breakpoint_set.remove(&breakpoint) {
            breakpoint_set.insert(breakpoint);
        }

        self.send_changed_breakpoints(project_path, buffer_path, buffer_snapshot, cx);
    }

    pub fn send_changed_breakpoints(
        &self,
        project_path: &ProjectPath,
        buffer_path: PathBuf,
        buffer_snapshot: BufferSnapshot,
        cx: &mut ModelContext<Self>,
    ) {
        let clients = self.running_clients().collect::<Vec<_>>();

        if clients.is_empty() {
            return;
        }

        let Some(breakpoints) = self.breakpoints.get(project_path) else {
            return;
        };

        let source_breakpoints = breakpoints
            .iter()
            .map(|bp| bp.source_for_snapshot(&buffer_snapshot))
            .collect::<Vec<_>>();

        let mut tasks = Vec::new();
        for client in clients {
            let buffer_path = buffer_path.clone();
            let source_breakpoints = source_breakpoints.clone();
            tasks.push(async move {
                client
                    .set_breakpoints(Arc::from(buffer_path), source_breakpoints)
                    .await
            });
        }

        cx.background_executor()
            .spawn(async move { futures::future::join_all(tasks).await })
            .detach()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Breakpoint {
    pub active_position: Option<text::Anchor>,
    pub cache_position: u32,
}

impl Breakpoint {
    pub fn to_source_breakpoint(&self, buffer: &Buffer) -> SourceBreakpoint {
        let line = self
            .active_position
            .map(|position| buffer.summary_for_anchor::<Point>(&position).row)
            .unwrap_or(self.cache_position) as u64
            + 1u64;

        SourceBreakpoint {
            line,
            condition: None,
            hit_condition: None,
            log_message: None,
            column: None,
            mode: None,
        }
    }

    pub fn set_active_position(&mut self, buffer: &Buffer) {
        if self.active_position.is_none() {
            let bias = if self.cache_position == 0 {
                text::Bias::Right
            } else {
                text::Bias::Left
            };

            self.active_position = Some(buffer.anchor_at(Point::new(self.cache_position, 0), bias));
        }
    }

    pub fn point_for_buffer(&self, buffer: &Buffer) -> Point {
        self.active_position
            .map(|position| buffer.summary_for_anchor::<Point>(&position))
            .unwrap_or(Point::new(self.cache_position, 0))
    }

    pub fn point_for_buffer_snapshot(&self, buffer_snapshot: &BufferSnapshot) -> Point {
        self.active_position
            .map(|position| buffer_snapshot.summary_for_anchor::<Point>(&position))
            .unwrap_or(Point::new(self.cache_position, 0))
    }

    pub fn source_for_snapshot(&self, snapshot: &BufferSnapshot) -> SourceBreakpoint {
        let line = self
            .active_position
            .map(|position| snapshot.summary_for_anchor::<Point>(&position).row)
            .unwrap_or(self.cache_position) as u64
            + 1u64;

        SourceBreakpoint {
            line,
            condition: None,
            hit_condition: None,
            log_message: None,
            column: None,
            mode: None,
        }
    }

    pub fn to_serialized(&self, buffer: Option<&Buffer>, path: Arc<Path>) -> SerializedBreakpoint {
        match buffer {
            Some(buffer) => SerializedBreakpoint {
                position: self
                    .active_position
                    .map(|position| buffer.summary_for_anchor::<Point>(&position).row + 1u32)
                    .unwrap_or(self.cache_position + 1u32),
                path,
            },
            None => SerializedBreakpoint {
                position: self.cache_position + 1u32,
                path,
            },
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SerializedBreakpoint {
    pub position: u32,
    pub path: Arc<Path>,
}

impl SerializedBreakpoint {
    pub fn to_source_breakpoint(&self) -> SourceBreakpoint {
        SourceBreakpoint {
            line: self.position as u64,
            condition: None,
            hit_condition: None,
            log_message: None,
            column: None,
            mode: None,
        }
    }
}
