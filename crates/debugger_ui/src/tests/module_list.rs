use crate::{
    debugger_panel_item::ThreadItem,
    tests::{active_debug_panel_item, init_test, init_test_workspace},
};
use dap::{
    requests::{Disconnect, Initialize, Launch, Modules, StackTrace},
    StoppedEvent,
};
use gpui::{BackgroundExecutor, TestAppContext, VisualTestContext};
use project::{FakeFs, Project};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[gpui::test]
async fn test_module_list(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    let project = Project::test(fs, ["/project".as_ref()], cx).await;
    let workspace = init_test_workspace(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(*workspace, cx);

    let task = project.update(cx, |project, cx| {
        project.start_debug_session(
            task::DebugAdapterConfig {
                label: "test config".into(),
                kind: task::DebugAdapterKind::Fake,
                request: task::DebugRequestType::Launch,
                program: None,
                cwd: None,
                initialize_args: None,
            },
            cx,
        )
    });

    let (session, client) = task.await.unwrap();

    client
        .on_request::<Initialize, _>(move |_, _| {
            Ok(dap::Capabilities {
                supports_modules_request: Some(true),
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

    let called_modules = Arc::new(AtomicBool::new(false));
    let modules = vec![
        dap::Module {
            id: dap::ModuleId::Number(1),
            name: "First Module".into(),
            address_range: None,
            date_time_stamp: None,
            path: None,
            symbol_file_path: None,
            symbol_status: None,
            version: None,
            is_optimized: None,
            is_user_code: None,
        },
        dap::Module {
            id: dap::ModuleId::Number(2),
            name: "Second Module".into(),
            address_range: None,
            date_time_stamp: None,
            path: None,
            symbol_file_path: None,
            symbol_status: None,
            version: None,
            is_optimized: None,
            is_user_code: None,
        },
    ];

    client
        .on_request::<Modules, _>({
            let called_modules = called_modules.clone();
            let modules = modules.clone();
            move |_, _| unsafe {
                static mut REQUEST_COUNT: i32 = 1;
                assert_eq!(
                    1, REQUEST_COUNT,
                    "This request should only be called once from the host"
                );
                REQUEST_COUNT += 1;
                called_modules.store(true, Ordering::SeqCst);

                Ok(dap::ModulesResponse {
                    modules: modules.clone(),
                    total_modules: Some(2u64),
                })
            }
        })
        .await;

    client
        .fake_event(dap::messages::Events::Stopped(StoppedEvent {
            reason: dap::StoppedEventReason::Pause,
            description: None,
            thread_id: Some(1),
            preserve_focus_hint: None,
            text: None,
            all_threads_stopped: None,
            hit_breakpoint_ids: None,
        }))
        .await;

    client.on_request::<Disconnect, _>(move |_, _| Ok(())).await;

    cx.run_until_parked();

    assert!(
        !called_modules.load(std::sync::atomic::Ordering::SeqCst),
        "Request Modules shouldn't be called before it's needed"
    );

    active_debug_panel_item(workspace, cx).update(cx, |item, cx| {
        item.set_thread_item(ThreadItem::Modules, cx);
    });

    cx.run_until_parked();

    assert!(
        called_modules.load(std::sync::atomic::Ordering::SeqCst),
        "Request Modules should be called because a user clicked on the module list"
    );

    active_debug_panel_item(workspace, cx).update(cx, |item, cx| {
        item.set_thread_item(ThreadItem::Modules, cx);

        let actual_modules = item.module_list().update(cx, |list, cx| list.modules(cx));
        assert_eq!(modules, actual_modules);
    });

    let shutdown_session = project.update(cx, |project, cx| {
        project.dap_store().update(cx, |dap_store, cx| {
            dap_store.shutdown_session(&session.read(cx).id(), cx)
        })
    });

    shutdown_session.await.unwrap();
}
