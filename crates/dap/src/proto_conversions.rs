use anyhow::{Context as _, Result};
use client::proto::{
    self, BreakpointMode, BreakpointModeApplicability, Capabilities, DapChecksum,
    DapChecksumAlgorithm, DapEvaluateContext, DapModule, DapScope, DapScopePresentationHint,
    DapSource, DapSourcePresentationHint, DapStackFrame, DapVariable, ExceptionBreakpointsFilter,
};
use dap_types::{OutputEventCategory, OutputEventGroup, ScopePresentationHint, Source};

pub trait ProtoConversion {
    type ProtoType;
    type Output;

    fn to_proto(&self) -> Self::ProtoType;
    fn from_proto(payload: Self::ProtoType) -> Self::Output;
}

impl<T> ProtoConversion for Vec<T>
where
    T: ProtoConversion<Output = T>,
{
    type ProtoType = Vec<T::ProtoType>;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        self.iter().map(|item| item.to_proto()).collect()
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        payload
            .into_iter()
            .map(|item| T::from_proto(item))
            .collect()
    }
}

impl ProtoConversion for dap_types::Scope {
    type ProtoType = DapScope;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        Self::ProtoType {
            name: self.name.clone(),
            presentation_hint: self
                .presentation_hint
                .as_ref()
                .map(|hint| hint.to_proto().into()),
            variables_reference: self.variables_reference,
            named_variables: self.named_variables,
            indexed_variables: self.indexed_variables,
            expensive: self.expensive,
            source: self.source.as_ref().map(Source::to_proto),
            line: self.line,
            end_line: self.end_line,
            column: self.column,
            end_column: self.end_column,
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        let presentation_hint = payload
            .presentation_hint
            .and_then(DapScopePresentationHint::from_i32);
        Self {
            name: payload.name,
            presentation_hint: presentation_hint.map(ScopePresentationHint::from_proto),
            variables_reference: payload.variables_reference,
            named_variables: payload.named_variables,
            indexed_variables: payload.indexed_variables,
            expensive: payload.expensive,
            source: payload.source.map(dap_types::Source::from_proto),
            line: payload.line,
            end_line: payload.end_line,
            column: payload.column,
            end_column: payload.end_column,
        }
    }
}

impl ProtoConversion for dap_types::Variable {
    type ProtoType = DapVariable;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        Self::ProtoType {
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

    fn from_proto(payload: Self::ProtoType) -> Self {
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
            declaration_location_reference: None, // TODO
            value_location_reference: None,       // TODO
        }
    }
}

