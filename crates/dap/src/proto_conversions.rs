use client::proto::DapScope;

pub trait ProtoConversion {
    type DapType;

    fn to_proto(&self) -> Self::DapType;
    fn from_proto(payload: Self::DapType) -> Self;
}

impl ProtoConversion for dap_types::Scope {
    type DapType = DapScope;

    fn to_proto(&self) -> Self::DapType {
        Self::DapType {
            name: self.name.clone(),
            presentation_hint: Default::default(), //TODO Debugger Collab
            variables_reference: self.variables_reference,
            named_variables: self.named_variables,
            indexed_variables: self.indexed_variables,
            expensive: self.expensive,
            source: None, //TODO Debugger Collab
            line: self.line,
            end_line: self.end_line,
            column: self.column,
            end_column: self.end_column,
        }
    }

    fn from_proto(_payload: Self::DapType) -> Self {
        todo!()
    }
}
