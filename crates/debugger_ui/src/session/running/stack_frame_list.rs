use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use dap::StackFrameId;
use gpui::{
    list, AnyElement, Entity, EventEmitter, FocusHandle, Focusable, ListState, Subscription, Task,
    WeakEntity,
};

use language::Point;
use project::debugger::session::{Session, StackFrame, ThreadId};
use project::{ProjectItem, ProjectPath};
use ui::{prelude::*, Tooltip};
use workspace::Workspace;

#[derive(Debug)]
pub enum StackFrameListEvent {
    SelectedStackFrameChanged(StackFrameId),
}

pub struct StackFrameList {
    list: ListState,
    thread_id: Option<ThreadId>,
    focus_handle: FocusHandle,
    _subscription: Subscription,
    session: Entity<Session>,
    entries: Vec<StackFrameEntry>,
    workspace: WeakEntity<Workspace>,
    current_stack_frame_id: StackFrameId,
    _fetch_stack_frames_task: Option<Task<Result<()>>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StackFrameEntry {
    Normal(dap::StackFrame),
    Collapsed(Vec<dap::StackFrame>),
}

impl StackFrameList {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        session: Entity<Session>,
        cx: &mut Context<Self>,
    ) -> Self {
        let weak_entity = cx.weak_entity();
        let focus_handle = cx.focus_handle();

        let list = ListState::new(
            0,
            gpui::ListAlignment::Top,
            px(1000.),
            move |ix, _window, cx| {
                weak_entity
                    .upgrade()
                    .map(|stack_frame_list| {
                        stack_frame_list.update(cx, |this, cx| this.render_entry(ix, cx))
                    })
                    .unwrap_or(div().into_any())
            },
        );

        let _subscription = cx.observe(&session, |stack_frame_list, _, cx| {
            stack_frame_list.build_entries(cx);
        });

        Self {
            list,
            session,
            workspace,
            focus_handle,
            _subscription,
            thread_id: None,
            entries: Default::default(),
            _fetch_stack_frames_task: None,
            current_stack_frame_id: Default::default(),
        }
    }

