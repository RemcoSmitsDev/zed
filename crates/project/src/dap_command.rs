use anyhow::Result;
use dap::{client::DebugAdapterClientId, requests::Next, NextArguments};
use rpc::proto;

pub trait DapCommand: 'static + Sized + Send + std::fmt::Debug {
    type Response: 'static + Send + std::fmt::Debug;
    type DapRequest: 'static + Send + dap::requests::Request;
    type ProtoRequest: 'static + Send + proto::RequestMessage;

    fn to_proto(
        &self,
        debug_client_id: &DebugAdapterClientId,
        upstream_project_id: u64,
    ) -> Self::ProtoRequest;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments;

    fn response_from_dap(
        self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response>;

    fn response_from_proto(
        self,
        message: <Self::ProtoRequest as proto::RequestMessage>::Response,
    ) -> Result<Self::Response>;
}

#[derive(Debug)]
pub(crate) struct NextCommand {
    args: NextArguments,
}

impl DapCommand for NextCommand {
    type Response = <Next as dap::requests::Request>::Response;
    type DapRequest = Next;
    type ProtoRequest = proto::DapNextRequest;

    fn to_proto(
        &self,
        debug_client_id: &DebugAdapterClientId,
        upstream_project_id: u64,
    ) -> proto::DapNextRequest {
        proto::DapNextRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            thread_id: self.args.thread_id,
            single_thread: self.args.single_thread,
            granularity: Some(match self.args.granularity {
                Some(dap::SteppingGranularity::Line) => proto::SteppingGranularity::Line.into(),
                Some(dap::SteppingGranularity::Instruction) => {
                    proto::SteppingGranularity::Instruction.into()
                }
                Some(dap::SteppingGranularity::Statement) | None => {
                    proto::SteppingGranularity::Statement.into()
                }
            }),
        }
    }

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        self.args.clone()
    }

    fn response_from_dap(
        self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        todo!("")
    }

    fn response_from_proto(
        self,
        message: <Self::ProtoRequest as proto::RequestMessage>::Response,
    ) -> Result<Self::Response> {
        todo!("")
    }
}
