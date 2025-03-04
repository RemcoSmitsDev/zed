use crate::{
    debugger_panel::DebugPanel,
    session::running::stack_frame_list::StackFrameEntry,
    tests::{active_debug_session_panel, init_test, init_test_workspace},
};
use dap::{
    requests::{Disconnect, StackTrace, Threads},
    StackFrame,
};
use editor::{Editor, ToPoint as _};
use gpui::{BackgroundExecutor, Render, TestAppContext, VisualTestContext};
use project::{FakeFs, Project};
use serde_json::json;
use std::sync::Arc;
use unindent::Unindent as _;

#[gpui::test]
async fn test_fetch_initial_stack_frames_and_go_to_stack_frame(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    let test_file_content = r#"
        import { SOME_VALUE } './module.js';

        console.log(SOME_VALUE);
    "#
    .unindent();

    let module_file_content = r#"
        export SOME_VALUE = 'some value';
    "#
    .unindent();

    fs.insert_tree(
        "/project",
        json!({
           "src": {
               "test.js": test_file_content,
               "module.js": module_file_content,
           }
        }),
    )
    .await;

    let project = Project::test(fs, ["/project".as_ref()], cx).await;
    let workspace = init_test_workspace(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(*workspace, cx);

    let task = project.update(cx, |project, cx| {
        project.start_debug_session(dap::test_config(), cx)
    });

    let session = task.await.unwrap();
    let client = session.update(cx, |session, _| session.adapter_client().unwrap());

    client
        .on_request::<Threads, _>(move |_, _| {
            Ok(dap::ThreadsResponse {
                threads: vec![dap::Thread {
                    id: 1,
                    name: "Thread 1".into(),
                }],
            })
        })
        .await;

    let stack_frames = vec![
        StackFrame {
            id: 1,
            name: "Stack Frame 1".into(),
            source: Some(dap::Source {
                name: Some("test.js".into()),
                path: Some("/project/src/test.js".into()),
                source_reference: None,
                presentation_hint: None,
                origin: None,
                sources: None,
                adapter_data: None,
                checksums: None,
            }),
            line: 3,
            column: 1,
            end_line: None,
            end_column: None,
            can_restart: None,
            instruction_pointer_reference: None,
            module_id: None,
            presentation_hint: None,
        },
        StackFrame {
            id: 2,
            name: "Stack Frame 2".into(),
            source: Some(dap::Source {
                name: Some("module.js".into()),
                path: Some("/project/src/module.js".into()),
                source_reference: None,
                presentation_hint: None,
                origin: None,
                sources: None,
                adapter_data: None,
                checksums: None,
            }),
            line: 1,
            column: 1,
            end_line: None,
            end_column: None,
            can_restart: None,
            instruction_pointer_reference: None,
            module_id: None,
            presentation_hint: None,
        },
    ];

    client
        .on_request::<StackTrace, _>({
            let stack_frames = Arc::new(stack_frames.clone());
            move |_, args| {
                assert_eq!(1, args.thread_id);

                Ok(dap::StackTraceResponse {
                    stack_frames: (*stack_frames).clone(),
                    total_frames: None,
                })
            }
        })
        .await;

    client.on_request::<Disconnect, _>(move |_, _| Ok(())).await;

    client
        .fake_event(dap::messages::Events::Stopped(dap::StoppedEvent {
            reason: dap::StoppedEventReason::Pause,
            description: None,
            thread_id: Some(1),
            preserve_focus_hint: None,
            text: None,
            all_threads_stopped: None,
            hit_breakpoint_ids: None,
        }))
        .await;

    cx.run_until_parked();

    workspace.update(cx, |workspace, window, cx| {
        let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
        debug_panel.update(cx, |panel, cx| {
            panel.render(window, cx);
        });
    });

    let stack_frame_list = workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let active_debug_panel_item = debug_panel
                .update(cx, |this, cx| this.active_session(cx))
                .unwrap();

            active_debug_panel_item
                .read(cx)
                .mode()
                .as_running()
                .unwrap()
                .read(cx)
                .stack_frame_list();

            active_debug_panel_item.update(cx, |debug_panel_item, cx| {
                let stack_frame_list = debug_panel_item
                    .mode()
                    .as_running()
                    .unwrap()
                    .update(cx, |state, _| state.stack_frame_list().clone());

                stack_frame_list.update(cx, |list, cx| {
                    list.stack_frames(cx);
                });

                stack_frame_list
            })
        })
        .unwrap();

    cx.run_until_parked();

    let (stack_frame_list, stack_frame_id) = stack_frame_list.update(cx, |list, cx| {
        (
            list.stack_frames(cx)
                .into_iter()
                .map(|frame| frame.dap)
                .collect::<Vec<_>>(),
            list.current_stack_frame_id(),
        )
    });

    assert_eq!(Some(1), stack_frame_id);
    assert_eq!(stack_frames, stack_frame_list);

    let shutdown_session = project.update(cx, |project, cx| {
        project.dap_store().update(cx, |dap_store, cx| {
            dap_store.shutdown_session(&session.read(cx).session_id(), cx)
        })
    });

    shutdown_session.await.unwrap();
}

