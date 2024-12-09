use client::proto::{
    DapChecksum, DapChecksumAlgorithm, DapScope, DapScopePresentationHint, DapSource,
    DapSourcePresentationHint, DapStackPresentationHint, DapVariable,
};

pub trait ProtoConversion {
    type DapType;

    fn to_proto(&self) -> Self::DapType;
    fn from_proto(payload: Self::DapType) -> Self;
}

impl<T> ProtoConversion for Vec<T>
where
    T: ProtoConversion,
{
    type DapType = Vec<T::DapType>;

    fn to_proto(&self) -> Self::DapType {
        self.iter().map(|item| item.to_proto()).collect()
    }

    fn from_proto(payload: Self::DapType) -> Self {
        payload
            .into_iter()
            .map(|item| T::from_proto(item))
            .collect()
    }
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

    fn from_proto(payload: Self::DapType) -> Self {
        Self {
            name: payload.name,
            presentation_hint: DapScopePresentationHint::from_i32(payload.presentation_hint)
                .map(|val| dap_types::ScopePresentationHint::from_proto(val)),
            variables_reference: payload.variables_reference,
            named_variables: payload.named_variables,
            indexed_variables: payload.indexed_variables,
            expensive: payload.expensive,
            source: payload.source.map(|src| dap_types::Source::from_proto(src)),
            line: payload.line,
            end_line: payload.end_line,
            column: payload.column,
            end_column: payload.end_column,
        }
    }
}

impl ProtoConversion for dap_types::Variable {
    type DapType = DapVariable;

    fn to_proto(&self) -> Self::DapType {
        Self::DapType {
            name: self.name.clone(),
            value: self.value.clone(),
            r#type: self.type_.clone(),
            evaluate_name: self.evaluate_name.clone(),
            variables_reference: self.variables_reference,
            named_variables: self.named_variables,
            indexed_variables: self.indexed_variables,
            memory_reference: self.memory_reference.clone(),
        }
    }

    fn from_proto(payload: Self::DapType) -> Self {
        Self {
            name: payload.name,
            value: payload.value,
            type_: payload.r#type,
            evaluate_name: payload.evaluate_name,
            presentation_hint: None, // TODO Debugger Collab Add this
            variables_reference: payload.variables_reference,
            named_variables: payload.named_variables,
            indexed_variables: payload.indexed_variables,
            memory_reference: payload.memory_reference,
        }
    }
}

impl ProtoConversion for dap_types::ScopePresentationHint {
    type DapType = DapScopePresentationHint;

    fn to_proto(&self) -> Self::DapType {
        match self {
            dap_types::ScopePresentationHint::Locals => DapScopePresentationHint::Locals,
            dap_types::ScopePresentationHint::Arguments => DapScopePresentationHint::Arguments,
            dap_types::ScopePresentationHint::Registers => DapScopePresentationHint::Registers,
            dap_types::ScopePresentationHint::ReturnValue => DapScopePresentationHint::ReturnValue,
            dap_types::ScopePresentationHint::Unknown => DapScopePresentationHint::ScopeUnknown,
            &_ => unreachable!(),
        }
    }

    fn from_proto(payload: Self::DapType) -> Self {
        match payload {
            DapScopePresentationHint::Locals => dap_types::ScopePresentationHint::Locals,
            DapScopePresentationHint::Arguments => dap_types::ScopePresentationHint::Arguments,
            DapScopePresentationHint::Registers => dap_types::ScopePresentationHint::Registers,
            DapScopePresentationHint::ReturnValue => dap_types::ScopePresentationHint::ReturnValue,
            DapScopePresentationHint::ScopeUnknown => dap_types::ScopePresentationHint::Unknown,
        }
    }
}

impl ProtoConversion for dap_types::SourcePresentationHint {
    type DapType = DapSourcePresentationHint;

