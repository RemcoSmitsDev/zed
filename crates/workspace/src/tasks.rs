use project::TaskSourceKind;
use remote::ConnectionState;
use task::{ResolvedTask, TaskContext, TemplateType};
use ui::ViewContext;

use crate::Workspace;

pub fn schedule_task(
    workspace: &Workspace,
    task_source_kind: TaskSourceKind,
    task_to_resolve: &TemplateType,
    task_cx: &TaskContext,
    omit_history: bool,
    cx: &mut ViewContext<'_, Workspace>,
) {
    match workspace.project.read(cx).ssh_connection_state(cx) {
        None | Some(ConnectionState::Connected) => {}
        Some(
            ConnectionState::Connecting
            | ConnectionState::Disconnected
            | ConnectionState::HeartbeatMissed
            | ConnectionState::Reconnecting,
        ) => {
            log::warn!("Cannot schedule tasks when disconnected from a remote host");
            return;
        }
    }

    if let Some(spawn_in_terminal) =
        task_to_resolve.resolve_task(&task_source_kind.to_id_base(), task_cx)
    {
        schedule_resolved_task(
            workspace,
            task_source_kind,
            spawn_in_terminal,
            omit_history,
            cx,
        );
    }
}

pub fn schedule_resolved_task(
    workspace: &Workspace,
    task_source_kind: TaskSourceKind,
    mut resolved_task: ResolvedTask,
    omit_history: bool,
    cx: &mut ViewContext<'_, Workspace>,
) {
    if let Some(resolved) = resolved_task.resolved.take() {
        if let Some(spawn_in_terminal) = resolved.as_task() {
            if !omit_history {
                resolved_task.resolved = Some(resolved.clone());
                workspace.project().update(cx, |project, cx| {
                    if let Some(task_inventory) =
                        project.task_store().read(cx).task_inventory().cloned()
                    {
                        task_inventory.update(cx, |inventory, _| {
                            inventory.task_scheduled(task_source_kind, resolved_task);
                        })
                    }
                });
            }
            cx.emit(crate::Event::SpawnTask(Box::new(spawn_in_terminal)));
        }
    }
}
