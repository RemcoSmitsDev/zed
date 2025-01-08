use async_trait::async_trait;
use dap::{requests::Next, NextArguments};
use rpc::proto;

pub trait DapCommand: 'static + Sized + Send + std::fmt::Debug {
    type Response: 'static + Send + std::fmt::Debug;
    type DapRequest: 'static + Send + dap::requests::Request;
    type ProtoRequest: 'static + Send + proto::RequestMessage;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments;

    fn response_from_dap(
        self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Self::Response;
}

impl DapCommand for NextArguments {
    type Response = <Next as dap::requests::Request>::Response;
    type DapRequest = Next;
    type ProtoRequest = proto::PrepareRename;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        todo!()
    }

    fn response_from_dap(
        self,
        _message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Self::Response {
        todo!()
    }
}
