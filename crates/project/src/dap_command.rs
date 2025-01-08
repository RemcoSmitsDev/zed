use anyhow::Result;
use dap::{client::DebugAdapterClientId, requests::Next, NextArguments};
use rpc::proto;

pub trait DapCommand: 'static + Sized + Send + std::fmt::Debug {
    type Response: 'static + Send + std::fmt::Debug;
    type DapRequest: 'static + Send + dap::requests::Request;
    type ProtoRequest: 'static + Send + proto::RequestMessage;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> DebugAdapterClientId;

    fn from_proto(request: &Self::ProtoRequest) -> Self;

    fn to_proto(
        &self,
        debug_client_id: &DebugAdapterClientId,
        upstream_project_id: u64,
    ) -> Self::ProtoRequest;

    fn response_to_proto(
        message: Self::Response,
    ) -> <Self::ProtoRequest as proto::RequestMessage>::Response;

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
    pub args: NextArguments,
}

impl DapCommand for NextCommand {
    type Response = <Next as dap::requests::Request>::Response;
    type DapRequest = Next;
    type ProtoRequest = proto::DapNextRequest;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> DebugAdapterClientId {
        DebugAdapterClientId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            args: NextArguments {
                thread_id: request.thread_id,
                single_thread: request.single_thread,
                granularity: None,
            },
        }
    }

    fn response_to_proto(
        _message: Self::Response,
    ) -> <Self::ProtoRequest as proto::RequestMessage>::Response {
        proto::Ack {}
    }

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
        _message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(())
    }

    fn response_from_proto(
        self,
        _message: <Self::ProtoRequest as proto::RequestMessage>::Response,
    ) -> Result<Self::Response> {
        Ok(())
    }
}

// #[derive(Debug)]
// pub(crate) struct ContinueCommand {
//     pub args: ContinueArguments,
// }

// impl DapCommand for ContinueCommand {
//     type Response = <Continue as dap::requests::Request>::Response;
//     type DapRequest = Continue;
//     type ProtoRequest = proto::DapContinueRequest;

//     fn to_proto(
//         &self,
//         debug_client_id: &DebugAdapterClientId,
//         upstream_project_id: u64,
//     ) -> proto::DapContinueRequest {
//         proto::DapContinueRequest {
//             project_id: upstream_project_id,
//             client_id: debug_client_id.to_proto(),
//             thread_id: self.args.thread_id,
//             single_thread: self.args.single_thread,
//             granularity: Some(match self.args.granularity {
//                 Some(dap::SteppingGranularity::Line) => proto::SteppingGranularity::Line.into(),
//                 Some(dap::SteppingGranularity::Instruction) => {
//                     proto::SteppingGranularity::Instruction.into()
//                 }
//                 Some(dap::SteppingGranularity::Statement) | None => {
//                     proto::SteppingGranularity::Statement.into()
//                 }
//             }),
//         }
//     }

//     fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
//         self.args.clone()
//     }

//     fn response_from_dap(
//         self,
//         _message: <Self::DapRequest as dap::requests::Request>::Response,
//     ) -> Result<Self::Response> {
//         Ok(())
//     }

//     fn response_from_proto(
//         self,
//         _message: <Self::ProtoRequest as proto::RequestMessage>::Response,
//     ) -> Result<Self::Response> {
//         Ok(())
//     }
// }
