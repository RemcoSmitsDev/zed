use anyhow::Result;
use collections::HashMap;
use dap_types::Capabilities;
use gpui::{ModelContext, Task};
use std::sync::Arc;
use task::DebugAdapterConfig;

use crate::client::{DebugAdapterClient, DebugAdapterClientId};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct DebugSessionId(pub usize);

impl DebugSessionId {
    pub fn from_proto(client_id: u64) -> Self {
        Self(client_id as usize)
    }

    pub fn to_proto(&self) -> u64 {
        self.0 as u64
    }
}

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

    pub fn update_configuration(&mut self, f: impl FnOnce(&mut DebugAdapterConfig)) {
        f(&mut self.configuration);
    }

    pub fn capabilities(&self, client_id: &DebugAdapterClientId) -> Capabilities {
        self.capabilities
            .get(client_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn update_capabilities(
        &mut self,
        client_id: &DebugAdapterClientId,
        new_capabilities: Capabilities,
    ) {
        if let Some(capabilities) = self.capabilities.get_mut(client_id) {
            *capabilities = capabilities.merge(new_capabilities);
        } else {
            self.capabilities.insert(*client_id, new_capabilities);
        }
    }

    pub fn add_client(&mut self, client: Arc<DebugAdapterClient>) {
        self.clients.insert(client.id(), client);
    }

    pub fn remove_client(
        &mut self,
        client_id: &DebugAdapterClientId,
    ) -> Option<Arc<DebugAdapterClient>> {
        self.clients.remove(client_id)
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

    pub fn shutdown_clients(&self, cx: &mut ModelContext<Self>) -> Task<Result<()>> {
        let mut tasks = Vec::new();
        for client in self.clients.values() {
            tasks.push(cx.spawn({
                let client = client.clone();
                |_, _| async move { client.shutdown().await }
            }));
        }

        cx.background_executor().spawn(async move {
            futures::future::join_all(tasks).await;
            Ok(())
        })
    }
}
