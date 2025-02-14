use crate::ProjectPath;

use collections::{BTreeMap, HashSet};
use dap::SourceBreakpoint;
use gpui::{Context, Entity};
use language::{
    proto::{deserialize_anchor, serialize_anchor as serialize_text_anchor},
    Buffer, BufferSnapshot,
};
use rpc::{proto, AnyProtoClient};
use std::{
    hash::{Hash, Hasher},
    path::Path,
    sync::Arc,
};
use text::Point;
use util::ResultExt as _;

struct Remote {
    upstream_client: Option<AnyProtoClient>,
    upstream_project_id: u64,
}

enum BreakpointMode {
    Local,
    Remote(Remote),
}

pub struct BreakpointStore {
    pub breakpoints: BTreeMap<ProjectPath, HashSet<Breakpoint>>,
    downstream_client: Option<(AnyProtoClient, u64)>,
    mode: BreakpointMode,
}

impl BreakpointStore {
    pub fn local() -> Self {
        BreakpointStore {
            breakpoints: BTreeMap::new(),
            mode: BreakpointMode::Local,
            downstream_client: None,
        }
    }

    pub(crate) fn remote(upstream_project_id: u64, upstream_client: AnyProtoClient) -> Self {
        BreakpointStore {
            breakpoints: BTreeMap::new(),
            mode: BreakpointMode::Remote(Remote {
                upstream_client: Some(upstream_client),
                upstream_project_id,
            }),
            downstream_client: None,
        }
    }

    pub fn shared(&mut self, project_id: u64, downstream_client: AnyProtoClient) {
        self.downstream_client = Some((downstream_client.clone(), project_id));

        for (project_path, breakpoints) in self.breakpoints.iter() {
            downstream_client
                .send(proto::SynchronizeBreakpoints {
                    project_id,
                    project_path: Some(project_path.to_proto()),
                    breakpoints: breakpoints
                        .iter()
                        .filter_map(|breakpoint| breakpoint.to_proto())
                        .collect(),
                })
                .log_err();
        }
    }

    pub fn unshared(&mut self, cx: &mut Context<Self>) {
        self.downstream_client.take();

        cx.notify();
    }

    pub fn set_breakpoints_from_proto(
        &mut self,
        breakpoints: Vec<proto::SynchronizeBreakpoints>,
        cx: &mut Context<Self>,
    ) {
        let mut new_breakpoints = BTreeMap::new();
        for project_breakpoints in breakpoints {
            let Some(project_path) = project_breakpoints.project_path else {
                continue;
            };

            new_breakpoints.insert(
                ProjectPath::from_proto(project_path),
                project_breakpoints
                    .breakpoints
                    .into_iter()
                    .filter_map(Breakpoint::from_proto)
                    .collect::<HashSet<_>>(),
            );
        }

        std::mem::swap(&mut self.breakpoints, &mut new_breakpoints);
        cx.notify();
    }

    pub fn on_open_buffer(
        &mut self,
        project_path: &ProjectPath,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) {
        let entry = self.breakpoints.remove(project_path).unwrap_or_default();
        let mut set_bp: HashSet<Breakpoint> = HashSet::default();

        let buffer = buffer.read(cx);

        for mut bp in entry.into_iter() {
            bp.set_active_position(&buffer);
            set_bp.insert(bp);
        }

        self.breakpoints.insert(project_path.clone(), set_bp);

        cx.notify();
    }

    pub fn on_file_rename(&mut self, old_project_path: ProjectPath, new_project_path: ProjectPath) {
        if let Some(breakpoints) = self.breakpoints.remove(&old_project_path) {
            self.breakpoints.insert(new_project_path, breakpoints);
        }
    }
}

type LogMessage = Arc<str>;

#[derive(Clone, Debug)]
pub enum BreakpointEditAction {
    Toggle,
    EditLogMessage(LogMessage),
}

#[derive(Clone, Debug)]
pub enum BreakpointKind {
    Standard,
    Log(LogMessage),
}

impl BreakpointKind {
    pub fn to_int(&self) -> i32 {
        match self {
            BreakpointKind::Standard => 0,
            BreakpointKind::Log(_) => 1,
        }
    }

    pub fn log_message(&self) -> Option<LogMessage> {
        match self {
            BreakpointKind::Standard => None,
            BreakpointKind::Log(message) => Some(message.clone()),
        }
    }
}

impl PartialEq for BreakpointKind {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

impl Eq for BreakpointKind {}

impl Hash for BreakpointKind {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
    }
}

#[derive(Clone, Debug)]
pub struct Breakpoint {
    pub active_position: Option<text::Anchor>,
    pub cached_position: u32,
    pub kind: BreakpointKind,
}

