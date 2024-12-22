use collections::HashMap;
use dap_types::Capabilities;
use std::sync::Arc;
use task::DebugAdapterConfig;

use crate::client::{DebugAdapterClient, DebugAdapterClientId};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct DebugSessionId(pub usize);

pub struct DebugSession {
    id: DebugSessionId,
    configuration: DebugAdapterConfig,
    capabilities: HashMap<DebugAdapterClientId, Capabilities>,
    clients: HashMap<DebugAdapterClientId, Arc<DebugAdapterClient>>,
}

impl DebugSession {
    pub fn new(id: DebugSessionId, configuration: DebugAdapterConfig) -> Self {
        Self {
            id,
            configuration,
            clients: HashMap::default(),
            capabilities: HashMap::default(),
        }
    }

    pub fn id(&self) -> DebugSessionId {
        self.id
    }

    pub fn name(&self) -> String {
        self.configuration.label.clone()
    }

    pub fn configuration(&self) -> &DebugAdapterConfig {
        &self.configuration
    }

    pub fn capabilities(&self, client_id: &DebugAdapterClientId) -> Capabilities {
        self.capabilities
            .get(client_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn add_client(&mut self, client: Arc<DebugAdapterClient>) {
        self.clients.insert(client.id(), client);
    }

    pub fn remove_client(&mut self, client_id: &DebugAdapterClientId) {
        self.clients.remove(client_id);
    }

    pub fn client_by_id(
        &self,
        client_id: &DebugAdapterClientId,
    ) -> Option<Arc<DebugAdapterClient>> {
        self.clients.get(client_id).cloned()
    }

    pub fn clients(&self) -> impl Iterator<Item = Arc<DebugAdapterClient>> + '_ {
        self.clients.values().cloned()
    }
}
