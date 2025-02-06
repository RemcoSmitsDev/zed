use collections::{BTreeMap, HashMap};
use dap::{Module, ModuleEvent};
use gpui::{App, Context, Entity, Task, WeakEntity};
use std::{
    any::Any,
    collections::hash_map::Entry,
    hash::{Hash, Hasher},
    sync::Arc,
};
use task::DebugAdapterConfig;

use crate::{
    dap_command::{self, DapCommand},
    dap_store::DapStore,
};
use dap::client::{DebugAdapterClient, DebugAdapterClientId};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct DebugSessionId(pub usize);

impl DebugSessionId {
    pub fn from_proto(session_id: u64) -> Self {
        Self(session_id as usize)
    }

    pub fn to_proto(&self) -> u64 {
        self.0 as u64
    }
}

#[derive(Copy, Clone, PartialEq, PartialOrd)]
#[repr(transparent)]
struct ThreadId(u64);

struct Variable {
    variable: dap::Variable,
    variables: Vec<Variable>,
}

struct Scope {
    scope: dap::Scope,
    variables: Vec<Variable>,
}

struct StackFrame {
    stack_frame: dap::StackFrame,
    scopes: Vec<Scope>,
}

#[derive(Copy, Clone, Default, PartialEq, Eq)]
pub enum ThreadStatus {
    #[default]
    Running,
    Stopped,
    Exited,
    Ended,
}

struct Thread {
    thread: dap::Thread,
    stack_frames: Vec<StackFrame>,
    status: ThreadStatus,
    has_stopped: bool,
}

pub struct DebugAdapterClientState {
    dap_store: WeakEntity<DapStore>,
    client_id: DebugAdapterClientId,
    modules: Vec<dap::Module>,
    threads: BTreeMap<ThreadId, Thread>,
    requests: HashMap<RequestSlot, Task<anyhow::Result<()>>>,
}

trait CacheableCommand: 'static {
    fn as_any(&self) -> &dyn Any;
    fn dyn_eq(&self, rhs: &dyn CacheableCommand) -> bool;
    fn dyn_hash(&self, hasher: &mut dyn Hasher);
}

impl<T> CacheableCommand for T
where
    T: DapCommand + PartialEq + Hash,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn dyn_eq(&self, rhs: &dyn CacheableCommand) -> bool {
        rhs.as_any()
            .downcast_ref::<Self>()
            .map_or(false, |rhs| self == rhs)
    }
    fn dyn_hash(&self, mut hasher: &mut dyn Hasher) {
        T::hash(self, &mut hasher);
    }
}

pub(crate) struct RequestSlot(Arc<dyn CacheableCommand>);

impl<T: DapCommand + PartialEq + Hash> From<T> for RequestSlot {
    fn from(request: T) -> Self {
        Self(Arc::new(request))
    }
}

impl PartialEq for RequestSlot {
    fn eq(&self, other: &Self) -> bool {
        self.0.dyn_eq(other.0.as_ref())
    }
}

impl Eq for RequestSlot {}

impl Hash for RequestSlot {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.dyn_hash(state);
        self.0.as_any().type_id().hash(state);
    }
}

impl DebugAdapterClientState {
    pub(crate) fn wait_for_request(
        &self,
        request: impl Into<RequestSlot>,
    ) -> Option<&Task<anyhow::Result<()>>> {
        self.requests.get(&request.into())
    }

    pub fn modules(&mut self, cx: &mut Context<Self>) -> &[Module] {
        let slot = dap_command::ModulesCommand.into();
        let entry = self.requests.entry(slot);
        if let Entry::Vacant(vacant) = entry {
            let client_id = self.client_id;

            if let Ok(request) = self.dap_store.update(cx, |dap_store, cx| {
                let command = dap_command::ModulesCommand;

                dap_store.request_dap(&client_id, command, cx)
            }) {
                let task = cx.spawn(|this, mut cx| async move {
                    let result = request.await?;
                    this.update(&mut cx, |this, _| {
                        this.modules = result;
                    })?;
                    Ok(())
                });

                vacant.insert(task);
            }
        }

        &self.modules
    }