    fn to_proto(&self) -> Self::DapType {
        match self {
            dap_types::SourcePresentationHint::Normal => DapSourcePresentationHint::SourceNormal,
            dap_types::SourcePresentationHint::Emphasize => DapSourcePresentationHint::Emphasize,
            dap_types::SourcePresentationHint::Deemphasize => {
                DapSourcePresentationHint::Deemphasize
            }
            dap_types::SourcePresentationHint::Unknown => DapSourcePresentationHint::SourceUnknown,
            &_ => unreachable!(),
        }
    }

    fn from_proto(payload: Self::DapType) -> Self {
        match payload {
            DapSourcePresentationHint::SourceNormal => dap_types::SourcePresentationHint::Normal,
            DapSourcePresentationHint::Emphasize => dap_types::SourcePresentationHint::Emphasize,
            DapSourcePresentationHint::Deemphasize => {
                dap_types::SourcePresentationHint::Deemphasize
            }
            DapSourcePresentationHint::SourceUnknown => dap_types::SourcePresentationHint::Unknown,
        }
    }
}

impl ProtoConversion for dap_types::Checksum {
    type DapType = DapChecksum;

    fn to_proto(&self) -> Self::DapType {
        DapChecksum {
            algorithm: self.algorithm.to_proto().into(),
            checksum: self.checksum.clone(),
        }
    }

    fn from_proto(payload: Self::DapType) -> Self {
        Self {
            algorithm: dap_types::ChecksumAlgorithm::from_proto(payload.algorithm()),
            checksum: payload.checksum,
        }
    }
}

impl ProtoConversion for dap_types::ChecksumAlgorithm {
    type DapType = DapChecksumAlgorithm;

    fn to_proto(&self) -> Self::DapType {
        match self {
            dap_types::ChecksumAlgorithm::Md5 => DapChecksumAlgorithm::Md5,
            dap_types::ChecksumAlgorithm::Sha1 => DapChecksumAlgorithm::Sha1,
            dap_types::ChecksumAlgorithm::Sha256 => DapChecksumAlgorithm::Sha256,
            dap_types::ChecksumAlgorithm::Timestamp => DapChecksumAlgorithm::Timestamp,
            _ => unreachable!(),
        }
    }

    fn from_proto(payload: Self::DapType) -> Self {
        match payload {
            DapChecksumAlgorithm::Md5 => dap_types::ChecksumAlgorithm::Md5,
            DapChecksumAlgorithm::Sha1 => dap_types::ChecksumAlgorithm::Sha1,
            DapChecksumAlgorithm::Sha256 => dap_types::ChecksumAlgorithm::Sha256,
            DapChecksumAlgorithm::Timestamp => dap_types::ChecksumAlgorithm::Timestamp,
            DapChecksumAlgorithm::ChecksumAlgorithmUnspecified => unreachable!(),
        }
    }
}

impl ProtoConversion for dap_types::Source {
    type DapType = DapSource;

    fn to_proto(&self) -> Self::DapType {
        Self::DapType {
            name: self.name.clone(),
            path: self.path.clone(),
            source_reference: self.source_reference,
            presentation_hint: self.presentation_hint.map(|hint| hint.to_proto().into()),
            origin: self.origin.clone(),
            sources: self
                .sources
                .clone()
                .map(|src| src.to_proto())
                .unwrap_or_default(),
            adapter_data: Default::default(), // TODO: Need adapter_data field
            checksums: self
                .checksums
                .clone()
                .map(|c| c.to_proto())
                .unwrap_or_default(),
        }
    }

    fn from_proto(payload: Self::DapType) -> Self {
        Self {
            name: payload.name.clone(),
            path: payload.path.clone(),
            source_reference: payload.source_reference,
            presentation_hint: payload
                .presentation_hint
                .and_then(|val| DapSourcePresentationHint::from_i32(val))
                .map(|val| dap_types::SourcePresentationHint::from_proto(val)),
            origin: payload.origin.clone(),
            sources: Some(Vec::from_proto(payload.sources)),
            checksums: Some(Vec::from_proto(payload.checksums)),
            adapter_data: None,
        }
    }
}
