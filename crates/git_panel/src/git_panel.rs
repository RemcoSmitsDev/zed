use anyhow::{Context, Result};
use git::repository::Branch;
use gpui::{
    actions, list, Action, AnyElement, AppContext, AsyncWindowContext, EventEmitter, Flatten,
    FocusHandle, FocusableView, ListState, Render, View, ViewContext, WeakView,
};
use ui::prelude::*;
use workspace::{
    dock::{DockPosition, Panel, PanelEvent},
    pane::Event,
    Workspace,
};

pub struct GitPanel {
    branches: Option<Vec<Branch>>,
    branch_list: ListState,
    focus_handle: FocusHandle,
    width: Option<Pixels>,
    workspace: WeakView<Workspace>,
}

actions!(git_panel, [ToggleFocus]);

pub fn init(cx: &mut AppContext) {
    cx.observe_new_views(|workspace: &mut Workspace, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, cx| {
            workspace.toggle_panel_focus::<GitPanel>(cx);
        });
    })
    .detach();
}

impl GitPanel {
    fn branches(workspace: &mut Workspace, cx: &AppContext) -> Result<Vec<Branch>> {
        workspace
            .project()
            .read(cx)
            .get_first_worktree_root_repo(cx)
            .context("Repository was not indexed yet")?
            .branches()
    }

    // pub fn new(workspace: &mut Workspace, cx: &mut ViewContext<Workspace>) -> Result<View<Self>> {
    //     Ok(cx.new_view(|cx: &mut ViewContext<Self>| {
    //         let focus_handle = cx.focus_handle();
    //         cx.on_focus(&focus_handle, Self::focus_in).detach();

    //         let view = cx.view().downgrade();

    //         let branch_list = ListState::new(
    //             0,
    //             gpui::ListAlignment::Top,
    //             px(1000.),
    //             move |ix, cx| {
    //                 view.upgrade()
    //                     .and_then(|view| view.update(cx, |this, cx| this.render_branch(ix, cx)))
    //                     .unwrap_or_else(|| div().into_any())
    //             },
    //         );

    //         Self {
    //             focus_handle,
    //             width: None,
    //             branch_list,
    //             workspace,
    //         }
    //     }))
    // }

    pub async fn load(
        workspace: WeakView<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<View<Self>> {
        // cx.background_executor().timer(Duration::from_secs(5)).await;

        let worspace_handle = workspace.clone();
        workspace.update(&mut cx, |workspace, cx| {
            cx.new_view::<Self>(|cx| {
                let focus_handle = cx.focus_handle();
                cx.on_focus(&focus_handle, Self::focus_in).detach();

                let view = cx.view().downgrade();

                let branch_list =
                    ListState::new(10, gpui::ListAlignment::Top, px(1000.), move |ix, cx| {
                        view.upgrade()
                            .and_then(|view| view.update(cx, |this, cx| this.render_branch(ix, cx)))
                            .unwrap_or_else(|| div().into_any())
                    });

                Self {
                    focus_handle,
                    width: None,
                    branch_list,
                    workspace: worspace_handle,
                    branches: Default::default(),
                }
            })
        })
    }

    fn focus_in(&mut self, cx: &mut ViewContext<Self>) {
        if !self.focus_handle.contains_focused(cx) {
            cx.emit(Event::Focus);
        }
    }

    fn render_branch(&self, ix: usize, cx: &mut ViewContext<Self>) -> Option<AnyElement> {
        let branch = self
            .branches
            .as_ref()
            .and_then(|branches| branches.get(ix))?;
        let branch_name = String::from(branch.name.clone());

        let element_id = format!("branch-{}", branch_name.clone());

        Some(
            h_flex()
                .id(ElementId::Name(element_id.into()))
                .w_full()
                .justify_between()
                .text_ui_xs(cx)
                .p_0p5()
                .bg(if branch.is_head {
                    cx.theme().colors().background
                } else {
                    cx.theme().colors().element_background
                })
                .cursor_pointer()
                .hover(|s| {
                    s.text_color(cx.theme().colors().text)
                        .bg(cx.theme().colors().background)
                })
                .text_color(if branch.is_head {
                    cx.theme().colors().text
                } else {
                    cx.theme().colors().text_muted
                })
                .child(
                    h_flex()
                        .gap_0p5()
                        .child(
                            Icon::new(IconName::FileGit)
                                .size(ui::IconSize::XSmall)
                                .into_element(),
                        )
                        .child(branch_name),
                )
                .child("7")
                .into_any_element(),
        )
    }
}

impl EventEmitter<Event> for GitPanel {}

impl EventEmitter<PanelEvent> for GitPanel {}

impl FocusableView for GitPanel {
    fn focus_handle(&self, _cx: &AppContext) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for GitPanel {
    fn persistent_name() -> &'static str {
        "Git Panel"
    }

    fn position(&self, _: &WindowContext) -> DockPosition {
        DockPosition::Left
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        position == DockPosition::Left
    }

    fn set_position(&mut self, _: DockPosition, _: &mut ViewContext<Self>) {}

    fn size(&self, _: &WindowContext) -> Pixels {
        self.width.unwrap_or(Pixels::from(200.0))
    }

    fn set_size(&mut self, size: Option<Pixels>, cx: &mut ViewContext<Self>) {
        self.width = size;
        cx.notify();
    }

    fn icon(&self, _: &WindowContext) -> Option<IconName> {
        Some(IconName::FileGit)
    }

    fn icon_tooltip(&self, _: &WindowContext) -> Option<&'static str> {
        Some("Git Panel")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn icon_label(&self, _: &WindowContext) -> Option<String> {
        None
    }

    fn is_zoomed(&self, _: &WindowContext) -> bool {
        false
    }

    fn starts_open(&self, _: &WindowContext) -> bool {
        false
    }

    fn set_zoomed(&mut self, _zoomed: bool, _cx: &mut ViewContext<Self>) {}

    fn set_active(&mut self, _active: bool, _cx: &mut ViewContext<Self>) {}
}

impl Render for GitPanel {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        if self.branches.is_none() {
            if let Some(Ok(branches)) = self
                .workspace
                .update(cx, |w, cx| Self::branches(w, cx))
                .ok()
            {
                self.branches = Some(branches);
            }
        }

        div()
            .p_2()
            .size_full()
            .child(
                v_flex()
                    .gap_1()
                    .size_full()
                    .child(Label::new("Branches").size(ui::LabelSize::XSmall))
                    .child(list(self.branch_list.clone()).size_full()),
            )
            .into_element()
    }
}
