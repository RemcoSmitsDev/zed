#![allow(non_snake_case)]

pub mod error;
mod macros;
mod typed_envelope;

pub use error::*;
pub use typed_envelope::*;

pub use prost::{DecodeError, Message};
use serde::Serialize;
use std::{
    any::{Any, TypeId},
    cmp,
    fmt::{self, Debug},
    iter, mem,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

include!(concat!(env!("OUT_DIR"), "/zed.messages.rs"));

pub const SSH_PEER_ID: PeerId = PeerId { owner_id: 0, id: 0 };
pub const SSH_PROJECT_ID: u64 = 0;

pub trait EnvelopedMessage: Clone + Debug + Serialize + Sized + Send + Sync + 'static {
    const NAME: &'static str;
    const PRIORITY: MessagePriority;
    fn into_envelope(
        self,
        id: u32,
        responding_to: Option<u32>,
        original_sender_id: Option<PeerId>,
    ) -> Envelope;
    fn from_envelope(envelope: Envelope) -> Option<Self>;
}

pub trait EntityMessage: EnvelopedMessage {
    type Entity;
    fn remote_entity_id(&self) -> u64;
}

pub trait RequestMessage: EnvelopedMessage {
    type Response: EnvelopedMessage;
}

pub trait AnyTypedEnvelope: 'static + Send + Sync {
    fn payload_type_id(&self) -> TypeId;
    fn payload_type_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send + Sync>;
    fn is_background(&self) -> bool;
    fn original_sender_id(&self) -> Option<PeerId>;
    fn sender_id(&self) -> PeerId;
    fn message_id(&self) -> u32;
}

pub enum MessagePriority {
    Foreground,
    Background,
}

impl<T: EnvelopedMessage> AnyTypedEnvelope for TypedEnvelope<T> {
    fn payload_type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn payload_type_name(&self) -> &'static str {
        T::NAME
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any + Send + Sync> {
        self
    }

    fn is_background(&self) -> bool {
        matches!(T::PRIORITY, MessagePriority::Background)
    }

    fn original_sender_id(&self) -> Option<PeerId> {
        self.original_sender_id
    }

    fn sender_id(&self) -> PeerId {
        self.sender_id
    }

    fn message_id(&self) -> u32 {
        self.message_id
    }
}

impl PeerId {
    pub fn from_u64(peer_id: u64) -> Self {
        let owner_id = (peer_id >> 32) as u32;
        let id = peer_id as u32;
        Self { owner_id, id }
    }

    pub fn as_u64(self) -> u64 {
        ((self.owner_id as u64) << 32) | (self.id as u64)
    }
}

impl Copy for PeerId {}

impl Eq for PeerId {}

impl Ord for PeerId {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.owner_id
            .cmp(&other.owner_id)
            .then_with(|| self.id.cmp(&other.id))
    }
}

impl PartialOrd for PeerId {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::hash::Hash for PeerId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.owner_id.hash(state);
        self.id.hash(state);
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner_id, self.id)
    }
}

pub trait FromProto {
    fn from_proto(proto: String) -> Self;
}

pub trait ToProto {
    fn to_proto(self) -> String;
}

impl FromProto for PathBuf {
    #[cfg(target_os = "windows")]
    fn from_proto(proto: String) -> Self {
        proto.split("/").collect()
    }

    #[cfg(not(target_os = "windows"))]
    fn from_proto(proto: String) -> Self {
        PathBuf::from(proto)
    }
}

impl FromProto for Arc<Path> {
    fn from_proto(proto: String) -> Self {
        PathBuf::from_proto(proto).into()
    }
}