// Custom implementation for PartialEq, Eq, and Hash is done
// to get toggle breakpoint to solely be based on a breakpoint's
// location. Otherwise, a user can get in situation's where there's
// overlapping breakpoint's with them being aware.
impl PartialEq for Breakpoint {
    fn eq(&self, other: &Self) -> bool {
        match (&self.active_position, &other.active_position) {
            (None, None) => self.cached_position == other.cached_position,
            (None, Some(_)) => false,
            (Some(_), None) => false,
            (Some(self_position), Some(other_position)) => self_position == other_position,
        }
    }
}

impl Eq for Breakpoint {}

impl Hash for Breakpoint {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if self.active_position.is_some() {
            self.active_position.hash(state);
        } else {
            self.cached_position.hash(state);
        }
    }
}

impl Breakpoint {
    pub fn to_source_breakpoint(&self, buffer: &Buffer) -> SourceBreakpoint {
        let line = self
            .active_position
            .map(|position| buffer.summary_for_anchor::<Point>(&position).row)
            .unwrap_or(self.cached_position) as u64;

        let log_message = match &self.kind {
            BreakpointKind::Standard => None,
            BreakpointKind::Log(message) => Some(message.clone().to_string()),
        };

        SourceBreakpoint {
            line,
            condition: None,
            hit_condition: None,
            log_message,
            column: None,
            mode: None,
        }
    }

    pub fn set_active_position(&mut self, buffer: &Buffer) {
        if self.active_position.is_none() {
            self.active_position =
                Some(buffer.breakpoint_anchor(Point::new(self.cached_position, 0)));
        }
    }

    pub fn point_for_buffer(&self, buffer: &Buffer) -> Point {
        self.active_position
            .map(|position| buffer.summary_for_anchor::<Point>(&position))
            .unwrap_or(Point::new(self.cached_position, 0))
    }

    pub fn point_for_buffer_snapshot(&self, buffer_snapshot: &BufferSnapshot) -> Point {
        self.active_position
            .map(|position| buffer_snapshot.summary_for_anchor::<Point>(&position))
            .unwrap_or(Point::new(self.cached_position, 0))
    }

    pub fn source_for_snapshot(&self, snapshot: Option<&BufferSnapshot>) -> SourceBreakpoint {
        let line = match snapshot {
            Some(snapshot) => self
                .active_position
                .map(|position| snapshot.summary_for_anchor::<Point>(&position).row)
                .unwrap_or(self.cached_position) as u64,
            None => self.cached_position as u64,
        };

        let log_message = match &self.kind {
            BreakpointKind::Standard => None,
            BreakpointKind::Log(log_message) => Some(log_message.clone().to_string()),
        };

        SourceBreakpoint {
            line,
            condition: None,
            hit_condition: None,
            log_message,
            column: None,
            mode: None,
        }
    }

    pub fn to_serialized(&self, buffer: Option<&Buffer>, path: Arc<Path>) -> SerializedBreakpoint {
        match buffer {
            Some(buffer) => SerializedBreakpoint {
                position: self
                    .active_position
                    .map(|position| buffer.summary_for_anchor::<Point>(&position).row)
                    .unwrap_or(self.cached_position),
                path,
                kind: self.kind.clone(),
            },
            None => SerializedBreakpoint {
                position: self.cached_position,
                path,
                kind: self.kind.clone(),
            },
        }
    }

    pub fn to_proto(&self) -> Option<client::proto::Breakpoint> {
        Some(client::proto::Breakpoint {
            position: if let Some(position) = &self.active_position {
                Some(serialize_text_anchor(position))
            } else {
                None
            },
            cached_position: self.cached_position,
            kind: match self.kind {
                BreakpointKind::Standard => proto::BreakpointKind::Standard.into(),
                BreakpointKind::Log(_) => proto::BreakpointKind::Log.into(),
            },
            message: if let BreakpointKind::Log(message) = &self.kind {
                Some(message.to_string())
            } else {
                None
            },
        })
    }

    pub fn from_proto(breakpoint: client::proto::Breakpoint) -> Option<Self> {
        Some(Self {
            active_position: if let Some(position) = breakpoint.position.clone() {
                deserialize_anchor(position)
            } else {
                None
            },
            cached_position: breakpoint.cached_position,
            kind: match proto::BreakpointKind::from_i32(breakpoint.kind) {
                Some(proto::BreakpointKind::Log) => {
                    BreakpointKind::Log(breakpoint.message.clone().unwrap_or_default().into())
                }
                None | Some(proto::BreakpointKind::Standard) => BreakpointKind::Standard,
            },
        })
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SerializedBreakpoint {
    pub position: u32,
    pub path: Arc<Path>,
    pub kind: BreakpointKind,
}

impl SerializedBreakpoint {
    pub fn to_source_breakpoint(&self) -> SourceBreakpoint {
        let log_message = match &self.kind {
            BreakpointKind::Standard => None,
            BreakpointKind::Log(message) => Some(message.clone().to_string()),
        };

        SourceBreakpoint {
            line: self.position as u64,
            condition: None,
            hit_condition: None,
            log_message,
            column: None,
            mode: None,
        }
    }
}
