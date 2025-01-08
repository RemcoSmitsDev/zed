use anyhow::Result;
use async_trait::async_trait;
use dap::{
    client::DebugAdapterClient,
    requests::{Next, Request},
    NextArguments,
};
use rpc::proto;
use std::sync::Arc;

#[async_trait(?Send)]
pub trait DapCommand<R: Request>: 'static + Sized + Send + std::fmt::Debug
where
    R: proto::RequestMessage,
{
    // fn to_proto(&self, arguments: R::Arguments) -> proto::RequestMessage;

    async fn to_dap_client(
        &self,
        arguments: R::Arguments,
        client: &Arc<DebugAdapterClient>,
    ) -> Result<<R as proto::RequestMessage>::Response>;
}

#[derive(Debug)]
struct NextProto {}

#[async_trait(?Send)]
impl DapCommand<Next> for NextProto {
    async fn to_dap_client(
        &self,
        arguments: NextArguments,
        client: &Arc<DebugAdapterClient>,
    ) -> Result<()> {
        client.request::<Next>(arguments).await
    }
}
