use crate::transport::Transport;
use anyhow::{anyhow, Context, Result};

use crate::adapters::{build_adapter, DebugAdapter};
use dap_types::{
    messages::{Message, Response},
    requests::{
        Attach, Continue, Disconnect, Launch, Next, Request, SetBreakpoints, StepBack, StepIn,
        StepOut, Terminate, Variables,
    },
    AttachRequestArguments, ContinueArguments, ContinueResponse, DisconnectArguments,
    LaunchRequestArguments, NextArguments, Scope, SetBreakpointsArguments, SetBreakpointsResponse,
    Source, SourceBreakpoint, StackFrame, StepBackArguments, StepInArguments, StepOutArguments,
    SteppingGranularity, TerminateArguments, Variable, VariablesArguments,
};
use futures::{AsyncBufRead, AsyncWrite};
use gpui::{AppContext, AsyncAppContext};
use parking_lot::{Mutex, MutexGuard};
use serde_json::Value;
use smol::{
    channel::{bounded, Receiver, Sender},
    process::Child,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    hash::Hash,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use task::{DebugAdapterConfig, DebugRequestType};

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ThreadStatus {
    #[default]
    Running,
    Stopped,
    Exited,
    Ended,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct DebugAdapterClientId(pub usize);

#[derive(Debug, Clone)]
pub struct VariableContainer {
    pub container_reference: u64,
    pub variable: Variable,
    pub depth: usize,
}

#[derive(Debug, Default, Clone)]
pub struct ThreadState {
    pub status: ThreadStatus,
    pub stack_frames: Vec<StackFrame>,
    /// HashMap<stack_frame_id, Vec<Scope>>
    pub scopes: HashMap<u64, Vec<Scope>>,
    /// BTreeMap<scope.variables_reference, Vec<VariableContainer>>
    pub variables: BTreeMap<u64, Vec<VariableContainer>>,
    pub fetched_variable_ids: HashSet<u64>,
    // we update this value only once we stopped,
    // we will use this to indicated if we should show a warning when debugger thread was exited
    pub stopped: bool,
}

pub struct DebugAdapterClient {
    id: DebugAdapterClientId,
    adapter: Arc<Box<dyn DebugAdapter>>,
    transport: Arc<Transport>,
    _process: Arc<Mutex<Option<Child>>>,
    sequence_count: AtomicU64,
    config: DebugAdapterConfig,
    /// thread_id -> thread_state
    thread_states: Arc<Mutex<HashMap<u64, ThreadState>>>,
    capabilities: Arc<Mutex<Option<dap_types::Capabilities>>>,
}

pub struct TransportParams {
    rx: Box<dyn AsyncBufRead + Unpin + Send>,
    tx: Box<dyn AsyncWrite + Unpin + Send>,
    err: Option<Box<dyn AsyncBufRead + Unpin + Send>>,
    process: Option<Child>,
}

impl TransportParams {
    pub fn new(
        rx: Box<dyn AsyncBufRead + Unpin + Send>,
        tx: Box<dyn AsyncWrite + Unpin + Send>,
        err: Option<Box<dyn AsyncBufRead + Unpin + Send>>,
        process: Option<Child>,
    ) -> Self {
        TransportParams {
            rx,
            tx,
            err,
            process,
        }
    }
}

impl DebugAdapterClient {
    pub async fn new<F>(
        id: DebugAdapterClientId,
        config: DebugAdapterConfig,
        event_handler: F,
        cx: &mut AsyncAppContext,
    ) -> Result<Arc<Self>>
    where
        F: FnMut(Message, &mut AppContext) + 'static + Send + Sync + Clone,
    {
        let adapter = Arc::new(build_adapter(&config).context("Creating debug adapter")?);
        let transport_params = adapter.connect(cx).await?;

        let transport = Self::handle_transport(
            transport_params.rx,
            transport_params.tx,
            transport_params.err,
            event_handler,
            cx,
        );

        Ok(Arc::new(Self {
            id,
            config,
            adapter,
            transport,
            capabilities: Default::default(),
            thread_states: Default::default(),
            sequence_count: AtomicU64::new(1),
            _process: Arc::new(Mutex::new(transport_params.process)),
        }))
    }

    pub fn handle_transport<F>(
        rx: Box<dyn AsyncBufRead + Unpin + Send>,
        tx: Box<dyn AsyncWrite + Unpin + Send>,
        err: Option<Box<dyn AsyncBufRead + Unpin + Send>>,
        event_handler: F,
        cx: &mut AsyncAppContext,
    ) -> Arc<Transport>
    where
        F: FnMut(Message, &mut AppContext) + 'static + Send + Sync + Clone,
    {
        let transport = Transport::start(rx, tx, err, cx);

        let server_rx = transport.server_rx.clone();
        let server_tr = transport.server_tx.clone();
        cx.spawn(|mut cx| async move {
            Self::handle_recv(server_rx, server_tr, event_handler, &mut cx).await
        })
        .detach();

        transport
    }

    async fn handle_recv<F>(
        server_rx: Receiver<Message>,
        client_tx: Sender<Message>,
        mut event_handler: F,
        cx: &mut AsyncAppContext,
    ) -> Result<()>
    where
        F: FnMut(Message, &mut AppContext) + 'static + Send + Sync + Clone,
    {
        while let Ok(payload) = server_rx.recv().await {
            match payload {
                Message::Event(ev) => cx.update(|cx| event_handler(Message::Event(ev), cx))?,
                Message::Response(_) => unreachable!(),
                Message::Request(req) => {
                    cx.update(|cx| event_handler(Message::Request(req), cx))?
                }
            };
        }

        drop(client_tx);

        anyhow::Ok(())
    }

    /// Send a request to an adapter and get a response back
    /// Note: This function will block until a response is sent back from the adapter
    pub async fn request<R: Request>(&self, arguments: R::Arguments) -> Result<R::Response> {
        let serialized_arguments = serde_json::to_value(arguments)?;

        let (callback_tx, callback_rx) = bounded::<Result<Response>>(1);

        let sequence_id = self.next_sequence_id();

        let request = crate::messages::Request {
            seq: sequence_id,
            command: R::COMMAND.to_string(),
            arguments: Some(serialized_arguments),
        };

        {
            self.transport
                .current_requests
                .lock()
                .await
                .insert(sequence_id, callback_tx);
        }

        self.transport
            .server_tx
            .send(Message::Request(request))
            .await?;

        let response = callback_rx.recv().await??;

        match response.success {
            true => Ok(serde_json::from_value(response.body.unwrap_or_default())?),
            false => Err(anyhow!("Request failed")),
        }
    }

    pub fn id(&self) -> DebugAdapterClientId {
        self.id
    }

    pub fn config(&self) -> DebugAdapterConfig {
        self.config.clone()
    }

    pub fn adapter(&self) -> Arc<Box<dyn DebugAdapter>> {
        self.adapter.clone()
    }

    pub fn request_args(&self) -> Value {
        self.adapter.request_args()
    }

    pub fn request_type(&self) -> DebugRequestType {
        self.config.request.clone()
    }

    pub fn capabilities(&self) -> dap_types::Capabilities {
        self.capabilities.lock().clone().unwrap_or_default()
    }

    /// Get the next sequence id to be used in a request
    pub fn next_sequence_id(&self) -> u64 {
        self.sequence_count.fetch_add(1, Ordering::Relaxed)
    }

    pub fn update_thread_state_status(&self, thread_id: u64, status: ThreadStatus) {
        if let Some(thread_state) = self.thread_states().get_mut(&thread_id) {
            thread_state.status = status;
        };
    }

    pub fn thread_states(&self) -> MutexGuard<HashMap<u64, ThreadState>> {
        self.thread_states.lock()
    }

    pub fn thread_state_by_id(&self, thread_id: u64) -> ThreadState {
        self.thread_states.lock().get(&thread_id).cloned().unwrap()
    }

    pub async fn launch(&self, args: Option<Value>) -> Result<()> {
        self.request::<Launch>(LaunchRequestArguments {
            raw: args.unwrap_or(Value::Null),
        })
        .await
    }

    pub async fn attach(&self, args: Option<Value>) -> Result<()> {
        self.request::<Attach>(AttachRequestArguments {
            raw: args.unwrap_or(Value::Null),
        })
        .await
    }

    pub async fn resume(&self, thread_id: u64) -> Result<ContinueResponse> {
        let supports_single_thread_execution_requests = self
            .capabilities()
            .supports_single_thread_execution_requests
            .unwrap_or_default();

        self.request::<Continue>(ContinueArguments {
            thread_id,
            single_thread: supports_single_thread_execution_requests.then(|| true),
        })
        .await
    }

    pub async fn step_over(&self, thread_id: u64, granularity: SteppingGranularity) -> Result<()> {
        let capabilities = self.capabilities();

        let supports_single_thread_execution_requests = capabilities
            .supports_single_thread_execution_requests
            .unwrap_or_default();
        let supports_stepping_granularity = capabilities
            .supports_stepping_granularity
            .unwrap_or_default();

        self.request::<Next>(NextArguments {
            thread_id,
            granularity: supports_stepping_granularity.then(|| granularity),
            single_thread: supports_single_thread_execution_requests.then(|| true),
        })
        .await
    }

    pub async fn step_in(&self, thread_id: u64, granularity: SteppingGranularity) -> Result<()> {
        let capabilities = self.capabilities();

        let supports_single_thread_execution_requests = capabilities
            .supports_single_thread_execution_requests
            .unwrap_or_default();
        let supports_stepping_granularity = capabilities
            .supports_stepping_granularity
            .unwrap_or_default();

        self.request::<StepIn>(StepInArguments {
            thread_id,
            target_id: None,
            granularity: supports_stepping_granularity.then(|| granularity),
            single_thread: supports_single_thread_execution_requests.then(|| true),
        })
        .await
    }

    pub async fn step_out(&self, thread_id: u64, granularity: SteppingGranularity) -> Result<()> {
        let capabilities = self.capabilities();

        let supports_single_thread_execution_requests = capabilities
            .supports_single_thread_execution_requests
            .unwrap_or_default();
        let supports_stepping_granularity = capabilities
            .supports_stepping_granularity
            .unwrap_or_default();

        self.request::<StepOut>(StepOutArguments {
            thread_id,
            granularity: supports_stepping_granularity.then(|| granularity),
            single_thread: supports_single_thread_execution_requests.then(|| true),
        })
        .await
    }

    pub async fn step_back(&self, thread_id: u64, granularity: SteppingGranularity) -> Result<()> {
        let capabilities = self.capabilities();

        let supports_single_thread_execution_requests = capabilities
            .supports_single_thread_execution_requests
            .unwrap_or_default();
        let supports_stepping_granularity = capabilities
            .supports_stepping_granularity
            .unwrap_or_default();

        self.request::<StepBack>(StepBackArguments {
            thread_id,
            granularity: supports_stepping_granularity.then(|| granularity),
            single_thread: supports_single_thread_execution_requests.then(|| true),
        })
        .await
    }

    pub async fn set_breakpoints(
        &self,
        absolute_file_path: Arc<Path>,
        breakpoints: Vec<SourceBreakpoint>,
    ) -> Result<SetBreakpointsResponse> {
        self.request::<SetBreakpoints>(SetBreakpointsArguments {
            source: Source {
                path: Some(String::from(absolute_file_path.to_string_lossy())),
                name: None,
                source_reference: None,
                presentation_hint: None,
                origin: None,
                sources: None,
                adapter_data: None,
                checksums: None,
            },
            breakpoints: Some(breakpoints),
            source_modified: None,
            lines: None,
        })
        .await
    }

    pub async fn shutdown(&self) -> Result<()> {
        let _ = self.terminate().await;

        self.transport.server_tx.close();
        self.transport.server_rx.close();

        let mut adapter = self._process.lock().take();

        async move {
            let mut current_requests = self.transport.current_requests.lock().await;
            let mut pending_requests = self.transport.pending_requests.lock().await;

            current_requests.clear();
            pending_requests.clear();

            if let Some(mut adapter) = adapter.take() {
                adapter.kill()?;
            }

            drop(current_requests);
            drop(pending_requests);
            drop(adapter);

            anyhow::Ok(())
        }
        .await
    }

    pub async fn terminate(&self) -> Result<()> {
        let support_terminate_request = self
            .capabilities()
            .supports_terminate_request
            .unwrap_or_default();

        if support_terminate_request {
            self.request::<Terminate>(TerminateArguments {
                restart: Some(false),
            })
            .await
        } else {
            self.request::<Disconnect>(DisconnectArguments {
                restart: Some(false),
                terminate_debuggee: Some(true),
                suspend_debuggee: Some(false),
            })
            .await
        }
    }

    pub async fn variables(&self, variables_reference: u64) -> Result<Vec<Variable>> {
        anyhow::Ok(
            self.request::<Variables>(VariablesArguments {
                variables_reference,
                filter: None,
                start: None,
                count: None,
                format: None,
            })
            .await?
            .variables,
        )
    }
}