impl ProtoConversion for dap_types::ScopePresentationHint {
    type ProtoType = DapScopePresentationHint;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        match self {
            dap_types::ScopePresentationHint::Locals => DapScopePresentationHint::Locals,
            dap_types::ScopePresentationHint::Arguments => DapScopePresentationHint::Arguments,
            dap_types::ScopePresentationHint::Registers => DapScopePresentationHint::Registers,
            dap_types::ScopePresentationHint::ReturnValue => DapScopePresentationHint::ReturnValue,
            dap_types::ScopePresentationHint::Unknown => DapScopePresentationHint::ScopeUnknown,
            _ => unreachable!(),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
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
    type ProtoType = DapSourcePresentationHint;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        match self {
            dap_types::SourcePresentationHint::Normal => DapSourcePresentationHint::SourceNormal,
            dap_types::SourcePresentationHint::Emphasize => DapSourcePresentationHint::Emphasize,
            dap_types::SourcePresentationHint::Deemphasize => {
                DapSourcePresentationHint::Deemphasize
            }
            dap_types::SourcePresentationHint::Unknown => DapSourcePresentationHint::SourceUnknown,
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
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
    type ProtoType = DapChecksum;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        DapChecksum {
            algorithm: self.algorithm.to_proto().into(),
            checksum: self.checksum.clone(),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        Self {
            algorithm: dap_types::ChecksumAlgorithm::from_proto(payload.algorithm()),
            checksum: payload.checksum,
        }
    }
}

impl ProtoConversion for dap_types::ChecksumAlgorithm {
    type ProtoType = DapChecksumAlgorithm;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        match self {
            dap_types::ChecksumAlgorithm::Md5 => DapChecksumAlgorithm::Md5,
            dap_types::ChecksumAlgorithm::Sha1 => DapChecksumAlgorithm::Sha1,
            dap_types::ChecksumAlgorithm::Sha256 => DapChecksumAlgorithm::Sha256,
            dap_types::ChecksumAlgorithm::Timestamp => DapChecksumAlgorithm::Timestamp,
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
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
    type ProtoType = DapSource;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        Self::ProtoType {
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
            adapter_data: Default::default(), // TODO Debugger Collab
            checksums: self
                .checksums
                .clone()
                .map(|c| c.to_proto())
                .unwrap_or_default(),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        Self {
            name: payload.name.clone(),
            path: payload.path.clone(),
            source_reference: payload.source_reference,
            presentation_hint: payload
                .presentation_hint
                .and_then(DapSourcePresentationHint::from_i32)
                .map(dap_types::SourcePresentationHint::from_proto),
            origin: payload.origin.clone(),
            sources: Some(Vec::<dap_types::Source>::from_proto(payload.sources)),
            checksums: Some(Vec::<dap_types::Checksum>::from_proto(payload.checksums)),
            adapter_data: None, // TODO Debugger Collab
        }
    }
}

impl ProtoConversion for dap_types::StackFrame {
    type ProtoType = DapStackFrame;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        Self::ProtoType {
            id: self.id,
            name: self.name.clone(),
            source: self.source.as_ref().map(|src| src.to_proto()),
            line: self.line,
            column: self.column,
            end_line: self.end_line,
            end_column: self.end_column,
            can_restart: self.can_restart,
            instruction_pointer_reference: self.instruction_pointer_reference.clone(),
            module_id: None,         // TODO Debugger Collab
            presentation_hint: None, // TODO Debugger Collab
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        Self {
            id: payload.id,
            name: payload.name,
            source: payload.source.map(dap_types::Source::from_proto),
            line: payload.line,
            column: payload.column,
            end_line: payload.end_line,
            end_column: payload.end_column,
            can_restart: payload.can_restart,
            instruction_pointer_reference: payload.instruction_pointer_reference,
            module_id: None,         // TODO Debugger Collab
            presentation_hint: None, // TODO Debugger Collab
        }
    }
}

impl ProtoConversion for dap_types::Module {
    type ProtoType = DapModule;
    type Output = Result<Self>;

    fn to_proto(&self) -> Self::ProtoType {
        let id = match &self.id {
            dap_types::ModuleId::Number(num) => proto::dap_module_id::Id::Number(*num),
            dap_types::ModuleId::String(string) => proto::dap_module_id::Id::String(string.clone()),
        };

        DapModule {
            id: Some(proto::DapModuleId { id: Some(id) }),
            name: self.name.clone(),
            path: self.path.clone(),
            is_optimized: self.is_optimized,
            is_user_code: self.is_user_code,
            version: self.version.clone(),
            symbol_status: self.symbol_status.clone(),
            symbol_file_path: self.symbol_file_path.clone(),
            date_time_stamp: self.date_time_stamp.clone(),
            address_range: self.address_range.clone(),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Result<Self> {
        let id = match payload
            .id
            .context("All DapModule proto messages must have an id")?
            .id
            .context("All DapModuleID proto messages must have an id")?
        {
            proto::dap_module_id::Id::String(string) => dap_types::ModuleId::String(string),
            proto::dap_module_id::Id::Number(num) => dap_types::ModuleId::Number(num),
        };

        Ok(Self {
            id,
            name: payload.name,
            path: payload.path,
            is_optimized: payload.is_optimized,
            is_user_code: payload.is_user_code,
            version: payload.version,
            symbol_status: payload.symbol_status,
            symbol_file_path: payload.symbol_file_path,
            date_time_stamp: payload.date_time_stamp,
            address_range: payload.address_range,
        })
    }
}

impl ProtoConversion for dap_types::SteppingGranularity {
    type ProtoType = proto::SteppingGranularity;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        match self {
            dap_types::SteppingGranularity::Statement => proto::SteppingGranularity::Statement,
            dap_types::SteppingGranularity::Line => proto::SteppingGranularity::Line,
            dap_types::SteppingGranularity::Instruction => proto::SteppingGranularity::Instruction,
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        match payload {
            proto::SteppingGranularity::Line => dap_types::SteppingGranularity::Line,
            proto::SteppingGranularity::Instruction => dap_types::SteppingGranularity::Instruction,
            proto::SteppingGranularity::Statement => dap_types::SteppingGranularity::Statement,
        }
    }
}

impl ProtoConversion for dap_types::OutputEventCategory {
    type ProtoType = proto::DapOutputCategory;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        match self {
            OutputEventCategory::Console => proto::DapOutputCategory::ConsoleOutput,
            OutputEventCategory::Important => proto::DapOutputCategory::Important,
            OutputEventCategory::Stdout => proto::DapOutputCategory::Stdout,
            OutputEventCategory::Stderr => proto::DapOutputCategory::Stderr,
            OutputEventCategory::Unknown => proto::DapOutputCategory::Unknown,
            OutputEventCategory::Telemetry => proto::DapOutputCategory::Telemetry,
            _ => unreachable!(),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        match payload {
            proto::DapOutputCategory::ConsoleOutput => Self::Console,
            proto::DapOutputCategory::Important => Self::Important,
            proto::DapOutputCategory::Stdout => Self::Stdout,
            proto::DapOutputCategory::Stderr => Self::Stderr,
            proto::DapOutputCategory::Unknown => Self::Unknown,
            proto::DapOutputCategory::Telemetry => Self::Telemetry,
        }
    }
}

impl ProtoConversion for dap_types::OutputEvent {
    type ProtoType = proto::DapOutputEvent;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        proto::DapOutputEvent {
            category: self
                .category
                .as_ref()
                .map(|category| category.to_proto().into()),
            output: self.output.clone(),
            variables_reference: self.variables_reference,
            source: self.source.as_ref().map(|source| source.to_proto()),
            line: self.line.map(|line| line as u32),
            column: self.column.map(|column| column as u32),
            group: self.group.map(|group| group.to_proto().into()),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        dap_types::OutputEvent {
            category: payload
                .category
                .and_then(proto::DapOutputCategory::from_i32)
                .map(OutputEventCategory::from_proto),
            output: payload.output.clone(),
            variables_reference: payload.variables_reference,
            source: payload.source.map(Source::from_proto),
            line: payload.line.map(|line| line as u64),
            column: payload.column.map(|column| column as u64),
            group: payload
                .group
                .and_then(proto::DapOutputEventGroup::from_i32)
                .map(OutputEventGroup::from_proto),
            data: None,
            location_reference: None,
        }
    }
}

impl ProtoConversion for dap_types::OutputEventGroup {
    type ProtoType = proto::DapOutputEventGroup;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        match self {
            dap_types::OutputEventGroup::Start => proto::DapOutputEventGroup::Start,
            dap_types::OutputEventGroup::StartCollapsed => {
                proto::DapOutputEventGroup::StartCollapsed
            }
            dap_types::OutputEventGroup::End => proto::DapOutputEventGroup::End,
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        match payload {
            proto::DapOutputEventGroup::Start => Self::Start,
            proto::DapOutputEventGroup::StartCollapsed => Self::StartCollapsed,
            proto::DapOutputEventGroup::End => Self::End,
        }
    }
}

impl ProtoConversion for dap_types::CompletionItem {
    type ProtoType = proto::DapCompletionItem;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        proto::DapCompletionItem {
            label: self.label.clone(),
            text: self.text.clone(),
            detail: self.detail.clone(),
            typ: self
                .type_
                .as_ref()
                .map(ProtoConversion::to_proto)
                .map(|typ| typ.into()),
            start: self.start,
            length: self.length,
            selection_start: self.selection_start,
            selection_length: self.selection_length,
            sort_text: self.sort_text.clone(),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        let typ = payload.typ(); // todo(debugger): This might be a potential issue/bug because it defaults to a type when it's None

        Self {
            label: payload.label,
            detail: payload.detail,
            sort_text: payload.sort_text,
            text: payload.text.clone(),
            type_: Some(dap_types::CompletionItemType::from_proto(typ)),
            start: payload.start,
            length: payload.length,
            selection_start: payload.selection_start,
            selection_length: payload.selection_length,
        }
    }
}

impl ProtoConversion for dap_types::EvaluateArgumentsContext {
    type ProtoType = DapEvaluateContext;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        match self {
            dap_types::EvaluateArgumentsContext::Variables => {
                proto::DapEvaluateContext::EvaluateVariables
            }
            dap_types::EvaluateArgumentsContext::Watch => proto::DapEvaluateContext::Watch,
            dap_types::EvaluateArgumentsContext::Hover => proto::DapEvaluateContext::Hover,
            dap_types::EvaluateArgumentsContext::Repl => proto::DapEvaluateContext::Repl,
            dap_types::EvaluateArgumentsContext::Clipboard => proto::DapEvaluateContext::Clipboard,
            dap_types::EvaluateArgumentsContext::Unknown => {
                proto::DapEvaluateContext::EvaluateUnknown
            }
            _ => unreachable!(),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        match payload {
            proto::DapEvaluateContext::EvaluateVariables => {
                dap_types::EvaluateArgumentsContext::Variables
            }
            proto::DapEvaluateContext::Watch => dap_types::EvaluateArgumentsContext::Watch,
            proto::DapEvaluateContext::Hover => dap_types::EvaluateArgumentsContext::Hover,
            proto::DapEvaluateContext::Repl => dap_types::EvaluateArgumentsContext::Repl,
            proto::DapEvaluateContext::Clipboard => dap_types::EvaluateArgumentsContext::Clipboard,
            proto::DapEvaluateContext::EvaluateUnknown => {
                dap_types::EvaluateArgumentsContext::Unknown
            }
        }
    }
}

impl ProtoConversion for dap_types::CompletionItemType {
    type ProtoType = proto::DapCompletionItemType;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        match self {
            dap_types::CompletionItemType::Class => proto::DapCompletionItemType::Class,
            dap_types::CompletionItemType::Color => proto::DapCompletionItemType::Color,
            dap_types::CompletionItemType::Constructor => proto::DapCompletionItemType::Constructor,
            dap_types::CompletionItemType::Customcolor => proto::DapCompletionItemType::Customcolor,
            dap_types::CompletionItemType::Enum => proto::DapCompletionItemType::Enum,
            dap_types::CompletionItemType::Field => proto::DapCompletionItemType::Field,
            dap_types::CompletionItemType::File => proto::DapCompletionItemType::CompletionItemFile,
            dap_types::CompletionItemType::Function => proto::DapCompletionItemType::Function,
            dap_types::CompletionItemType::Interface => proto::DapCompletionItemType::Interface,
            dap_types::CompletionItemType::Keyword => proto::DapCompletionItemType::Keyword,
            dap_types::CompletionItemType::Method => proto::DapCompletionItemType::Method,
            dap_types::CompletionItemType::Module => proto::DapCompletionItemType::Module,
            dap_types::CompletionItemType::Property => proto::DapCompletionItemType::Property,
            dap_types::CompletionItemType::Reference => proto::DapCompletionItemType::Reference,
            dap_types::CompletionItemType::Snippet => proto::DapCompletionItemType::Snippet,
            dap_types::CompletionItemType::Text => proto::DapCompletionItemType::Text,
            dap_types::CompletionItemType::Unit => proto::DapCompletionItemType::Unit,
            dap_types::CompletionItemType::Value => proto::DapCompletionItemType::Value,
            dap_types::CompletionItemType::Variable => proto::DapCompletionItemType::Variable,
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        match payload {
            proto::DapCompletionItemType::Class => dap_types::CompletionItemType::Class,
            proto::DapCompletionItemType::Color => dap_types::CompletionItemType::Color,
            proto::DapCompletionItemType::CompletionItemFile => dap_types::CompletionItemType::File,
            proto::DapCompletionItemType::Constructor => dap_types::CompletionItemType::Constructor,
            proto::DapCompletionItemType::Customcolor => dap_types::CompletionItemType::Customcolor,
            proto::DapCompletionItemType::Enum => dap_types::CompletionItemType::Enum,
            proto::DapCompletionItemType::Field => dap_types::CompletionItemType::Field,
            proto::DapCompletionItemType::Function => dap_types::CompletionItemType::Function,
            proto::DapCompletionItemType::Interface => dap_types::CompletionItemType::Interface,
            proto::DapCompletionItemType::Keyword => dap_types::CompletionItemType::Keyword,
            proto::DapCompletionItemType::Method => dap_types::CompletionItemType::Method,
            proto::DapCompletionItemType::Module => dap_types::CompletionItemType::Module,
            proto::DapCompletionItemType::Property => dap_types::CompletionItemType::Property,
            proto::DapCompletionItemType::Reference => dap_types::CompletionItemType::Reference,
            proto::DapCompletionItemType::Snippet => dap_types::CompletionItemType::Snippet,
            proto::DapCompletionItemType::Text => dap_types::CompletionItemType::Text,
            proto::DapCompletionItemType::Unit => dap_types::CompletionItemType::Unit,
            proto::DapCompletionItemType::Value => dap_types::CompletionItemType::Value,
            proto::DapCompletionItemType::Variable => dap_types::CompletionItemType::Variable,
        }
    }
}

impl ProtoConversion for dap_types::Thread {
    type ProtoType = proto::DapThread;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        proto::DapThread {
            id: self.id,
            name: self.name.clone(),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        Self {
            id: payload.id,
            name: payload.name,
        }
    }
}

impl ProtoConversion for dap_types::ExceptionBreakMode {
    type ProtoType = proto::ExceptionBreakMode;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        match self {
            dap_types::ExceptionBreakMode::Never => proto::ExceptionBreakMode::Never,
            dap_types::ExceptionBreakMode::Always => proto::ExceptionBreakMode::Always,
            dap_types::ExceptionBreakMode::Unhandled => proto::ExceptionBreakMode::Unhandled,
            dap_types::ExceptionBreakMode::UserUnhandled => {
                proto::ExceptionBreakMode::AlwaysUnhandled
            }
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        match payload {
            proto::ExceptionBreakMode::Never => dap_types::ExceptionBreakMode::Never,
            proto::ExceptionBreakMode::Always => dap_types::ExceptionBreakMode::Always,
            proto::ExceptionBreakMode::Unhandled => dap_types::ExceptionBreakMode::Unhandled,
            proto::ExceptionBreakMode::AlwaysUnhandled => {
                dap_types::ExceptionBreakMode::UserUnhandled
            }
        }
    }
}

impl ProtoConversion for dap_types::ExceptionPathSegment {
    type ProtoType = proto::ExceptionPathSegment;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        proto::ExceptionPathSegment {
            negate: self.negate,
            names: self.names.clone(),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        Self {
            negate: payload.negate,
            names: payload.names,
        }
    }
}

impl ProtoConversion for dap_types::ExceptionOptions {
    type ProtoType = proto::ExceptionOptions;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        proto::ExceptionOptions {
            path: self.path.as_ref().map(|p| p.to_proto()).unwrap_or_default(),
            break_mode: self.break_mode.to_proto() as i32,
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        Self {
            path: if payload.path.is_empty() {
                None
            } else {
                Some(
                    payload
                        .path
                        .into_iter()
                        .map(dap_types::ExceptionPathSegment::from_proto)
                        .collect(),
                )
            },
            break_mode: match payload.break_mode {
                0 => dap_types::ExceptionBreakMode::Never,
                1 => dap_types::ExceptionBreakMode::Always,
                2 => dap_types::ExceptionBreakMode::Unhandled,
                3 => dap_types::ExceptionBreakMode::UserUnhandled,
                _ => dap_types::ExceptionBreakMode::Never,
            },
        }
    }
}

impl ProtoConversion for dap_types::ExceptionFilterOptions {
    type ProtoType = proto::ExceptionFilterOptions;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        proto::ExceptionFilterOptions {
            filter_id: self.filter_id.clone(),
            condition: self.condition.clone(),
            mode: self.mode.clone(),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        Self {
            filter_id: payload.filter_id,
            condition: payload.condition,
            mode: payload.mode,
        }
    }
}

impl ProtoConversion for dap_types::BreakpointReason {
    type ProtoType = proto::DapBreakpointReason;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        match self {
            dap_types::BreakpointReason::Pending => proto::DapBreakpointReason::Pending,
            dap_types::BreakpointReason::Failed => proto::DapBreakpointReason::Failed,
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        match payload {
            proto::DapBreakpointReason::Pending => dap_types::BreakpointReason::Pending,
            proto::DapBreakpointReason::Failed => dap_types::BreakpointReason::Failed,
        }
    }
}

impl ProtoConversion for dap_types::Breakpoint {
    type ProtoType = proto::DapBreakpoint;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        proto::DapBreakpoint {
            id: self.id.map(|id| id as i64),
            verified: self.verified,
            message: self.message.clone(),
            source: self.source.as_ref().map(|source| source.to_proto()),
            line: self.line.map(|line| line as u32),
            column: self.column.map(|column| column as u32),
            end_line: self.end_line.map(|line| line as u32),
            end_column: self.end_column.map(|column| column as u32),
            instruction_reference: self.instruction_reference.clone(),
            offset: self.offset.map(|offset| offset as u32),
            reason: self.reason.as_ref().map(|reason| reason.to_proto() as i32),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        Self {
            id: payload.id.map(|id| id as u64),
            verified: payload.verified,
            message: payload.message,
            source: payload.source.map(dap_types::Source::from_proto),
            line: payload.line.map(|line| line as u64),
            column: payload.column.map(|column| column as u64),
            end_line: payload.end_line.map(|line| line as u64),
            end_column: payload.end_column.map(|column| column as u64),
            instruction_reference: payload.instruction_reference,
            offset: payload.offset.map(|offset| offset as u64),
            reason: payload.reason.and_then(|reason| match reason {
                0 => Some(dap_types::BreakpointReason::Pending),
                1 => Some(dap_types::BreakpointReason::Failed),
                _ => None,
            }),
        }
    }
}

impl ProtoConversion for dap_types::BreakpointModeApplicability {
    type ProtoType = BreakpointModeApplicability;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        match self {
            dap_types::BreakpointModeApplicability::Source => BreakpointModeApplicability::Source,
            dap_types::BreakpointModeApplicability::Exception => {
                BreakpointModeApplicability::Exception
            }
            dap_types::BreakpointModeApplicability::Data => BreakpointModeApplicability::Data,
            dap_types::BreakpointModeApplicability::Instruction => {
                BreakpointModeApplicability::InstructionBreakpoint
            }
            dap_types::BreakpointModeApplicability::Unknown => {
                BreakpointModeApplicability::BreakpointModeUnknown
            }
            _ => unreachable!(),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        match payload {
            BreakpointModeApplicability::Source => dap_types::BreakpointModeApplicability::Source,
            BreakpointModeApplicability::Exception => {
                dap_types::BreakpointModeApplicability::Exception
            }
            BreakpointModeApplicability::Data => dap_types::BreakpointModeApplicability::Data,
            BreakpointModeApplicability::InstructionBreakpoint => {
                dap_types::BreakpointModeApplicability::Instruction
            }
            BreakpointModeApplicability::BreakpointModeUnknown => {
                dap_types::BreakpointModeApplicability::Unknown
            }
        }
    }
}

impl ProtoConversion for dap_types::BreakpointMode {
    type ProtoType = BreakpointMode;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        BreakpointMode {
            mode: self.mode.clone(),
            label: self.label.clone(),
            description: self.description.clone(),
            applies_to: self
                .applies_to
                .iter()
                .map(|a| a.to_proto() as i32)
                .collect(),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        Self {
            mode: payload.mode,
            label: payload.label,
            description: payload.description,
            applies_to: payload
                .applies_to
                .into_iter()
                .filter_map(BreakpointModeApplicability::from_i32)
                .map(dap_types::BreakpointModeApplicability::from_proto)
                .collect(),
        }
    }
}

impl ProtoConversion for dap_types::ExceptionBreakpointsFilter {
    type ProtoType = ExceptionBreakpointsFilter;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        ExceptionBreakpointsFilter {
            filter: self.filter.clone(),
            label: self.label.clone(),
            description: self.description.clone(),
            default_value: self.default,
            supports_condition: self.supports_condition,
            condition_description: self.condition_description.clone(),
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        Self {
            filter: payload.filter,
            label: payload.label,
            description: payload.description,
            default: payload.default_value,
            supports_condition: payload.supports_condition,
            condition_description: payload.condition_description,
        }
    }
}

impl ProtoConversion for dap_types::Capabilities {
    type ProtoType = Capabilities;
    type Output = Self;

    fn to_proto(&self) -> Self::ProtoType {
        Capabilities {
            supports_configuration_done_request: self.supports_configuration_done_request,
            supports_function_breakpoints: self.supports_function_breakpoints,
            supports_conditional_breakpoints: self.supports_conditional_breakpoints,
            supports_hit_conditional_breakpoints: self.supports_hit_conditional_breakpoints,
            supports_evaluate_for_hovers: self.supports_evaluate_for_hovers,
            exception_breakpoint_filters: self
                .exception_breakpoint_filters
                .as_ref()
                .map(|filters| filters.to_proto())
                .unwrap_or_default(),
            supports_step_back: self.supports_step_back,
            supports_set_variable: self.supports_set_variable,
            supports_restart_frame: self.supports_restart_frame,
            supports_goto_targets_request: self.supports_goto_targets_request,
            supports_step_in_targets_request: self.supports_step_in_targets_request,
            supports_completions_request: self.supports_completions_request,
            completion_trigger_characters: self
                .completion_trigger_characters
                .clone()
                .unwrap_or_default(),
            supports_modules_request: self.supports_modules_request,
            additional_module_columns: self
                .additional_module_columns
                .as_ref()
                .map(|columns| {
                    columns
                        .iter()
                        .map(|col| col.attribute_name.clone())
                        .collect()
                })
                .unwrap_or_default(),
            supported_checksum_algorithms: self
                .supported_checksum_algorithms
                .as_ref()
                .map(|algorithms| algorithms.iter().map(|alg| format!("{:?}", alg)).collect())
                .unwrap_or_default(),
            supports_restart_request: self.supports_restart_request,
            supports_exception_options: self.supports_exception_options,
            supports_value_formatting_options: self.supports_value_formatting_options,
            supports_exception_info_request: self.supports_exception_info_request,
            support_terminate_debuggee: self.support_terminate_debuggee,
            support_suspend_debuggee: self.support_suspend_debuggee,
            supports_delayed_stack_trace_loading: self.supports_delayed_stack_trace_loading,
            supports_loaded_sources_request: self.supports_loaded_sources_request,
            supports_log_points: self.supports_log_points,
            supports_terminate_threads_request: self.supports_terminate_threads_request,
            supports_set_expression: self.supports_set_expression,
            supports_terminate_request: self.supports_terminate_request,
            supports_data_breakpoints: self.supports_data_breakpoints,
            supports_read_memory_request: self.supports_read_memory_request,
            supports_write_memory_request: self.supports_write_memory_request,
            supports_disassemble_request: self.supports_disassemble_request,
            supports_cancel_request: self.supports_cancel_request,
            supports_breakpoint_locations_request: self.supports_breakpoint_locations_request,
            supports_clipboard_context: self.supports_clipboard_context,
            supports_stepping_granularity: self.supports_stepping_granularity,
            supports_instruction_breakpoints: self.supports_instruction_breakpoints,
            supports_exception_filter_options: self.supports_exception_filter_options,
            supports_single_thread_execution_requests: self
                .supports_single_thread_execution_requests,
            supports_data_breakpoint_bytes: self.supports_data_breakpoint_bytes,
            breakpoint_modes: self
                .breakpoint_modes
                .as_ref()
                .map(|modes| modes.to_proto())
                .unwrap_or_default(),
            supports_ansistyling: self.supports_ansistyling,
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        Self {
            supports_configuration_done_request: payload.supports_configuration_done_request,
            supports_function_breakpoints: payload.supports_function_breakpoints,
            supports_conditional_breakpoints: payload.supports_conditional_breakpoints,
            supports_hit_conditional_breakpoints: payload.supports_hit_conditional_breakpoints,
            supports_evaluate_for_hovers: payload.supports_evaluate_for_hovers,
            exception_breakpoint_filters: if payload.exception_breakpoint_filters.is_empty() {
                None
            } else {
                Some(Vec::<dap_types::ExceptionBreakpointsFilter>::from_proto(
                    payload.exception_breakpoint_filters,
                ))
            },
            supports_step_back: payload.supports_step_back,
            supports_set_variable: payload.supports_set_variable,
            supports_restart_frame: payload.supports_restart_frame,
            supports_goto_targets_request: payload.supports_goto_targets_request,
            supports_step_in_targets_request: payload.supports_step_in_targets_request,
            supports_completions_request: payload.supports_completions_request,
            completion_trigger_characters: if payload.completion_trigger_characters.is_empty() {
                None
            } else {
                Some(payload.completion_trigger_characters)
            },
            supports_modules_request: payload.supports_modules_request,
            additional_module_columns: if payload.additional_module_columns.is_empty() {
                None
            } else {
                Some(
                    payload
                        .additional_module_columns
                        .into_iter()
                        .map(|name| dap_types::ColumnDescriptor {
                            attribute_name: name,
                            label: String::new(),
                            format: None,
                            type_: None,
                            width: None,
                        })
                        .collect(),
                )
            },
            supported_checksum_algorithms: if payload.supported_checksum_algorithms.is_empty() {
                None
            } else {
                Some(
                    payload
                        .supported_checksum_algorithms
                        .into_iter()
                        .filter_map(|alg| match alg.as_str() {
                            "Md5" => Some(dap_types::ChecksumAlgorithm::Md5),
                            "Sha1" => Some(dap_types::ChecksumAlgorithm::Sha1),
                            "Sha256" => Some(dap_types::ChecksumAlgorithm::Sha256),
                            "Timestamp" => Some(dap_types::ChecksumAlgorithm::Timestamp),
                            _ => None,
                        })
                        .collect(),
                )
            },
            supports_restart_request: payload.supports_restart_request,
            supports_exception_options: payload.supports_exception_options,
            supports_value_formatting_options: payload.supports_value_formatting_options,
            supports_exception_info_request: payload.supports_exception_info_request,
            support_terminate_debuggee: payload.support_terminate_debuggee,
            support_suspend_debuggee: payload.support_suspend_debuggee,
            supports_delayed_stack_trace_loading: payload.supports_delayed_stack_trace_loading,
            supports_loaded_sources_request: payload.supports_loaded_sources_request,
            supports_log_points: payload.supports_log_points,
            supports_terminate_threads_request: payload.supports_terminate_threads_request,
            supports_set_expression: payload.supports_set_expression,
            supports_terminate_request: payload.supports_terminate_request,
            supports_data_breakpoints: payload.supports_data_breakpoints,
            supports_read_memory_request: payload.supports_read_memory_request,
            supports_write_memory_request: payload.supports_write_memory_request,
            supports_disassemble_request: payload.supports_disassemble_request,
            supports_cancel_request: payload.supports_cancel_request,
            supports_breakpoint_locations_request: payload.supports_breakpoint_locations_request,
            supports_clipboard_context: payload.supports_clipboard_context,
            supports_stepping_granularity: payload.supports_stepping_granularity,
            supports_instruction_breakpoints: payload.supports_instruction_breakpoints,
            supports_exception_filter_options: payload.supports_exception_filter_options,
            supports_single_thread_execution_requests: payload
                .supports_single_thread_execution_requests,
            supports_data_breakpoint_bytes: payload.supports_data_breakpoint_bytes,
            breakpoint_modes: if payload.breakpoint_modes.is_empty() {
                None
            } else {
                Some(Vec::<dap_types::BreakpointMode>::from_proto(
                    payload.breakpoint_modes,
                ))
            },
            supports_ansistyling: payload.supports_ansistyling,
        }
    }
}