    pub(crate) fn set_thread_id(&mut self, thread_id: Option<ThreadId>, cx: &mut Context<Self>) {
        self.thread_id = thread_id;
        self.build_entries(cx);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn entries(&self) -> &Vec<StackFrameEntry> {
        &self.entries
    }

    pub fn stack_frames(&self, cx: &mut App) -> Vec<StackFrame> {
        self.thread_id
            .map(|thread_id| {
                self.session
                    .update(cx, |this, cx| this.stack_frames(thread_id, cx))
            })
            .unwrap_or_default()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn dap_stack_frames(&self, cx: &mut App) -> Vec<dap::StackFrame> {
        self.stack_frames(cx)
            .into_iter()
            .map(|stack_frame| stack_frame.dap.clone())
            .collect()
    }

    pub fn _get_main_stack_frame_id(&self, cx: &mut Context<Self>) -> u64 {
        self.stack_frames(cx)
            .first()
            .map(|stack_frame| stack_frame.dap.id)
            .unwrap_or(0)
    }

    pub fn current_stack_frame_id(&self) -> u64 {
        self.current_stack_frame_id
    }

    pub fn _current_thread_id(&self) -> Option<ThreadId> {
        self.thread_id
    }

    fn build_entries(&mut self, cx: &mut Context<Self>) {
        let mut entries = Vec::new();
        let mut collapsed_entries = Vec::new();

        for stack_frame in &self.stack_frames(cx) {
            match stack_frame.dap.presentation_hint {
                Some(dap::StackFramePresentationHint::Deemphasize) => {
                    collapsed_entries.push(stack_frame.dap.clone());
                }
                _ => {
                    let collapsed_entries = std::mem::take(&mut collapsed_entries);
                    if !collapsed_entries.is_empty() {
                        entries.push(StackFrameEntry::Collapsed(collapsed_entries.clone()));
                    }

                    entries.push(StackFrameEntry::Normal(stack_frame.dap.clone()));
                }
            }
        }

        let collapsed_entries = std::mem::take(&mut collapsed_entries);
        if !collapsed_entries.is_empty() {
            entries.push(StackFrameEntry::Collapsed(collapsed_entries.clone()));
        }

        std::mem::swap(&mut self.entries, &mut entries);
        self.list.reset(self.entries.len());
        cx.notify();
    }

    // fn fetch_stack_frames(
    //     &mut self,
    //     go_to_stack_frame: bool,
    //     window: &Window,
    //     cx: &mut Context<Self>,
    // ) {
    //     // If this is a remote debug session we never need to fetch stack frames ourselves
    //     // because the host will fetch and send us stack frames whenever there's a stop event
    //     if self.dap_store.read(cx).as_remote().is_some() {
    //         return;
    //     }

    //     let task = self.dap_store.update(cx, |store, cx| {
    //         store.stack_frames(&self.client_id, self.thread_id, cx)
    //     });

    //     self.fetch_stack_frames_task = Some(cx.spawn_in(window, |this, mut cx| async move {
    //         let mut stack_frames = task.await?;

    //         let task = this.update_in(&mut cx, |this, window, cx| {
    //             std::mem::swap(&mut this.stack_frames, &mut stack_frames);

    //             this.build_entries();

    //             cx.emit(StackFrameListEvent::StackFramesUpdated);

    //             let stack_frame = this
    //                 .stack_frames
    //                 .first()
    //                 .cloned()
    //                 .ok_or_else(|| anyhow!("No stack frame found to select"))?;

    //             anyhow::Ok(this.select_stack_frame(&stack_frame, go_to_stack_frame, window, cx))
    //         })?;

    //         task?.await?;

    //         this.update(&mut cx, |this, _| {
    //             this.fetch_stack_frames_task.take();
    //         })
    //     }));
    // }

    pub fn select_stack_frame(
        &mut self,
        stack_frame: &dap::StackFrame,
        go_to_stack_frame: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.current_stack_frame_id = stack_frame.id;

        cx.emit(StackFrameListEvent::SelectedStackFrameChanged(
            stack_frame.id,
        ));
        cx.notify();

        if !go_to_stack_frame {
            return Task::ready(Ok(()));
        };

        let row = (stack_frame.line.saturating_sub(1)) as u32;

        let Some(abs_path) = self.abs_path_from_stack_frame(&stack_frame, cx) else {
            return Task::ready(Err(anyhow!("Project path not found")));
        };

        cx.spawn_in(window, {
            // let client_id = self.client_id;
            move |this, mut cx| async move {
                let buffer = this
                    .update(&mut cx, |this, cx| {
                        this.workspace.update(cx, |workspace, cx| {
                            workspace
                                .project()
                                .update(cx, |this, cx| this.open_local_buffer(abs_path.clone(), cx))
                        })
                    })??
                    .await?;
                let position = buffer.update(&mut cx, |this, _| {
                    this.snapshot().anchor_before(Point::new(row, 0))
                })?;
                this.update_in(&mut cx, |this, window, cx| {
                    this.workspace.update(cx, |workspace, cx| {
                        let project_path = buffer.read(cx).project_path(cx).ok_or_else(|| {
                            anyhow!("Could not select a stack frame for unnamed buffer")
                        })?;
                        Result::<_, anyhow::Error>::Ok(workspace.open_path_preview(
                            project_path,
                            None,
                            false,
                            true,
                            window,
                            cx,
                        ))
                    })
                })???
                .await?;

                // TODO(debugger): make this work again
                this.update(&mut cx, |this, cx| {
                    this.workspace.update(cx, |workspace, cx| {
                        let breakpoint_store = workspace.project().read(cx).breakpoint_store();

                        breakpoint_store.update(cx, |store, cx| {
                            let _ = store.set_active_position(Some((abs_path, position)));
                            cx.notify();
                        })
                    })
                })?
            }
        })
    }

    fn abs_path_from_stack_frame(
        &self,
        stack_frame: &dap::StackFrame,
        cx: &mut Context<Self>,
    ) -> Option<Arc<Path>> {
        stack_frame.source.as_ref().and_then(|s| {
            s.path
                .as_deref()
                .map(|path| Arc::<Path>::from(Path::new(path)))
        })
    }

    pub fn restart_stack_frame(&mut self, stack_frame_id: u64, cx: &mut Context<Self>) {
        self.session.update(cx, |state, cx| {
            state.restart_stack_frame(stack_frame_id, cx)
        });
    }

    fn render_normal_entry(
        &self,
        stack_frame: &dap::StackFrame,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let source = stack_frame.source.clone();
        let is_selected_frame = stack_frame.id == self.current_stack_frame_id;

        let formatted_path = format!(
            "{}:{}",
            source.clone().and_then(|s| s.name).unwrap_or_default(),
            stack_frame.line,
        );

        let supports_frame_restart = self
            .session
            .read(cx)
            .capabilities()
            .supports_restart_frame
            .unwrap_or_default();

        let origin = stack_frame
            .source
            .to_owned()
            .and_then(|source| source.origin);

        h_flex()
            .rounded_md()
            .justify_between()
            .w_full()
            .group("")
            .id(("stack-frame", stack_frame.id))
            .tooltip({
                let formatted_path = formatted_path.clone();
                move |_window, app| {
                    app.new(|_| {
                        let mut tooltip = Tooltip::new(formatted_path.clone());

                        if let Some(origin) = &origin {
                            tooltip = tooltip.meta(origin);
                        }

                        tooltip
                    })
                    .into()
                }
            })
            .p_1()
            .when(is_selected_frame, |this| {
                this.bg(cx.theme().colors().element_hover)
            })
            .on_click(cx.listener({
                let stack_frame = stack_frame.clone();
                move |this, _, window, cx| {
                    this.select_stack_frame(&stack_frame, true, window, cx)
                        .detach_and_log_err(cx);
                }
            }))
            .hover(|style| style.bg(cx.theme().colors().element_hover).cursor_pointer())
            .child(
                v_flex()
                    .child(
                        h_flex()
                            .gap_0p5()
                            .text_ui_sm(cx)
                            .truncate()
                            .child(stack_frame.name.clone())
                            .child(formatted_path),
                    )
                    .child(
                        h_flex()
                            .text_ui_xs(cx)
                            .truncate()
                            .text_color(cx.theme().colors().text_muted)
                            .when_some(source.and_then(|s| s.path), |this, path| this.child(path)),
                    ),
            )
            .when(
                supports_frame_restart && stack_frame.can_restart.unwrap_or(true),
                |this| {
                    this.child(
                        h_flex()
                            .id(("restart-stack-frame", stack_frame.id))
                            .visible_on_hover("")
                            .absolute()
                            .right_2()
                            .overflow_hidden()
                            .rounded_md()
                            .border_1()
                            .border_color(cx.theme().colors().element_selected)
                            .bg(cx.theme().colors().element_background)
                            .hover(|style| {
                                style
                                    .bg(cx.theme().colors().ghost_element_hover)
                                    .cursor_pointer()
                            })
                            .child(
                                IconButton::new(
                                    ("restart-stack-frame", stack_frame.id),
                                    IconName::DebugRestart,
                                )
                                .icon_size(IconSize::Small)
                                .on_click(cx.listener({
                                    let stack_frame_id = stack_frame.id;
                                    move |this, _, _window, cx| {
                                        this.restart_stack_frame(stack_frame_id, cx);
                                    }
                                }))
                                .tooltip(move |window, cx| {
                                    Tooltip::text("Restart Stack Frame")(window, cx)
                                }),
                            ),
                    )
                },
            )
            .into_any()
    }

    pub fn expand_collapsed_entry(
        &mut self,
        ix: usize,
        stack_frames: &Vec<dap::StackFrame>,
        cx: &mut Context<Self>,
    ) {
        self.entries.splice(
            ix..ix + 1,
            stack_frames
                .iter()
                .map(|frame| StackFrameEntry::Normal(frame.clone())),
        );
        self.list.reset(self.entries.len());
        cx.notify();
    }

    fn render_collapsed_entry(
        &self,
        ix: usize,
        stack_frames: &Vec<dap::StackFrame>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let first_stack_frame = &stack_frames[0];

        h_flex()
            .rounded_md()
            .justify_between()
            .w_full()
            .group("")
            .id(("stack-frame", first_stack_frame.id))
            .p_1()
            .on_click(cx.listener({
                let stack_frames = stack_frames.clone();
                move |this, _, _window, cx| {
                    this.expand_collapsed_entry(ix, &stack_frames, cx);
                }
            }))
            .hover(|style| style.bg(cx.theme().colors().element_hover).cursor_pointer())
            .child(
                v_flex()
                    .text_ui_sm(cx)
                    .truncate()
                    .text_color(cx.theme().colors().text_muted)
                    .child(format!(
                        "Show {} more{}",
                        stack_frames.len(),
                        first_stack_frame
                            .source
                            .as_ref()
                            .and_then(|source| source.origin.as_ref())
                            .map_or(String::new(), |origin| format!(": {}", origin))
                    )),
            )
            .into_any()
    }

    fn render_entry(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        match &self.entries[ix] {
            StackFrameEntry::Normal(stack_frame) => self.render_normal_entry(stack_frame, cx),
            StackFrameEntry::Collapsed(stack_frames) => {
                self.render_collapsed_entry(ix, stack_frames, cx)
            }
        }
    }
}

impl Render for StackFrameList {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p_1()
            .child(list(self.list.clone()).size_full())
    }
}

impl Focusable for StackFrameList {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<StackFrameListEvent> for StackFrameList {}