    pub fn handle_module_event(&mut self, event: &dap::ModuleEvent) {
        match event.reason {
            dap::ModuleEventReason::New => self.modules.push(event.module.clone()),
            dap::ModuleEventReason::Changed => {
                if let Some(module) = self.modules.iter_mut().find(|m| m.id == event.module.id) {
                    *module = event.module.clone();
                }
            }
            dap::ModuleEventReason::Removed => self.modules.retain(|m| m.id != event.module.id),
        }
    }
}

pub struct DebugSession {
    id: DebugSessionId,
    mode: DebugSessionMode,
    states: HashMap<DebugAdapterClientId, Entity<DebugAdapterClientState>>,
    ignore_breakpoints: bool,
}

pub enum DebugSessionMode {
    Local(LocalDebugSession),
    Remote(RemoteDebugSession),
}

pub struct LocalDebugSession {
    configuration: DebugAdapterConfig,
    clients: HashMap<DebugAdapterClientId, Arc<DebugAdapterClient>>,
}

impl LocalDebugSession {
    pub fn configuration(&self) -> &DebugAdapterConfig {
        &self.configuration
    }

    pub fn update_configuration(
        &mut self,
        f: impl FnOnce(&mut DebugAdapterConfig),
        cx: &mut Context<DebugSession>,
    ) {
        f(&mut self.configuration);
        cx.notify();
    }

    pub fn add_client(&mut self, client: Arc<DebugAdapterClient>, cx: &mut Context<DebugSession>) {
        self.clients.insert(client.id(), client);
        cx.notify();
    }

    pub fn remove_client(
        &mut self,
        client_id: &DebugAdapterClientId,
        cx: &mut Context<DebugSession>,
    ) -> Option<Arc<DebugAdapterClient>> {
        let client = self.clients.remove(client_id);
        cx.notify();

        client
    }

    pub fn client_by_id(
        &self,
        client_id: &DebugAdapterClientId,
    ) -> Option<Arc<DebugAdapterClient>> {
        self.clients.get(client_id).cloned()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn clients_len(&self) -> usize {
        self.clients.len()
    }

    pub fn clients(&self) -> impl Iterator<Item = Arc<DebugAdapterClient>> + '_ {
        self.clients.values().cloned()
    }

    pub fn client_ids(&self) -> impl Iterator<Item = DebugAdapterClientId> + '_ {
        self.clients.keys().cloned()
    }
}

pub struct RemoteDebugSession {
    label: String,
}

impl DebugSession {
    pub fn new_local(id: DebugSessionId, configuration: DebugAdapterConfig) -> Self {
        Self {
            id,
            ignore_breakpoints: false,
            states: HashMap::default(),
            mode: DebugSessionMode::Local(LocalDebugSession {
                configuration,
                clients: HashMap::default(),
            }),
        }
    }

    pub fn as_local(&self) -> Option<&LocalDebugSession> {
        match &self.mode {
            DebugSessionMode::Local(local) => Some(local),
            _ => None,
        }
    }

    pub fn as_local_mut(&mut self) -> Option<&mut LocalDebugSession> {
        match &mut self.mode {
            DebugSessionMode::Local(local) => Some(local),
            _ => None,
        }
    }

    pub fn new_remote(id: DebugSessionId, label: String, ignore_breakpoints: bool) -> Self {
        Self {
            id,
            ignore_breakpoints,
            states: HashMap::default(),
            mode: DebugSessionMode::Remote(RemoteDebugSession { label }),
        }
    }

    pub fn id(&self) -> DebugSessionId {
        self.id
    }

    pub fn name(&self) -> String {
        match &self.mode {
            DebugSessionMode::Local(local) => local.configuration.label.clone(),
            DebugSessionMode::Remote(remote) => remote.label.clone(),
        }
    }

    pub fn ignore_breakpoints(&self) -> bool {
        self.ignore_breakpoints
    }

    pub fn set_ignore_breakpoints(&mut self, ignore: bool, cx: &mut Context<Self>) {
        self.ignore_breakpoints = ignore;
        cx.notify();
    }

    pub fn client_state(
        &self,
        client_id: DebugAdapterClientId,
    ) -> Option<Entity<DebugAdapterClientState>> {
        self.states.get(&client_id).cloned()
    }
}
