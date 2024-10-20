pub use crate::transport::IoKind;
use crate::transport::{IoHandler, Transport};
use anyhow::{anyhow, Result};

use dap_types::{
    messages::{Message, Response},
    requests::Request,
};
use futures::{AsyncBufRead, AsyncBufReadExt as _, AsyncWrite};
use gpui::{AppContext, AsyncAppContext};
use parking_lot::Mutex;
use serde_json::Value;
use smol::{
    channel::{bounded, Receiver, Sender},
    process::Child,
};
use std::{
    hash::Hash,
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

pub struct DebugAdapterClient {
    id: DebugAdapterClientId,
    adapter_id: String,
    request_args: Value,
    transport: Arc<Transport>,
    _process: Arc<Mutex<Option<Child>>>,
    sequence_count: AtomicU64,
    config: DebugAdapterConfig,
    io_handlers: Arc<Mutex<Vec<IoHandler>>>,
    io_log_handlers: Arc<Mutex<Vec<IoHandler>>>,
}

pub struct TransportParams {
    input: Box<dyn AsyncWrite + Unpin + Send>,
    output: Box<dyn AsyncBufRead + Unpin + Send>,
    stdout: Option<Box<dyn AsyncBufRead + Unpin + Send>>,
    stderr: Box<dyn AsyncBufRead + Unpin + Send>,
    process: Child,
}

impl TransportParams {
    /// Defines communication channels for the debug adapter and logging:
    /// - `input` and `output`: Communication with the debug adapter
    /// - `stdin`, `stdout`, and `stderr`: Capture and log adapter process I/O
    pub fn new(
        input: Box<dyn AsyncWrite + Unpin + Send>,
        output: Box<dyn AsyncBufRead + Unpin + Send>,
        stdout: Option<Box<dyn AsyncBufRead + Unpin + Send>>,
        stderr: Box<dyn AsyncBufRead + Unpin + Send>,
        process: Child,
    ) -> Self {
        TransportParams {
            input,
            output,
            stdout,
            stderr,
            process,
        }
    }
}

impl DebugAdapterClient {
    pub async fn new<F>(
        id: DebugAdapterClientId,
        adapter_id: String,
        request_args: Value,
        config: DebugAdapterConfig,
        transport_params: TransportParams,
        event_handler: F,
        cx: &mut AsyncAppContext,
    ) -> Result<Arc<Self>>
    where
        F: FnMut(Message, &mut AppContext) + 'static + Send + Sync + Clone,
    {
        let io_handlers = Arc::new(Mutex::new(Vec::new()));
        let io_log_handlers = Arc::new(Mutex::new(Vec::new()));

        if let Some(stdout) = transport_params.stdout {
            Self::handle_adapter_logs(stdout, io_log_handlers.clone(), cx);
        }

        let transport = Self::handle_transport(
            transport_params.input,
            transport_params.output,
            transport_params.stderr,
            event_handler,
            io_handlers.clone(),
            io_log_handlers.clone(),
            cx,
        );
        Ok(Arc::new(Self {
            id,
            config,
            transport,
            adapter_id,
            io_handlers,
            request_args,
            io_log_handlers,
            sequence_count: AtomicU64::new(1),
            _process: Arc::new(Mutex::new(Some(transport_params.process))),
        }))
    }

    pub fn handle_transport<F>(
        stdin: Box<dyn AsyncWrite + Unpin + Send>,
        stdout: Box<dyn AsyncBufRead + Unpin + Send>,
        stderr: Box<dyn AsyncBufRead + Unpin + Send>,
        event_handler: F,
        io_handlers: Arc<Mutex<Vec<IoHandler>>>,
        io_log_handlers: Arc<Mutex<Vec<IoHandler>>>,
        cx: &mut AsyncAppContext,
    ) -> Arc<Transport>
    where
        F: FnMut(Message, &mut AppContext) + 'static + Send + Sync + Clone,
    {
        let transport = Transport::start(stdin, stdout, stderr, io_handlers, io_log_handlers, cx);

        let server_rx = transport.server_rx.clone();
        let server_tr = transport.server_tx.clone();
        cx.spawn(|mut cx| async move {
            Self::handle_recv(server_rx, server_tr, event_handler, &mut cx).await
        })
        .detach();

        transport
    }

    fn handle_adapter_logs(
        stdout: Box<dyn AsyncBufRead + Unpin + Send>,
        io_log_handlers: Arc<Mutex<Vec<IoHandler>>>,
        cx: &mut AsyncAppContext,
    ) {
        cx.spawn({
            let io_log_handlers = io_log_handlers.clone();
            |_| async move {
                use futures::stream::StreamExt;

                let mut lines = stdout.lines();
                while let Some(Ok(line)) = lines.next().await {
                    for handler in io_log_handlers.lock().iter_mut() {
                        handler(IoKind::StdOut, line.as_ref());
                    }
                }
            }
        })
        .detach();
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

        self.transport
            .current_requests
            .lock()
            .await
            .insert(sequence_id, callback_tx);

        self.respond(Message::Request(request)).await?;

        let response = callback_rx.recv().await??;

        match response.success {
            true => Ok(serde_json::from_value(response.body.unwrap_or_default())?),
            false => Err(anyhow!("Request failed")),
        }
    }

    pub async fn respond(&self, message: Message) -> Result<()> {
        self.transport
            .server_tx
            .send(message)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send response back: {}", e))
    }

    pub fn has_adapter_logs(&self) -> bool {
        true // TODO debugger: fix this when we move code to the transport layer
    }

    pub fn id(&self) -> DebugAdapterClientId {
        self.id
    }

    pub fn config(&self) -> DebugAdapterConfig {
        self.config.clone()
    }

    pub fn adapter_id(&self) -> String {
        self.adapter_id.clone()
    }

    pub fn request_args(&self) -> Value {
        self.request_args.clone()
    }

    pub fn request_type(&self) -> DebugRequestType {
        self.config.request.clone()
    }

    /// Get the next sequence id to be used in a request
    pub fn next_sequence_id(&self) -> u64 {
        self.sequence_count.fetch_add(1, Ordering::Relaxed)
    }

    pub async fn shutdown(&self) -> Result<()> {
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

    pub fn on_io<F>(&self, f: F)
    where
        F: 'static + Send + FnMut(IoKind, &str),
    {
        let mut io_handlers = self.io_handlers.lock();
        io_handlers.push(Box::new(f));
    }

    pub fn on_log_io<F>(&self, f: F)
    where
        F: 'static + Send + FnMut(IoKind, &str),
    {
        let mut io_log_handlers = self.io_log_handlers.lock();
        io_log_handlers.push(Box::new(f));
    }
}
