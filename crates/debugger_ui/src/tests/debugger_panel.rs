use std::time::Duration;

use crate::*;
use gpui::{BackgroundExecutor, Model, TestAppContext, VisualTestContext, WindowHandle};
use project::{FakeFs, Project};
use serde_json::json;
use settings::SettingsStore;
use unindent::Unindent as _;
use workspace::Workspace;

pub fn init_test(cx: &mut gpui::TestAppContext) {
    if std::env::var("RUST_LOG").is_ok() {
        env_logger::try_init().ok();
    }

    cx.update(|cx| {
        let settings = SettingsStore::test(cx);
        cx.set_global(settings);
        theme::init(theme::LoadThemes::JustBase, cx);
        command_palette_hooks::init(cx);
        language::init(cx);
        workspace::init_settings(cx);
        Project::init_settings(cx);
        crate::init(cx);
        editor::init(cx);
    });
}

async fn add_debugger_panel(
    project: &Model<Project>,
    cx: &mut TestAppContext,
) -> WindowHandle<Workspace> {
    let window = cx.add_window(|cx| Workspace::test_new(project.clone(), cx));

    let debugger_panel = window
        .update(cx, |_, cx| cx.spawn(DebugPanel::load))
        .unwrap()
        .await
        .expect("Failed to load debug panel");

    window
        .update(cx, |workspace, cx| {
            workspace.add_panel(debugger_panel, cx);
        })
        .unwrap();
    window
}

#[gpui::test]
async fn test_show_debug_panel(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    let file_contents = r#"
        // print goodbye
        fn main() {
            println!("goodbye world");
        }
    "#
    .unindent();

    fs.insert_tree(
        "/dir",
        json!({
           "src": {
               "main.rs": file_contents,
           }
        }),
    )
    .await;

    let project = Project::test(fs, ["/dir".as_ref()], cx).await;
    let workspace = add_debugger_panel(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(*workspace, cx);

    let task = project.update(cx, |project, cx| {
        project.dap_store().update(cx, |store, cx| {
            store.start_test_client(
                task::DebugAdapterConfig {
                    kind: task::DebugAdapterKind::Fake,
                    request: task::DebugRequestType::Launch,
                    program: None,
                    cwd: None,
                    initialize_args: None,
                },
                cx,
            )
        })
    });

    let client = task.await.unwrap();

    // assert we don't have a debug panel item yet
    workspace
        .update(cx, |workspace, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();

            assert!(debug_panel.update(cx, |this, cx| this.active_debug_panel_item(cx).is_none()));
        })
        .unwrap();

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

    dbg!("here");

    // // assert we added a debug panel item
    workspace
        .update(cx, |workspace, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let debug_panel_item = debug_panel
                .update(cx, |this, cx| this.active_debug_panel_item(cx))
                .unwrap();

            assert_eq!(client.id(), debug_panel_item.read(cx).client_id());
            assert_eq!(1, debug_panel_item.read(cx).thread_id());
        })
        .unwrap();

    dbg!("shutdown end");

    client.shutdown().await.unwrap();

    // Ensure that the project lasts until after the last await
    drop(project);
}
