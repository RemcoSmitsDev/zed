use call::ActiveCall;
use dap::requests::{Disconnect, Initialize, Launch, Scopes, StackTrace, Variables};
use dap::{Scope, Variable};
use debugger_ui::{debugger_panel::DebugPanel, variable_list::VariableContainer};
use gpui::{TestAppContext, View, VisualTestContext};
use std::sync::Arc;
use workspace::{dock::Panel, Workspace};

use super::TestServer;

pub fn init_test(cx: &mut gpui::TestAppContext) {
    if std::env::var("RUST_LOG").is_ok() {
        env_logger::try_init().ok();
    }

    cx.update(|cx| {
        theme::init(theme::LoadThemes::JustBase, cx);
        command_palette_hooks::init(cx);
        language::init(cx);
        workspace::init_settings(cx);
        project::Project::init_settings(cx);
        debugger_ui::init(cx);
        editor::init(cx);
    });
}

pub async fn add_debugger_panel(workspace: &View<Workspace>, cx: &mut VisualTestContext) {
    let debugger_panel = workspace
        .update(cx, |_, cx| cx.spawn(DebugPanel::load))
        .await
        .unwrap();

    workspace.update(cx, |workspace, cx| {
        workspace.add_panel(debugger_panel, cx);
    });
}

#[gpui::test]
async fn test_debug_panel_item_opens_on_remote(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let executor = cx_a.executor();
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;

    init_test(cx_a);
    init_test(cx_b);

    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    let (project_a, _worktree_id) = client_a.build_local_project("/a", cx_a).await;
    active_call_a
        .update(cx_a, |call, cx| call.set_location(Some(&project_a), cx))
        .await
        .unwrap();

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    active_call_b
        .update(cx_b, |call, cx| call.set_location(Some(&project_b), cx))
        .await
        .unwrap();

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    add_debugger_panel(&workspace_a, cx_a).await;
    add_debugger_panel(&workspace_b, cx_b).await;

    let task = project_a.update(cx_a, |project, cx| {
        project.dap_store().update(cx, |store, cx| {
            store.start_debug_session(
                dap::DebugAdapterConfig {
                    label: "test config".into(),
                    kind: dap::DebugAdapterKind::Fake,
                    request: dap::DebugRequestType::Launch,
                    program: None,
                    cwd: None,
                    initialize_args: None,
                },
                cx,
            )
        })
    });

    let (session, client) = task.await.unwrap();

    client
        .on_request::<Initialize, _>(move |_, _| {
            Ok(dap::Capabilities {
                supports_step_back: Some(false),
                ..Default::default()
            })
        })
        .await;

    client.on_request::<Launch, _>(move |_, _| Ok(())).await;

    client
        .on_request::<StackTrace, _>(move |_, _| {
            Ok(dap::StackTraceResponse {
                stack_frames: Vec::default(),
                total_frames: None,
            })
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

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    workspace_b.update(cx_b, |workspace, cx| {
        let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
        let active_debug_panel_item = debug_panel
            .update(cx, |this, cx| this.active_debug_panel_item(cx))
            .unwrap();

        assert_eq!(
            1,
            debug_panel.update(cx, |this, cx| this.pane().unwrap().read(cx).items_len())
        );
        assert_eq!(client.id(), active_debug_panel_item.read(cx).client_id());
        assert_eq!(1, active_debug_panel_item.read(cx).thread_id());
    });

    let shutdown_client = project_a.update(cx_a, |project, cx| {
        project.dap_store().update(cx, |dap_store, cx| {
            dap_store.shutdown_session(&session.read(cx).id(), cx)
        })
    });

    shutdown_client.await.unwrap();
}

#[gpui::test]
async fn test_active_debug_panel_item_set_on_join_project(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let executor = cx_a.executor();
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;

    init_test(cx_a);
    init_test(cx_b);

    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    let (project_a, _worktree_id) = client_a.build_local_project("/a", cx_a).await;
    active_call_a
        .update(cx_a, |call, cx| call.set_location(Some(&project_a), cx))
        .await
        .unwrap();

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);

    add_debugger_panel(&workspace_a, cx_a).await;

    let task = project_a.update(cx_a, |project, cx| {
        project.dap_store().update(cx, |store, cx| {
            store.start_debug_session(
                dap::DebugAdapterConfig {
                    label: "test config".into(),
                    kind: dap::DebugAdapterKind::Fake,
                    request: dap::DebugRequestType::Launch,
                    program: None,
                    cwd: None,
                    initialize_args: None,
                },
                cx,
            )
        })
    });

    let (session, client) = task.await.unwrap();

    client
        .on_request::<Initialize, _>(move |_, _| {
            Ok(dap::Capabilities {
                supports_step_back: Some(false),
                ..Default::default()
            })
        })
        .await;

    client.on_request::<Launch, _>(move |_, _| Ok(())).await;

    client
        .on_request::<StackTrace, _>(move |_, _| {
            Ok(dap::StackTraceResponse {
                stack_frames: Vec::default(),
                total_frames: None,
            })
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

    // Give client_a time to send a debug panel item to collab server
    cx_a.run_until_parked();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);
    add_debugger_panel(&workspace_b, cx_b).await;

    active_call_b
        .update(cx_b, |call, cx| call.set_location(Some(&project_b), cx))
        .await
        .unwrap();

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    workspace_b.update(cx_b, |workspace, cx| {
        let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
        let active_debug_panel_item = debug_panel
            .update(cx, |this, cx| this.active_debug_panel_item(cx))
            .unwrap();

        assert_eq!(
            1,
            debug_panel.update(cx, |this, cx| this.pane().unwrap().read(cx).items_len())
        );
        assert_eq!(client.id(), active_debug_panel_item.read(cx).client_id());
        assert_eq!(1, active_debug_panel_item.read(cx).thread_id());
    });

    let shutdown_client = project_a.update(cx_a, |project, cx| {
        project.dap_store().update(cx, |dap_store, cx| {
            dap_store.shutdown_session(&session.read(cx).id(), cx)
        })
    });

    shutdown_client.await.unwrap();

    cx_b.run_until_parked();

    // assert we don't have a debug panel item anymore because the client shutdown
    workspace_b.update(cx_b, |workspace, cx| {
        let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();

        debug_panel.update(cx, |this, cx| {
            assert!(this.active_debug_panel_item(cx).is_none());
            assert_eq!(0, this.pane().unwrap().read(cx).items_len());
        });
    });
}

#[gpui::test]
async fn test_debug_panel_remote_button_presses(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let executor = cx_a.executor();
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;

    init_test(cx_a);
    init_test(cx_b);

    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    let (project_a, _worktree_id) = client_a.build_local_project("/a", cx_a).await;
    active_call_a
        .update(cx_a, |call, cx| call.set_location(Some(&project_a), cx))
        .await
        .unwrap();

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    active_call_b
        .update(cx_b, |call, cx| call.set_location(Some(&project_b), cx))
        .await
        .unwrap();

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    add_debugger_panel(&workspace_a, cx_a).await;
    add_debugger_panel(&workspace_b, cx_b).await;

    let task = project_a.update(cx_a, |project, cx| {
        project.dap_store().update(cx, |store, cx| {
            store.start_debug_session(
                dap::DebugAdapterConfig {
                    label: "test config".into(),
                    kind: dap::DebugAdapterKind::Fake,
                    request: dap::DebugRequestType::Launch,
                    program: None,
                    cwd: None,
                    initialize_args: None,
                },
                cx,
            )
        })
    });

    let (_, client) = task.await.unwrap();

    client
        .on_request::<Initialize, _>(move |_, _| {
            Ok(dap::Capabilities {
                supports_step_back: Some(true),
                ..Default::default()
            })
        })
        .await;

    client.on_request::<Launch, _>(move |_, _| Ok(())).await;

    client
        .on_request::<StackTrace, _>(move |_, _| {
            Ok(dap::StackTraceResponse {
                stack_frames: Vec::default(),
                total_frames: None,
            })
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

    client
        .on_request::<dap::requests::Continue, _>(move |_, _| {
            Ok(dap::ContinueResponse {
                all_threads_continued: Some(true),
            })
        })
        .await;

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let remote_debug_item = workspace_b.update(cx_b, |workspace, cx| {
        let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
        let active_debug_panel_item = debug_panel
            .update(cx, |this, cx| this.active_debug_panel_item(cx))
            .unwrap();

        assert_eq!(
            1,
            debug_panel.update(cx, |this, cx| this.pane().unwrap().read(cx).items_len())
        );
        assert_eq!(client.id(), active_debug_panel_item.read(cx).client_id());
        assert_eq!(1, active_debug_panel_item.read(cx).thread_id());
        active_debug_panel_item
    });

    let local_debug_item = workspace_a.update(cx_a, |workspace, cx| {
        let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
        let active_debug_panel_item = debug_panel
            .update(cx, |this, cx| this.active_debug_panel_item(cx))
            .unwrap();

        assert_eq!(
            1,
            debug_panel.update(cx, |this, cx| this.pane().unwrap().read(cx).items_len())
        );
        assert_eq!(client.id(), active_debug_panel_item.read(cx).client_id());
        assert_eq!(1, active_debug_panel_item.read(cx).thread_id());
        active_debug_panel_item
    });

    remote_debug_item.update(cx_b, |this, cx| {
        this.continue_thread(cx);
    });

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    local_debug_item.update(cx_a, |debug_panel_item, cx| {
        assert_eq!(
            debugger_ui::debugger_panel::ThreadStatus::Running,
            debug_panel_item.thread_state().read(cx).status,
        );
    });

    remote_debug_item.update(cx_b, |debug_panel_item, cx| {
        assert_eq!(
            debugger_ui::debugger_panel::ThreadStatus::Running,
            debug_panel_item.thread_state().read(cx).status,
        );
    });

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

    client
        .on_request::<StackTrace, _>(move |_, _| {
            Ok(dap::StackTraceResponse {
                stack_frames: Vec::default(),
                total_frames: None,
            })
        })
        .await;

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    local_debug_item.update(cx_a, |debug_panel_item, cx| {
        assert_eq!(
            debugger_ui::debugger_panel::ThreadStatus::Stopped,
            debug_panel_item.thread_state().read(cx).status,
        );
    });

    remote_debug_item.update(cx_b, |debug_panel_item, cx| {
        assert_eq!(
            debugger_ui::debugger_panel::ThreadStatus::Stopped,
            debug_panel_item.thread_state().read(cx).status,
        );
    });

    client
        .on_request::<dap::requests::Continue, _>(move |_, _| {
            Ok(dap::ContinueResponse {
                all_threads_continued: Some(true),
            })
        })
        .await;

    local_debug_item.update(cx_a, |this, cx| {
        this.continue_thread(cx);
    });

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    local_debug_item.update(cx_a, |debug_panel_item, cx| {
        assert_eq!(
            debugger_ui::debugger_panel::ThreadStatus::Running,
            debug_panel_item.thread_state().read(cx).status,
        );
    });

    remote_debug_item.update(cx_b, |debug_panel_item, cx| {
        assert_eq!(
            debugger_ui::debugger_panel::ThreadStatus::Running,
            debug_panel_item.thread_state().read(cx).status,
        );
    });

    client
        .on_request::<dap::requests::Pause, _>(move |_, _| Ok(()))
        .await;

    client
        .on_request::<StackTrace, _>(move |_, _| {
            Ok(dap::StackTraceResponse {
                stack_frames: Vec::default(),
                total_frames: None,
            })
        })
        .await;

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

    remote_debug_item.update(cx_b, |this, cx| {
        this.pause_thread(cx);
    });

    cx_b.run_until_parked();
    cx_a.run_until_parked();

    client
        .on_request::<dap::requests::StepOut, _>(move |_, _| Ok(()))
        .await;

    remote_debug_item.update(cx_b, |this, cx| {
        this.step_out(cx);
    });

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

    cx_b.run_until_parked();
    cx_a.run_until_parked();

    client
        .on_request::<dap::requests::Next, _>(move |_, _| Ok(()))
        .await;

    remote_debug_item.update(cx_b, |this, cx| {
        this.step_over(cx);
    });

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

    cx_b.run_until_parked();
    cx_a.run_until_parked();

    client
        .on_request::<dap::requests::StepIn, _>(move |_, _| Ok(()))
        .await;

    remote_debug_item.update(cx_b, |this, cx| {
        this.step_in(cx);
    });

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

    cx_b.run_until_parked();
    cx_a.run_until_parked();

    client
        .on_request::<dap::requests::StepBack, _>(move |_, _| Ok(()))
        .await;

    remote_debug_item.update(cx_b, |this, cx| {
        this.step_back(cx);
    });

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

    cx_b.run_until_parked();
    cx_a.run_until_parked();

    remote_debug_item.update(cx_b, |this, cx| {
        this.stop_thread(cx);
    });

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    // assert we don't have a debug panel item anymore because the client shutdown
    workspace_b.update(cx_b, |workspace, cx| {
        let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();

        debug_panel.update(cx, |this, cx| {
            assert!(this.active_debug_panel_item(cx).is_none());
            assert_eq!(0, this.pane().unwrap().read(cx).items_len());
        });
    });
}

#[gpui::test]
async fn test_variable_list(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let executor = cx_a.executor();
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;

    init_test(cx_a);
    init_test(cx_b);
    init_test(cx_c);

    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b), (&client_c, cx_c)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);
    let active_call_c = cx_c.read(ActiveCall::global);

    let (project_a, _worktree_id) = client_a.build_local_project("/a", cx_a).await;
    active_call_a
        .update(cx_a, |call, cx| call.set_location(Some(&project_a), cx))
        .await
        .unwrap();

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    active_call_b
        .update(cx_b, |call, cx| call.set_location(Some(&project_b), cx))
        .await
        .unwrap();

    let project_c = client_c.join_remote_project(project_id, cx_c).await;
    active_call_c
        .update(cx_c, |call, cx| call.set_location(Some(&project_c), cx))
        .await
        .unwrap();

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);
    let (workspace_c, cx_c) = client_c.build_workspace(&project_c, cx_c);

    add_debugger_panel(&workspace_a, cx_a).await;
    add_debugger_panel(&workspace_b, cx_b).await;
    add_debugger_panel(&workspace_c, cx_c).await;

    let task = project_a.update(cx_a, |project, cx| {
        project.dap_store().update(cx, |store, cx| {
            store.start_debug_session(
                dap::DebugAdapterConfig {
                    label: "test config".into(),
                    kind: dap::DebugAdapterKind::Fake,
                    request: dap::DebugRequestType::Launch,
                    program: None,
                    cwd: None,
                    initialize_args: None,
                },
                cx,
            )
        })
    });

    let (session, client) = task.await.unwrap();

    client
        .on_request::<Initialize, _>(move |_, _| {
            Ok(dap::Capabilities {
                supports_step_back: Some(true),
                ..Default::default()
            })
        })
        .await;

    client.on_request::<Launch, _>(move |_, _| Ok(())).await;

    let stack_frames = vec![dap::StackFrame {
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
        line: 1,
        column: 1,
        end_line: None,
        end_column: None,
        can_restart: None,
        instruction_pointer_reference: None,
        module_id: None,
        presentation_hint: None,
    }];

    client
        .on_request::<StackTrace, _>({
            let stack_frames = std::sync::Arc::new(stack_frames.clone());
            move |_, args| {
                assert_eq!(1, args.thread_id);

                Ok(dap::StackTraceResponse {
                    stack_frames: (*stack_frames).clone(),
                    total_frames: None,
                })
            }
        })
        .await;

    let scopes = vec![Scope {
        name: "Scope 1".into(),
        presentation_hint: None,
        variables_reference: 2,
        named_variables: None,
        indexed_variables: None,
        expensive: false,
        source: None,
        line: None,
        column: None,
        end_line: None,
        end_column: None,
    }];

    client
        .on_request::<Scopes, _>({
            let scopes = Arc::new(scopes.clone());
            move |_, args| {
                assert_eq!(1, args.frame_id);

                Ok(dap::ScopesResponse {
                    scopes: (*scopes).clone(),
                })
            }
        })
        .await;

    let variables = vec![Variable {
        name: "variable1".into(),
        value: "{nested1: \"Nested 1\", nested2: \"Nested 2\"}".into(),
        type_: None,
        presentation_hint: None,
        evaluate_name: None,
        variables_reference: 3,
        named_variables: None,
        indexed_variables: None,
        memory_reference: None,
    }];

    client
        .on_request::<Variables, _>({
            let variables = Arc::new(variables.clone());
            move |_, args| {
                assert_eq!(2, args.variables_reference);

                Ok(dap::VariablesResponse {
                    variables: (*variables).clone(),
                })
            }
        })
        .await;

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

    cx_a.run_until_parked();
    cx_b.run_until_parked();
    cx_c.run_until_parked();

    let local_debug_item = workspace_a.update(cx_a, |workspace, cx| {
        let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
        let active_debug_panel_item = debug_panel
            .update(cx, |this, cx| this.active_debug_panel_item(cx))
            .unwrap();

        assert_eq!(
            1,
            debug_panel.update(cx, |this, cx| this.pane().unwrap().read(cx).items_len())
        );
        assert_eq!(client.id(), active_debug_panel_item.read(cx).client_id());
        assert_eq!(1, active_debug_panel_item.read(cx).thread_id());
        active_debug_panel_item
    });

    let remote_debug_item = workspace_b.update(cx_b, |workspace, cx| {
        let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
        let active_debug_panel_item = debug_panel
            .update(cx, |this, cx| this.active_debug_panel_item(cx))
            .unwrap();

        assert_eq!(
            1,
            debug_panel.update(cx, |this, cx| this.pane().unwrap().read(cx).items_len())
        );
        assert_eq!(client.id(), active_debug_panel_item.read(cx).client_id());
        assert_eq!(1, active_debug_panel_item.read(cx).thread_id());
        active_debug_panel_item
    });

    let first_visual_entries = vec!["v Scope 1", "    > variable1"];

    local_debug_item
        .update(cx_a, |this, _| this.variable_list().clone())
        .update(cx_a, |variable_list, cx| {
            assert_eq!(1, variable_list.scopes().len());
            assert_eq!(scopes, variable_list.scopes().get(&1).unwrap().clone());
            assert_eq!(
                vec![VariableContainer {
                    container_reference: scopes[0].variables_reference,
                    variable: variables[0].clone(),
                    depth: 1,
                },],
                variable_list.variables(cx)
            );

            variable_list.assert_visual_entries(first_visual_entries.clone(), cx);
        });

    remote_debug_item
        .update(cx_b, |this, _| this.variable_list().clone())
        .update(cx_b, |variable_list, cx| {
            assert_eq!(1, variable_list.scopes().len());
            assert_eq!(scopes, variable_list.scopes().get(&1).unwrap().clone());
            assert_eq!(
                vec![VariableContainer {
                    container_reference: scopes[0].variables_reference,
                    variable: variables[0].clone(),
                    depth: 1,
                },],
                variable_list.variables(cx)
            );

            variable_list.assert_visual_entries(first_visual_entries.clone(), cx);

            variable_list.toggle_variable_in_test(
                scopes[0].variables_reference,
                &variables[0],
                1,
                cx,
            );
        });

    let variables_2 = vec![Variable {
        name: "variable 2".into(),
        value: "hello world".into(),
        type_: None,
        presentation_hint: None,
        evaluate_name: None,
        variables_reference: 4,
        named_variables: None,
        indexed_variables: None,
        memory_reference: None,
    }];

    client
        .on_request::<Variables, _>({
            let variables = Arc::new(variables_2.clone());
            move |_, args| {
                assert_eq!(3, args.variables_reference);

                Ok(dap::VariablesResponse {
                    variables: (*variables).clone(),
                })
            }
        })
        .await;

    cx_a.run_until_parked();
    cx_b.run_until_parked();
    cx_c.run_until_parked();

    remote_debug_item
        .update(cx_b, |this, _| this.variable_list().clone())
        .update(cx_b, |variable_list, cx| {
            assert_eq!(1, variable_list.scopes().len());
            assert_eq!(2, variable_list.variables(cx).len());
            assert_eq!(scopes, variable_list.scopes().get(&1).unwrap().clone());
            assert_eq!(
                vec![
                    VariableContainer {
                        container_reference: scopes[0].variables_reference,
                        variable: variables[0].clone(),
                        depth: 1,
                    },
                    VariableContainer {
                        container_reference: variables[0].variables_reference,
                        variable: variables_2[0].clone(),
                        depth: 2,
                    },
                ],
                variable_list.variables(cx)
            );

            variable_list.assert_visual_entries(
                vec!["v Scope 1", "    v variable1", "        > variable 2"],
                cx,
            );
        });

    local_debug_item
        .update(cx_a, |this, _| this.variable_list().clone())
        .update(cx_a, |variable_list, cx| {
            assert_eq!(1, variable_list.scopes().len());
            assert_eq!(2, variable_list.variables(cx).len());
            assert_eq!(scopes, variable_list.scopes().get(&1).unwrap().clone());
            assert_eq!(
                vec![
                    VariableContainer {
                        container_reference: scopes[0].variables_reference,
                        variable: variables[0].clone(),
                        depth: 1,
                    },
                    VariableContainer {
                        container_reference: variables[0].variables_reference,
                        variable: variables_2[0].clone(),
                        depth: 2,
                    },
                ],
                variable_list.variables(cx)
            );

            variable_list.assert_visual_entries(first_visual_entries.clone(), cx);
        });

    client.on_request::<Disconnect, _>(move |_, _| Ok(())).await;

    let shutdown_client = project_a.update(cx_a, |project, cx| {
        project.dap_store().update(cx, |dap_store, cx| {
            dap_store.shutdown_session(&session.read(cx).id(), cx)
        })
    });

    shutdown_client.await.unwrap();

    cx_b.run_until_parked();
    cx_c.run_until_parked();
}