impl ToProto for PathBuf {
    #[cfg(target_os = "windows")]
    fn to_proto(self) -> String {
        self.components()
            .map(|comp| comp.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/")
    }

    #[cfg(not(target_os = "windows"))]
    fn to_proto(self) -> String {
        self.to_string_lossy().to_string()
    }
}

impl ToProto for &Path {
    #[cfg(target_os = "windows")]
    fn to_proto(self) -> String {
        self.components()
            .map(|comp| comp.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/")
    }

    #[cfg(not(target_os = "windows"))]
    fn to_proto(self) -> String {
        self.to_string_lossy().to_string()
    }
}

messages!(
    (AcceptTermsOfService, Foreground),
    (AcceptTermsOfServiceResponse, Foreground),
    (Ack, Foreground),
    (AckBufferOperation, Background),
    (AckChannelMessage, Background),
    (ActivateToolchain, Foreground),
    (ActiveToolchain, Foreground),
    (ActiveToolchainResponse, Foreground),
    (AddNotification, Foreground),
    (AddProjectCollaborator, Foreground),
    (AddWorktree, Foreground),
    (AddWorktreeResponse, Foreground),
    (AdvertiseContexts, Foreground),
    (ApplyCodeAction, Background),
    (ApplyCodeActionResponse, Background),
    (ApplyCompletionAdditionalEdits, Background),
    (ApplyCompletionAdditionalEditsResponse, Background),
    (BlameBuffer, Foreground),
    (BlameBufferResponse, Foreground),
    (BufferReloaded, Foreground),
    (BufferSaved, Foreground),
    (Call, Foreground),
    (CallCanceled, Foreground),
    (CancelCall, Foreground),
    (CancelLanguageServerWork, Foreground),
    (ChannelMessageSent, Foreground),
    (ChannelMessageUpdate, Foreground),
    (CloseBuffer, Foreground),
    (Commit, Background),
    (ComputeEmbeddings, Background),
    (ComputeEmbeddingsResponse, Background),
    (CopyProjectEntry, Foreground),
    (CountLanguageModelTokens, Background),
    (CountLanguageModelTokensResponse, Background),
    (CreateBufferForPeer, Foreground),
    (CreateChannel, Foreground),
    (CreateChannelResponse, Foreground),
    (CreateContext, Foreground),
    (CreateContextResponse, Foreground),
    (CreateProjectEntry, Foreground),
    (CreateRoom, Foreground),
    (CreateRoomResponse, Foreground),
    (DapContinueRequest, Background),
    (DapContinueResponse, Background),
    (DapModulesRequest, Background),
    (DapModulesResponse, Background),
    (DapLoadedSourcesRequest, Background),
    (DapLoadedSourcesResponse, Background),
    (DapDisconnectRequest, Background),
    (DapNextRequest, Background),
    (DapPauseRequest, Background),
    (DapRestartRequest, Background),
    (DapRestartStackFrameRequest, Background),
    (DapStepBackRequest, Background),
    (DapStepInRequest, Background),
    (DapStepOutRequest, Background),
    (DapTerminateThreadsRequest, Background),
    (DeclineCall, Foreground),
    (DeleteChannel, Foreground),
    (DeleteNotification, Foreground),
    (DeleteProjectEntry, Foreground),
    (EndStream, Foreground),
    (Error, Foreground),
    (ExpandProjectEntry, Foreground),
    (ExpandProjectEntryResponse, Foreground),
    (FindSearchCandidatesResponse, Background),
    (FindSearchCandidates, Background),
    (FlushBufferedMessages, Foreground),
    (ExpandAllForProjectEntry, Foreground),
    (ExpandAllForProjectEntryResponse, Foreground),
    (Follow, Foreground),
    (FollowResponse, Foreground),
    (ApplyCodeActionKind, Foreground),
    (ApplyCodeActionKindResponse, Foreground),
    (FormatBuffers, Foreground),
    (FormatBuffersResponse, Foreground),
    (FuzzySearchUsers, Foreground),
    (GetCachedEmbeddings, Background),
    (GetCachedEmbeddingsResponse, Background),
    (GetChannelMembers, Foreground),
    (GetChannelMembersResponse, Foreground),
    (GetChannelMessages, Background),
    (GetChannelMessagesById, Background),
    (GetChannelMessagesResponse, Background),
    (GetCodeActions, Background),
    (GetCodeActionsResponse, Background),
    (GetCompletions, Background),
    (GetCompletionsResponse, Background),
    (GetDeclaration, Background),
    (GetDeclarationResponse, Background),
    (GetDefinition, Background),
    (GetDefinitionResponse, Background),
    (GetDocumentHighlights, Background),
    (GetDocumentHighlightsResponse, Background),
    (GetHover, Background),
    (GetHoverResponse, Background),
    (GetNotifications, Foreground),
    (GetNotificationsResponse, Foreground),
    (GetPanicFiles, Background),
    (GetPanicFilesResponse, Background),
    (GetPathMetadata, Background),
    (GetPathMetadataResponse, Background),
    (GetPermalinkToLine, Foreground),
    (GetPermalinkToLineResponse, Foreground),
    (GetPrivateUserInfo, Foreground),
    (GetPrivateUserInfoResponse, Foreground),
    (GetProjectSymbols, Background),
    (GetProjectSymbolsResponse, Background),
    (GetReferences, Background),
    (GetReferencesResponse, Background),
    (GetSignatureHelp, Background),
    (GetSignatureHelpResponse, Background),
    (GetSupermavenApiKey, Background),
    (GetSupermavenApiKeyResponse, Background),
    (GetTypeDefinition, Background),
    (GetTypeDefinitionResponse, Background),
    (GetImplementation, Background),
    (GetImplementationResponse, Background),
    (GetLlmToken, Background),
    (GetLlmTokenResponse, Background),
    (OpenUnstagedDiff, Foreground),
    (OpenUnstagedDiffResponse, Foreground),
    (OpenUncommittedDiff, Foreground),
    (OpenUncommittedDiffResponse, Foreground),
    (GetUsers, Foreground),
    (GitBranches, Background),
    (GitBranchesResponse, Background),
    (Hello, Foreground),
    (HideToast, Background),
    (IncomingCall, Foreground),
    (InlayHints, Background),
    (InlayHintsResponse, Background),
    (InstallExtension, Background),
    (InviteChannelMember, Foreground),
    (JoinChannel, Foreground),
    (JoinChannelBuffer, Foreground),
    (JoinChannelBufferResponse, Foreground),
    (JoinChannelChat, Foreground),
    (JoinChannelChatResponse, Foreground),
    (JoinProject, Foreground),
    (JoinProjectResponse, Foreground),
    (JoinRoom, Foreground),
    (JoinRoomResponse, Foreground),
    (LanguageServerLog, Foreground),
    (LanguageServerPromptRequest, Foreground),
    (LanguageServerPromptResponse, Foreground),
    (LeaveChannelBuffer, Background),
    (LeaveChannelChat, Foreground),
    (LeaveProject, Foreground),
    (LeaveRoom, Foreground),
    (LinkedEditingRange, Background),
    (LinkedEditingRangeResponse, Background),
    (ListRemoteDirectory, Background),
    (ListRemoteDirectoryResponse, Background),
    (ListToolchains, Foreground),
    (ListToolchainsResponse, Foreground),
    (LspExtExpandMacro, Background),
    (LspExtExpandMacroResponse, Background),
    (LspExtOpenDocs, Background),
    (LspExtOpenDocsResponse, Background),
    (LspExtSwitchSourceHeader, Background),
    (LspExtSwitchSourceHeaderResponse, Background),
    (MarkNotificationRead, Foreground),
    (MoveChannel, Foreground),
    (MultiLspQuery, Background),
    (MultiLspQueryResponse, Background),
    (OnTypeFormatting, Background),
    (OnTypeFormattingResponse, Background),
    (OpenBufferById, Background),
    (OpenBufferByPath, Background),
    (OpenBufferForSymbol, Background),
    (OpenBufferForSymbolResponse, Background),
    (OpenBufferResponse, Background),
    (OpenCommitMessageBuffer, Background),
    (OpenContext, Foreground),
    (OpenContextResponse, Foreground),
    (OpenNewBuffer, Foreground),
    (OpenServerSettings, Foreground),
    (PerformRename, Background),
    (PerformRenameResponse, Background),
    (Ping, Foreground),
    (PrepareRename, Background),
    (PrepareRenameResponse, Background),
    (ProjectEntryResponse, Foreground),
    (RefreshInlayHints, Foreground),
    (RefreshLlmToken, Background),
    (RegisterBufferWithLanguageServers, Background),
    (RejoinChannelBuffers, Foreground),
    (RejoinChannelBuffersResponse, Foreground),
    (RejoinRemoteProjects, Foreground),
    (RejoinRemoteProjectsResponse, Foreground),
    (RejoinRoom, Foreground),
    (RejoinRoomResponse, Foreground),
    (ReloadBuffers, Foreground),
    (ReloadBuffersResponse, Foreground),
    (RemoveChannelMember, Foreground),
    (RemoveChannelMessage, Foreground),
    (RemoveContact, Foreground),
    (RemoveProjectCollaborator, Foreground),
    (RemoveWorktree, Foreground),
    (RenameChannel, Foreground),
    (RenameChannelResponse, Foreground),
    (RenameProjectEntry, Foreground),
    (RequestContact, Foreground),
    (ResolveCompletionDocumentation, Background),
    (ResolveCompletionDocumentationResponse, Background),
    (ResolveInlayHint, Background),
    (ResolveInlayHintResponse, Background),
    (RespondToChannelInvite, Foreground),
    (RespondToContactRequest, Foreground),
    (RestartLanguageServers, Foreground),
    (RoomUpdated, Foreground),
    (SaveBuffer, Foreground),
    (SendChannelMessage, Background),
    (SendChannelMessageResponse, Background),
    (SetChannelMemberRole, Foreground),
    (SetChannelVisibility, Foreground),
    (SetDebugClientCapabilities, Background),
    (SetRoomParticipantRole, Foreground),
    (ShareProject, Foreground),
    (ShareProjectResponse, Foreground),
    (ShowContacts, Foreground),
    (ShutdownDebugClient, Background),
    (ShutdownRemoteServer, Foreground),
    (Stage, Background),
    (StartLanguageServer, Foreground),
    (SubscribeToChannels, Foreground),
    (SyncExtensions, Background),
    (SyncExtensionsResponse, Background),
    (BreakpointsForFile, Background),
    (ToggleBreakpoint, Foreground),
    (SynchronizeBuffers, Foreground),
    (SynchronizeBuffersResponse, Foreground),
    (SynchronizeContexts, Foreground),
    (SynchronizeContextsResponse, Foreground),
    (TaskContext, Background),
    (TaskContextForLocation, Background),
    (Test, Foreground),
    (Toast, Background),
    (Unfollow, Foreground),
    (UnshareProject, Foreground),
    (Unstage, Background),
    (UpdateBuffer, Foreground),
    (UpdateBufferFile, Foreground),
    (UpdateChannelBuffer, Foreground),
    (UpdateChannelBufferCollaborators, Foreground),
    (UpdateChannelMessage, Foreground),
    (UpdateChannels, Foreground),
    (UpdateContacts, Foreground),
    (UpdateContext, Foreground),
    (UpdateDebugAdapter, Foreground),
    (UpdateDiagnosticSummary, Foreground),
    (UpdateDiffBases, Foreground),
    (UpdateFollowers, Foreground),
    (UpdateGitBranch, Background),
    (UpdateInviteInfo, Foreground),
    (UpdateLanguageServer, Foreground),
    (UpdateNotification, Foreground),
    (UpdateParticipantLocation, Foreground),
    (UpdateProject, Foreground),
    (UpdateProjectCollaborator, Foreground),
    (UpdateThreadStatus, Background),
    (UpdateUserChannels, Foreground),
    (UpdateUserPlan, Foreground),
    (UpdateWorktree, Foreground),
    (UpdateWorktreeSettings, Foreground),
    (UsersResponse, Foreground),
    (GitReset, Background),
    (GitCheckoutFiles, Background),
    (GitShow, Background),
    (GitCommitDetails, Background),
    (SetIndexText, Background),
    (Push, Background),
    (Fetch, Background),
    (GetRemotes, Background),
    (GetRemotesResponse, Background),
    (Pull, Background),
    (RemoteMessageResponse, Background),
    (VariablesRequest, Background),
    (DapVariables, Background),
    (IgnoreBreakpointState, Background),
    (ToggleIgnoreBreakpoints, Background),
    (DapStackTraceRequest, Background),
    (DapStackTraceResponse, Background),
    (DapScopesRequest, Background),
    (DapScopesResponse, Background),
    (DapSetVariableValueRequest, Background),
    (DapSetVariableValueResponse, Background),
    (DapEvaluateRequest, Background),
    (DapEvaluateResponse, Background),
    (DapCompletionRequest, Background),
    (DapCompletionResponse, Background),
    (DapThreadsRequest, Background),
    (DapThreadsResponse, Background),
    (DapTerminateRequest, Background)
);

request_messages!(
    (AcceptTermsOfService, AcceptTermsOfServiceResponse),
    (ApplyCodeAction, ApplyCodeActionResponse),
    (
        ApplyCompletionAdditionalEdits,
        ApplyCompletionAdditionalEditsResponse
    ),
    (Call, Ack),
    (CancelCall, Ack),
    (Commit, Ack),
    (CopyProjectEntry, ProjectEntryResponse),
    (ComputeEmbeddings, ComputeEmbeddingsResponse),
    (CreateChannel, CreateChannelResponse),
    (CreateProjectEntry, ProjectEntryResponse),
    (CreateRoom, CreateRoomResponse),
    (DeclineCall, Ack),
    (DeleteChannel, Ack),
    (DeleteProjectEntry, ProjectEntryResponse),
    (ExpandProjectEntry, ExpandProjectEntryResponse),
    (ExpandAllForProjectEntry, ExpandAllForProjectEntryResponse),
    (Follow, FollowResponse),
    (ApplyCodeActionKind, ApplyCodeActionKindResponse),
    (FormatBuffers, FormatBuffersResponse),
    (FuzzySearchUsers, UsersResponse),
    (GetCachedEmbeddings, GetCachedEmbeddingsResponse),
    (GetChannelMembers, GetChannelMembersResponse),
    (GetChannelMessages, GetChannelMessagesResponse),
    (GetChannelMessagesById, GetChannelMessagesResponse),
    (GetCodeActions, GetCodeActionsResponse),
    (GetCompletions, GetCompletionsResponse),
    (GetDefinition, GetDefinitionResponse),
    (GetDeclaration, GetDeclarationResponse),
    (GetImplementation, GetImplementationResponse),
    (GetDocumentHighlights, GetDocumentHighlightsResponse),
    (GetHover, GetHoverResponse),
    (GetLlmToken, GetLlmTokenResponse),
    (GetNotifications, GetNotificationsResponse),
    (GetPrivateUserInfo, GetPrivateUserInfoResponse),
    (GetProjectSymbols, GetProjectSymbolsResponse),
    (GetReferences, GetReferencesResponse),
    (GetSignatureHelp, GetSignatureHelpResponse),
    (OpenUnstagedDiff, OpenUnstagedDiffResponse),
    (OpenUncommittedDiff, OpenUncommittedDiffResponse),
    (GetSupermavenApiKey, GetSupermavenApiKeyResponse),
    (GetTypeDefinition, GetTypeDefinitionResponse),
    (LinkedEditingRange, LinkedEditingRangeResponse),
    (ListRemoteDirectory, ListRemoteDirectoryResponse),
    (GetUsers, UsersResponse),
    (IncomingCall, Ack),
    (InlayHints, InlayHintsResponse),
    (InviteChannelMember, Ack),
    (JoinChannel, JoinRoomResponse),
    (JoinChannelBuffer, JoinChannelBufferResponse),
    (JoinChannelChat, JoinChannelChatResponse),
    (JoinProject, JoinProjectResponse),
    (JoinRoom, JoinRoomResponse),
    (LeaveChannelBuffer, Ack),
    (LeaveRoom, Ack),
    (MarkNotificationRead, Ack),
    (MoveChannel, Ack),
    (OnTypeFormatting, OnTypeFormattingResponse),
    (OpenBufferById, OpenBufferResponse),
    (OpenBufferByPath, OpenBufferResponse),
    (OpenBufferForSymbol, OpenBufferForSymbolResponse),
    (OpenCommitMessageBuffer, OpenBufferResponse),
    (OpenNewBuffer, OpenBufferResponse),
    (PerformRename, PerformRenameResponse),
    (Ping, Ack),
    (PrepareRename, PrepareRenameResponse),
    (CountLanguageModelTokens, CountLanguageModelTokensResponse),
    (RefreshInlayHints, Ack),
    (RejoinChannelBuffers, RejoinChannelBuffersResponse),
    (RejoinRoom, RejoinRoomResponse),
    (ReloadBuffers, ReloadBuffersResponse),
    (RemoveChannelMember, Ack),
    (RemoveChannelMessage, Ack),
    (UpdateChannelMessage, Ack),
    (RemoveContact, Ack),
    (RenameChannel, RenameChannelResponse),
    (RenameProjectEntry, ProjectEntryResponse),
    (RequestContact, Ack),
    (
        ResolveCompletionDocumentation,
        ResolveCompletionDocumentationResponse
    ),
    (ResolveInlayHint, ResolveInlayHintResponse),
    (RespondToChannelInvite, Ack),
    (RespondToContactRequest, Ack),
    (SaveBuffer, BufferSaved),
    (Stage, Ack),
    (FindSearchCandidates, FindSearchCandidatesResponse),
    (SendChannelMessage, SendChannelMessageResponse),
    (SetChannelMemberRole, Ack),
    (SetChannelVisibility, Ack),
    (ShareProject, ShareProjectResponse),
    (SynchronizeBuffers, SynchronizeBuffersResponse),
    (TaskContextForLocation, TaskContext),
    (Test, Test),
    (Unstage, Ack),
    (UpdateBuffer, Ack),
    (UpdateParticipantLocation, Ack),
    (UpdateProject, Ack),
    (UpdateWorktree, Ack),
    (LspExtExpandMacro, LspExtExpandMacroResponse),
    (LspExtOpenDocs, LspExtOpenDocsResponse),
    (SetRoomParticipantRole, Ack),
    (BlameBuffer, BlameBufferResponse),
    (RejoinRemoteProjects, RejoinRemoteProjectsResponse),
    (MultiLspQuery, MultiLspQueryResponse),
    (RestartLanguageServers, Ack),
    (OpenContext, OpenContextResponse),
    (CreateContext, CreateContextResponse),
    (SynchronizeContexts, SynchronizeContextsResponse),
    (LspExtSwitchSourceHeader, LspExtSwitchSourceHeaderResponse),
    (AddWorktree, AddWorktreeResponse),
    (ShutdownRemoteServer, Ack),
    (RemoveWorktree, Ack),
    (OpenServerSettings, OpenBufferResponse),
    (GetPermalinkToLine, GetPermalinkToLineResponse),
    (FlushBufferedMessages, Ack),
    (LanguageServerPromptRequest, LanguageServerPromptResponse),
    (GitBranches, GitBranchesResponse),
    (UpdateGitBranch, Ack),
    (ListToolchains, ListToolchainsResponse),
    (ActivateToolchain, Ack),
    (ActiveToolchain, ActiveToolchainResponse),
    (GetPathMetadata, GetPathMetadataResponse),
    (GetPanicFiles, GetPanicFilesResponse),
    (CancelLanguageServerWork, Ack),
    (SyncExtensions, SyncExtensionsResponse),
    (InstallExtension, Ack),
    (RegisterBufferWithLanguageServers, Ack),
    (GitShow, GitCommitDetails),
    (GitReset, Ack),
    (GitCheckoutFiles, Ack),
    (SetIndexText, Ack),
    (Push, RemoteMessageResponse),
    (Fetch, RemoteMessageResponse),
    (GetRemotes, GetRemotesResponse),
    (Pull, RemoteMessageResponse),
    (DapNextRequest, Ack),
    (DapStepInRequest, Ack),
    (DapStepOutRequest, Ack),
    (DapStepBackRequest, Ack),
    (DapContinueRequest, DapContinueResponse),
    (DapModulesRequest, DapModulesResponse),
    (DapLoadedSourcesRequest, DapLoadedSourcesResponse),
    (DapPauseRequest, Ack),
    (DapDisconnectRequest, Ack),
    (DapTerminateThreadsRequest, Ack),
    (DapRestartRequest, Ack),
    (DapRestartStackFrameRequest, Ack),
    (VariablesRequest, DapVariables),
    (DapStackTraceRequest, DapStackTraceResponse),
    (DapScopesRequest, DapScopesResponse),
    (DapSetVariableValueRequest, DapSetVariableValueResponse),
    (DapEvaluateRequest, DapEvaluateResponse),
    (DapCompletionRequest, DapCompletionResponse),
    (DapThreadsRequest, DapThreadsResponse),
    (DapTerminateRequest, Ack),
    (ShutdownDebugClient, Ack),
    (ToggleBreakpoint, Ack)
);

entity_messages!(
    {project_id, ShareProject},
    AddProjectCollaborator,
    AddWorktree,
    ApplyCodeAction,
    ApplyCompletionAdditionalEdits,
    BlameBuffer,
    BufferReloaded,
    BufferSaved,
    CloseBuffer,
    Commit,
    CopyProjectEntry,
    CreateBufferForPeer,
    CreateProjectEntry,
    DeleteProjectEntry,
    ExpandProjectEntry,
    ExpandAllForProjectEntry,
    FindSearchCandidates,
    ApplyCodeActionKind,
    FormatBuffers,
    GetCodeActions,
    GetCompletions,
    GetDefinition,
    GetDeclaration,
    GetImplementation,
    GetDocumentHighlights,
    GetHover,
    GetProjectSymbols,
    GetReferences,
    GetSignatureHelp,
    OpenUnstagedDiff,
    OpenUncommittedDiff,
    GetTypeDefinition,
    InlayHints,
    JoinProject,
    LeaveProject,
    LinkedEditingRange,
    MultiLspQuery,
    RestartLanguageServers,
    OnTypeFormatting,
    OpenNewBuffer,
    OpenBufferById,
    OpenBufferByPath,
    OpenBufferForSymbol,
    OpenCommitMessageBuffer,
    PerformRename,
    PrepareRename,
    RefreshInlayHints,
    ReloadBuffers,
    RemoveProjectCollaborator,
    RenameProjectEntry,
    ResolveCompletionDocumentation,
    ResolveInlayHint,
    SaveBuffer,
    Stage,
    StartLanguageServer,
    SynchronizeBuffers,
    TaskContextForLocation,
    UnshareProject,
    Unstage,
    UpdateBuffer,
    UpdateBufferFile,
    UpdateDiagnosticSummary,
    UpdateDiffBases,
    UpdateLanguageServer,
    UpdateProject,
    UpdateProjectCollaborator,
    UpdateWorktree,
    UpdateWorktreeSettings,
    UpdateDebugAdapter,
    LspExtExpandMacro,
    LspExtOpenDocs,
    AdvertiseContexts,
    OpenContext,
    CreateContext,
    UpdateContext,
    SynchronizeContexts,
    LspExtSwitchSourceHeader,
    LanguageServerLog,
    Toast,
    HideToast,
    OpenServerSettings,
    GetPermalinkToLine,
    LanguageServerPromptRequest,
    GitBranches,
    UpdateGitBranch,
    ListToolchains,
    ActivateToolchain,
    ActiveToolchain,
    GetPathMetadata,
    CancelLanguageServerWork,
    RegisterBufferWithLanguageServers,
    GitShow,
    GitReset,
    GitCheckoutFiles,
    SetIndexText,

    Push,
    Fetch,
    GetRemotes,
    Pull,
    BreakpointsForFile,
    ToggleBreakpoint,
    ShutdownDebugClient,
    SetDebugClientCapabilities,
    DapNextRequest,
    DapStepInRequest,
    DapStepOutRequest,
    DapStepBackRequest,
    DapContinueRequest,
    DapPauseRequest,
    DapDisconnectRequest,
    DapTerminateThreadsRequest,
    DapRestartRequest,
    DapRestartStackFrameRequest,
    UpdateThreadStatus,
    VariablesRequest,
    IgnoreBreakpointState,
    ToggleIgnoreBreakpoints,
    DapStackTraceRequest,
    DapScopesRequest,
    DapSetVariableValueRequest,
    DapEvaluateRequest,
    DapCompletionRequest,
    DapThreadsRequest,
    DapTerminateRequest
);

entity_messages!(
    {channel_id, Channel},
    ChannelMessageSent,
    ChannelMessageUpdate,
    RemoveChannelMessage,
    UpdateChannelMessage,
    UpdateChannelBuffer,
    UpdateChannelBufferCollaborators,
);

impl From<Timestamp> for SystemTime {
    fn from(val: Timestamp) -> Self {
        UNIX_EPOCH
            .checked_add(Duration::new(val.seconds, val.nanos))
            .unwrap()
    }
}

impl From<SystemTime> for Timestamp {
    fn from(time: SystemTime) -> Self {
        let duration = time.duration_since(UNIX_EPOCH).unwrap();
        Self {
            seconds: duration.as_secs(),
            nanos: duration.subsec_nanos(),
        }
    }
}

impl From<u128> for Nonce {
    fn from(nonce: u128) -> Self {
        let upper_half = (nonce >> 64) as u64;
        let lower_half = nonce as u64;
        Self {
            upper_half,
            lower_half,
        }
    }
}

impl From<Nonce> for u128 {
    fn from(nonce: Nonce) -> Self {
        let upper_half = (nonce.upper_half as u128) << 64;
        let lower_half = nonce.lower_half as u128;
        upper_half | lower_half
    }
}

#[cfg(any(test, feature = "test-support"))]
pub const MAX_WORKTREE_UPDATE_MAX_CHUNK_SIZE: usize = 2;
#[cfg(not(any(test, feature = "test-support")))]
pub const MAX_WORKTREE_UPDATE_MAX_CHUNK_SIZE: usize = 256;

pub fn split_worktree_update(mut message: UpdateWorktree) -> impl Iterator<Item = UpdateWorktree> {
    let mut done = false;

    iter::from_fn(move || {
        if done {
            return None;
        }

        let updated_entries_chunk_size = cmp::min(
            message.updated_entries.len(),
            MAX_WORKTREE_UPDATE_MAX_CHUNK_SIZE,
        );
        let updated_entries: Vec<_> = message
            .updated_entries
            .drain(..updated_entries_chunk_size)
            .collect();

        let removed_entries_chunk_size = cmp::min(
            message.removed_entries.len(),
            MAX_WORKTREE_UPDATE_MAX_CHUNK_SIZE,
        );
        let removed_entries = message
            .removed_entries
            .drain(..removed_entries_chunk_size)
            .collect();

        let mut updated_repositories = Vec::new();
        let mut limit = MAX_WORKTREE_UPDATE_MAX_CHUNK_SIZE;
        while let Some(repo) = message.updated_repositories.first_mut() {
            let updated_statuses_limit = cmp::min(repo.updated_statuses.len(), limit);
            let removed_statuses_limit = cmp::min(repo.removed_statuses.len(), limit);

            updated_repositories.push(RepositoryEntry {
                work_directory_id: repo.work_directory_id,
                branch: repo.branch.clone(),
                branch_summary: repo.branch_summary.clone(),
                updated_statuses: repo
                    .updated_statuses
                    .drain(..updated_statuses_limit)
                    .collect(),
                removed_statuses: repo
                    .removed_statuses
                    .drain(..removed_statuses_limit)
                    .collect(),
                current_merge_conflicts: repo.current_merge_conflicts.clone(),
            });
            if repo.removed_statuses.is_empty() && repo.updated_statuses.is_empty() {
                message.updated_repositories.remove(0);
            }
            limit = limit.saturating_sub(removed_statuses_limit + updated_statuses_limit);
            if limit == 0 {
                break;
            }
        }

        done = message.updated_entries.is_empty()
            && message.removed_entries.is_empty()
            && message.updated_repositories.is_empty();

        let removed_repositories = if done {
            mem::take(&mut message.removed_repositories)
        } else {
            Default::default()
        };

        Some(UpdateWorktree {
            project_id: message.project_id,
            worktree_id: message.worktree_id,
            root_name: message.root_name.clone(),
            abs_path: message.abs_path.clone(),
            updated_entries,
            removed_entries,
            scan_id: message.scan_id,
            is_last_update: done && message.is_last_update,
            updated_repositories,
            removed_repositories,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_converting_peer_id_from_and_to_u64() {
        let peer_id = PeerId {
            owner_id: 10,
            id: 3,
        };
        assert_eq!(PeerId::from_u64(peer_id.as_u64()), peer_id);
        let peer_id = PeerId {
            owner_id: u32::MAX,
            id: 3,
        };
        assert_eq!(PeerId::from_u64(peer_id.as_u64()), peer_id);
        let peer_id = PeerId {
            owner_id: 10,
            id: u32::MAX,
        };
        assert_eq!(PeerId::from_u64(peer_id.as_u64()), peer_id);
        let peer_id = PeerId {
            owner_id: u32::MAX,
            id: u32::MAX,
        };
        assert_eq!(PeerId::from_u64(peer_id.as_u64()), peer_id);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_proto() {
        fn generate_proto_path(path: PathBuf) -> PathBuf {
            let proto = path.to_proto();
            PathBuf::from_proto(proto)
        }

        let path = PathBuf::from("C:\\foo\\bar");
        assert_eq!(path, generate_proto_path(path.clone()));

        let path = PathBuf::from("C:/foo/bar/");
        assert_eq!(path, generate_proto_path(path.clone()));

        let path = PathBuf::from("C:/foo\\bar\\");
        assert_eq!(path, generate_proto_path(path.clone()));
    }
}