#[gpui::test]
async fn test_select_stack_frame(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    let test_file_content = r#"
        import { SOME_VALUE } './module.js';

        console.log(SOME_VALUE);
    "#
    .unindent();

    let module_file_content = r#"
        export SOME_VALUE = 'some value';
    "#
    .unindent();

    fs.insert_tree(
        "/project",
        json!({
           "src": {
               "test.js": test_file_content,
               "module.js": module_file_content,
           }
        }),
    )
    .await;

    let project = Project::test(fs, ["/project".as_ref()], cx).await;
    let workspace = init_test_workspace(&project, cx).await;
    workspace.update(cx, |workspace, window, cx| {
        workspace.toggle_dock(workspace::dock::DockPosition::Bottom, window, cx);
    });

    let cx = &mut VisualTestContext::from_window(*workspace, cx);

    let task = project.update(cx, |project, cx| {
        project.start_debug_session(dap::test_config(), cx)
    });

    let session = task.await.unwrap();
    let client = session.update(cx, |session, _| session.adapter_client().unwrap());

    client
        .on_request::<Threads, _>(move |_, _| {
            Ok(dap::ThreadsResponse {
                threads: vec![dap::Thread {
                    id: 1,
                    name: "Thread 1".into(),
                }],
            })
        })
        .await;

    let stack_frames = vec![
        StackFrame {
            id: 1,
            name: "Stack Frame 1".into(),
            source: Some(dap::Source {
                name: Some("test.js".into()),
                path: Some("/project/src/test.js".into()),
                source_reference: None,
                presentation_hint: None,
                origin: None,
                sources: None,
                adapter_data: None,
                checksums: None,
            }),
            line: 3,
            column: 1,
            end_line: None,
            end_column: None,
            can_restart: None,
            instruction_pointer_reference: None,
            module_id: None,
            presentation_hint: None,
        },
        StackFrame {
            id: 2,
            name: "Stack Frame 2".into(),
            source: Some(dap::Source {
                name: Some("module.js".into()),
                path: Some("/project/src/module.js".into()),
                source_reference: None,
                presentation_hint: None,
                origin: None,
                sources: None,
                adapter_data: None,
                checksums: None,
            }),
            line: 1,
            column: 1,
            end_line: None,
            end_column: None,
            can_restart: None,
            instruction_pointer_reference: None,
            module_id: None,
            presentation_hint: None,
        },
    ];

    client
        .on_request::<StackTrace, _>({
            let stack_frames = Arc::new(stack_frames.clone());
            move |_, args| {
                assert_eq!(1, args.thread_id);

                Ok(dap::StackTraceResponse {
                    stack_frames: (*stack_frames).clone(),
                    total_frames: None,
                })
            }
        })
        .await;

    client.on_request::<Disconnect, _>(move |_, _| Ok(())).await;

    client
        .fake_event(dap::messages::Events::Stopped(dap::StoppedEvent {
            reason: dap::StoppedEventReason::Pause,
            description: None,
            thread_id: Some(1),
            preserve_focus_hint: None,
            text: None,
            all_threads_stopped: None,
            hit_breakpoint_ids: None,
        }))
        .await;

    cx.run_until_parked();

    workspace
        .update(cx, |workspace, window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let active_debug_panel_item = debug_panel
                .update(cx, |this, cx| this.active_session(cx))
                .unwrap();

            active_debug_panel_item.update(cx, |debug_panel_item, cx| {
                let stack_frame_list = debug_panel_item
                    .mode()
                    .as_running()
                    .unwrap()
                    .update(cx, |state, _| state.stack_frame_list().clone());

                let (stack_frame_list, stack_frame_id) = stack_frame_list.update(cx, |list, cx| {
                    (
                        list.stack_frames(cx)
                            .into_iter()
                            .map(|frame| frame.dap)
                            .collect::<Vec<_>>(),
                        list.current_stack_frame_id(),
                    )
                });

                assert_eq!(stack_frames, stack_frame_list);
                assert_eq!(Some(1), stack_frame_id);
            });

            let editors = workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>();
            assert_eq!(1, editors.len());

            let project_path = editors[0]
                .update(cx, |editor, cx| editor.project_path(cx))
                .unwrap();
            assert_eq!("src/test.js", project_path.path.to_string_lossy());
            assert_eq!(test_file_content, editors[0].read(cx).text(cx));
            assert_eq!(
                vec![2..3],
                editors[0].update(cx, |editor, cx| {
                    let snapshot = editor.snapshot(window, cx);

                    editor
                        .highlighted_rows::<editor::DebugCurrentRowHighlight>()
                        .map(|(range, _)| {
                            let start = range.start.to_point(&snapshot.buffer_snapshot);
                            let end = range.end.to_point(&snapshot.buffer_snapshot);
                            start.row..end.row
                        })
                        .collect::<Vec<_>>()
                })
            );
        })
        .unwrap();

    let stack_frame_list = workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let active_debug_panel_item = debug_panel
                .update(cx, |this, cx| this.active_session(cx))
                .unwrap();

            active_debug_panel_item
                .read(cx)
                .mode()
                .as_running()
                .unwrap()
                .read(cx)
                .stack_frame_list()
                .clone()
        })
        .unwrap();

    // select second stack frame
    stack_frame_list
        .update_in(cx, |stack_frame_list, window, cx| {
            stack_frame_list.select_stack_frame(&stack_frames[1], true, window, cx)
        })
        .await
        .unwrap();

    workspace
        .update(cx, |workspace, window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let active_debug_panel_item = debug_panel
                .update(cx, |this, cx| this.active_session(cx))
                .unwrap();

            active_debug_panel_item.update(cx, |debug_panel_item, cx| {
                let (stack_frame_list, stack_frame_id) = debug_panel_item
                    .mode()
                    .as_running()
                    .unwrap()
                    .update(cx, |state, cx| {
                        state.stack_frame_list().update(cx, |list, cx| {
                            (
                                list.stack_frames(cx)
                                    .into_iter()
                                    .map(|frame| frame.dap)
                                    .collect::<Vec<_>>(),
                                list.current_stack_frame_id(),
                            )
                        })
                    });

                assert_eq!(Some(2), stack_frame_id);
                assert_eq!(stack_frames, stack_frame_list);
            });

            let editors = workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>();
            assert_eq!(1, editors.len());

            let project_path = editors[0]
                .update(cx, |editor, cx| editor.project_path(cx))
                .unwrap();
            assert_eq!("src/module.js", project_path.path.to_string_lossy());
            assert_eq!(module_file_content, editors[0].read(cx).text(cx));
            assert_eq!(
                vec![0..1],
                editors[0].update(cx, |editor, cx| {
                    let snapshot = editor.snapshot(window, cx);

                    editor
                        .highlighted_rows::<editor::DebugCurrentRowHighlight>()
                        .map(|(range, _)| {
                            let start = range.start.to_point(&snapshot.buffer_snapshot);
                            let end = range.end.to_point(&snapshot.buffer_snapshot);
                            start.row..end.row
                        })
                        .collect::<Vec<_>>()
                })
            );
        })
        .unwrap();

    let shutdown_session = project.update(cx, |project, cx| {
        project.dap_store().update(cx, |dap_store, cx| {
            dap_store.shutdown_session(&session.read(cx).session_id(), cx)
        })
    });

    shutdown_session.await.unwrap();
}

