use anyhow::Result;
use dap::{
    client::DebugAdapterClientId, proto_conversions::ProtoConversion, requests::Next,
    NextArguments, SteppingGranularity,
};
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
pub struct StepCommand {
    pub thread_id: u64,
    pub granularity: Option<SteppingGranularity>,
    pub single_thread: Option<bool>,
}

impl StepCommand {
    fn from_proto(message: proto::DapStepRequest) -> Self {
        const LINE: i32 = proto::SteppingGranularity::Line as i32;
        const STATEMENT: i32 = proto::SteppingGranularity::Statement as i32;
        const INSTRUCTION: i32 = proto::SteppingGranularity::Instruction as i32;

        let granularity = message.granularity.map(|granularity| match granularity {
            LINE => SteppingGranularity::Line,
            INSTRUCTION => SteppingGranularity::Instruction,
            STATEMENT | _ => SteppingGranularity::Statement,
        });

        Self {
            thread_id: message.thread_id,
            granularity,
            single_thread: message.single_thread,
        }
    }

    fn to_proto(
        &self,
        debug_client_id: &DebugAdapterClientId,
        upstream_project_id: u64,
        target_id: Option<u64>,
    ) -> proto::DapStepRequest {
        proto::DapStepRequest {
            target_id,
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            thread_id: self.thread_id,
            single_thread: self.single_thread,
            granularity: self.granularity.map(|gran| gran.to_proto() as i32),
        }
    }
}

#[derive(Debug)]
pub(crate) struct NextCommand {
    pub inner: StepCommand,
}

impl DapCommand for NextCommand {
    type Response = <Next as dap::requests::Request>::Response;
    type DapRequest = Next;
    type ProtoRequest = proto::DapStepRequest;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> DebugAdapterClientId {
        DebugAdapterClientId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            inner: StepCommand::from_proto(request.clone()),
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
    ) -> proto::DapStepRequest {
        self.inner
            .to_proto(debug_client_id, upstream_project_id, None)
    }

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        NextArguments {
            thread_id: self.inner.thread_id,
            single_thread: self.inner.single_thread,
            granularity: self.inner.granularity,
        }
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