#[gpui::test]
async fn test_collapsed_entries(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    let test_file_content = r#"
        import { SOME_VALUE } './module.js';

        console.log(SOME_VALUE);
    "#
    .unindent();

    let module_file_content = r#"
        export SOME_VALUE = 'some value';
    "#
    .unindent();

    fs.insert_tree(
        "/project",
        json!({
           "src": {
               "test.js": test_file_content,
               "module.js": module_file_content,
           }
        }),
    )
    .await;

    let project = Project::test(fs, ["/project".as_ref()], cx).await;
    let workspace = init_test_workspace(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(*workspace, cx);

    let task = project.update(cx, |project, cx| {
        project.start_debug_session(dap::test_config(), cx)
    });

    let session = task.await.unwrap();
    let client = session.update(cx, |session, _| session.adapter_client().unwrap());

    let stack_frames = vec![
        StackFrame {
            id: 1,
            name: "Stack Frame 1".into(),
            source: Some(dap::Source {
                name: Some("test.js".into()),
                path: Some("/project/src/test.js".into()),
                source_reference: None,
                presentation_hint: None,
                origin: None,
                sources: None,
                adapter_data: None,
                checksums: None,
            }),
            line: 3,
            column: 1,
            end_line: None,
            end_column: None,
            can_restart: None,
            instruction_pointer_reference: None,
            module_id: None,
            presentation_hint: None,
        },
        StackFrame {
            id: 2,
            name: "Stack Frame 2".into(),
            source: Some(dap::Source {
                name: Some("module.js".into()),
                path: Some("/project/src/module.js".into()),
                source_reference: None,
                presentation_hint: None,
                origin: Some("ignored".into()),
                sources: None,
                adapter_data: None,
                checksums: None,
            }),
            line: 1,
            column: 1,
            end_line: None,
            end_column: None,
            can_restart: None,
            instruction_pointer_reference: None,
            module_id: None,
            presentation_hint: Some(dap::StackFramePresentationHint::Deemphasize),
        },
        StackFrame {
            id: 3,
            name: "Stack Frame 3".into(),
            source: Some(dap::Source {
                name: Some("module.js".into()),
                path: Some("/project/src/module.js".into()),
                source_reference: None,
                presentation_hint: None,
                origin: Some("ignored".into()),
                sources: None,
                adapter_data: None,
                checksums: None,
            }),
            line: 1,
            column: 1,
            end_line: None,
            end_column: None,
            can_restart: None,
            instruction_pointer_reference: None,
            module_id: None,
            presentation_hint: Some(dap::StackFramePresentationHint::Deemphasize),
        },
        StackFrame {
            id: 4,
            name: "Stack Frame 4".into(),
            source: Some(dap::Source {
                name: Some("module.js".into()),
                path: Some("/project/src/module.js".into()),
                source_reference: None,
                presentation_hint: None,
                origin: None,
                sources: None,
                adapter_data: None,
                checksums: None,
            }),
            line: 1,
            column: 1,
            end_line: None,
            end_column: None,
            can_restart: None,
            instruction_pointer_reference: None,
            module_id: None,
            presentation_hint: None,
        },
    ];

    client
        .on_request::<StackTrace, _>({
            let stack_frames = Arc::new(stack_frames.clone());
            move |_, args| {
                assert_eq!(1, args.thread_id);

                Ok(dap::StackTraceResponse {
                    stack_frames: (*stack_frames).clone(),
                    total_frames: None,
                })
            }
        })
        .await;

    client.on_request::<Disconnect, _>(move |_, _| Ok(())).await;

    client
        .fake_event(dap::messages::Events::Stopped(dap::StoppedEvent {
            reason: dap::StoppedEventReason::Pause,
            description: None,
            thread_id: Some(1),
            preserve_focus_hint: None,
            text: None,
            all_threads_stopped: None,
            hit_breakpoint_ids: None,
        }))
        .await;

    cx.run_until_parked();

    active_debug_session_panel(workspace, cx).update(cx, |debug_panel_item, cx| {
        debug_panel_item
            .mode()
            .as_running()
            .unwrap()
            .update(cx, |state, cx| {
                state.stack_frame_list().update(cx, |stack_frame_list, cx| {
                    assert_eq!(
                        &vec![
                            StackFrameEntry::Normal(stack_frames[0].clone()),
                            StackFrameEntry::Collapsed(vec![
                                stack_frames[1].clone(),
                                stack_frames[2].clone()
                            ]),
                            StackFrameEntry::Normal(stack_frames[3].clone()),
                        ],
                        stack_frame_list.entries()
                    );

                    stack_frame_list.expand_collapsed_entry(
                        1,
                        &vec![stack_frames[1].clone(), stack_frames[2].clone()],
                        cx,
                    );

                    assert_eq!(
                        &vec![
                            StackFrameEntry::Normal(stack_frames[0].clone()),
                            StackFrameEntry::Normal(stack_frames[1].clone()),
                            StackFrameEntry::Normal(stack_frames[2].clone()),
                            StackFrameEntry::Normal(stack_frames[3].clone()),
                        ],
                        stack_frame_list.entries()
                    );
                })
            });
    });
    let shutdown_session = project.update(cx, |project, cx| {
        project.dap_store().update(cx, |dap_store, cx| {
            dap_store.shutdown_session(&session.read(cx).session_id(), cx)
        })
    });

    shutdown_session.await.unwrap();
}
