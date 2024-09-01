use crate::debugger_panel_item::DebugPanelItem;
use dap::{client::ThreadState, requests::SetVariable, Scope, SetVariableArguments, Variable};

use editor::{actions::SelectAll, Editor};
use gpui::{
    anchored, deferred, list, AnyElement, ClipboardItem, DismissEvent, FocusHandle, FocusableView,
    ListState, Model, MouseDownEvent, Point, Subscription, View,
};
use menu::Confirm;
use ui::{prelude::*, ContextMenu, ListItem};

use std::{collections::HashMap, sync::Arc};

#[derive(Debug, Clone)]
struct SetVariableState {
    parent_variables_reference: u64,
    name: String,
    value: String,
}

#[derive(Debug, Clone)]
pub enum ThreadEntry {
    Scope(Scope),
    SetVariableEditor {
        depth: usize,
        state: SetVariableState,
    },
    Variable {
        depth: usize,
        scope: Scope,
        variable: Arc<Variable>,
        has_children: bool,
        parent_variables_reference: u64,
    },
}

pub struct VariableList {
    list: ListState,
    focus_handle: FocusHandle,
    open_entries: Vec<SharedString>,
    set_variable_editor: View<Editor>,
    debug_panel_item: Model<DebugPanelItem>,
    set_variable_state: Option<SetVariableState>,
    stack_frame_entries: HashMap<u64, Vec<ThreadEntry>>,
    open_context_menu: Option<(View<ContextMenu>, Point<Pixels>, Subscription)>,
}

impl VariableList {
    pub fn new(debug_panel_item: Model<DebugPanelItem>, cx: &mut ViewContext<Self>) -> Self {
        let weakview = cx.view().downgrade();
        let focus_handle = cx.focus_handle();

        let list = ListState::new(0, gpui::ListAlignment::Top, px(1000.), move |ix, cx| {
            weakview
                .upgrade()
                .map(|view| view.update(cx, |this, cx| this.render_entry(ix, cx)))
                .unwrap_or(div().into_any())
        });

        let set_variable_editor = cx.new_view(|cx| Editor::single_line(cx));

        cx.subscribe(&set_variable_editor, |this: &mut Self, _, event, cx| {
            match event {
                editor::EditorEvent::Blurred => {
                    this.set_variable_state.take();

                    let thread_state = this
                        .debug_panel_item
                        .read_with(cx, |panel, _| panel.current_thread_state());

                    this.build_entries(thread_state, false);
                    cx.notify();
                }
                _ => {}
            };
        })
        .detach();

        Self {
            list,
            focus_handle,
            debug_panel_item,
            set_variable_editor,
            open_context_menu: None,
            set_variable_state: None,
            open_entries: Default::default(),
            stack_frame_entries: Default::default(),
        }
    }

    fn render_entry(&mut self, ix: usize, cx: &mut ViewContext<Self>) -> AnyElement {
        let debug_item = self.debug_panel_item.read(cx);
        let Some(entries) = self
            .stack_frame_entries
            .get(&debug_item.current_thread_state().current_stack_frame_id)
        else {
            return div().into_any_element();
        };

        match &entries[ix] {
            ThreadEntry::Scope(scope) => self.render_scope(scope, cx),
            ThreadEntry::SetVariableEditor { depth, state } => {
                self.render_set_variable_editor(*depth, state, cx)
            }
            ThreadEntry::Variable {
                depth,
                scope,
                variable,
                has_children,
                parent_variables_reference,
            } => self.render_variable(
                ix,
                *parent_variables_reference,
                variable,
                scope,
                *depth,
                *has_children,
                cx,
            ),
        }
    }

    pub fn toggle_entry_collapsed(&mut self, entry_id: &SharedString, cx: &mut ViewContext<Self>) {
        match self.open_entries.binary_search(&entry_id) {
            Ok(ix) => {
                self.open_entries.remove(ix);
            }
            Err(ix) => {
                self.open_entries.insert(ix, entry_id.clone());
            }
        };

        let thread_state = self
            .debug_panel_item
            .read_with(cx, |panel, _cx| panel.current_thread_state());

        self.build_entries(thread_state, false);
        cx.notify();
    }

    pub fn build_entries(&mut self, thread_state: ThreadState, open_first_scope: bool) {
        let stack_frame_id = thread_state.current_stack_frame_id;
        let Some(scopes_and_vars) = thread_state.variables.get(&stack_frame_id) else {
            return;
        };

        let mut entries: Vec<ThreadEntry> = Vec::default();
        for (scope, variables) in scopes_and_vars {
            if variables.is_empty() {
                continue;
            }

            if open_first_scope && entries.is_empty() {
                self.open_entries.push(scope_entry_id(scope));
            }

            entries.push(ThreadEntry::Scope(scope.clone()));

            if self
                .open_entries
                .binary_search(&scope_entry_id(scope))
                .is_err()
            {
                continue;
            }

            let mut depth_check: Option<usize> = None;

            for (depth, variable) in variables {
                if depth_check.is_some_and(|d| *depth > d) {
                    continue;
                }

                if depth_check.is_some_and(|d| d >= *depth) {
                    depth_check = None;
                }

                let has_children = variable.variables_reference > 0;

                if self
                    .open_entries
                    .binary_search(&variable_entry_id(&variable, &scope, *depth))
                    .is_err()
                {
                    if depth_check.is_none() || depth_check.is_some_and(|d| d > *depth) {
                        depth_check = Some(*depth);
                    }
                }

                // TODO: allow setting variable value for non top level variables
                // right now its pinned to the `scope.variables_reference`
                if let Some(state) = self.set_variable_state.as_ref() {
                    if state.parent_variables_reference == scope.variables_reference
                        && state.name == variable.name
                    {
                        entries.push(ThreadEntry::SetVariableEditor {
                            depth: *depth,
                            state: state.clone(),
                        });
                    }
                }

                entries.push(ThreadEntry::Variable {
                    has_children,
                    depth: *depth,
                    scope: scope.clone(),
                    variable: Arc::new(variable.clone()),
                    parent_variables_reference: scope.variables_reference,
                });
            }
        }

        let len = entries.len();
        self.stack_frame_entries.insert(stack_frame_id, entries);
        self.list.reset(len);
    }

    fn deploy_variable_context_menu(
        &mut self,
        parent_variables_reference: u64,
        variable: &Variable,
        position: Point<Pixels>,
        cx: &mut ViewContext<Self>,
    ) {
        let this = cx.view().clone();

        let client = self.debug_panel_item.read_with(cx, |p, _| p.client());
        let support_set_variable = client
            .capabilities()
            .supports_set_variable
            .unwrap_or_default();

        let context_menu = ContextMenu::build(cx, |menu, cx| {
            menu.entry(
                "Copy name",
                None,
                cx.handler_for(&this, {
                    let variable = variable.clone();
                    move |_, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(variable.name.clone()))
                    }
                }),
            )
            .entry(
                "Copy value",
                None,
                cx.handler_for(&this, {
                    let variable = variable.clone();
                    move |_, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(variable.value.clone()))
                    }
                }),
            )
            .when(support_set_variable, move |menu| {
                let variable = variable.clone();

                menu.entry(
                    "Set value",
                    None,
                    cx.handler_for(&this, move |this, cx| {
                        this.set_variable_state = Some(SetVariableState {
                            parent_variables_reference,
                            name: variable.name.clone(),
                            value: variable.value.clone(),
                        });

                        this.set_variable_editor.update(cx, |editor, cx| {
                            editor.set_text(variable.value.clone(), cx);
                            editor.select_all(&SelectAll, cx);
                            editor.focus(cx);
                        });

                        let thread_state = this
                            .debug_panel_item
                            .read_with(cx, |panel, _| panel.current_thread_state());
                        this.build_entries(thread_state, false);

                        cx.notify();
                    }),
                )
            })
        });

        cx.focus_view(&context_menu);
        let subscription =
            cx.subscribe(&context_menu, |this, _, _: &DismissEvent, cx| {
                if this.open_context_menu.as_ref().is_some_and(|context_menu| {
                    context_menu.0.focus_handle(cx).contains_focused(cx)
                }) {
                    cx.focus_self();
                }
                this.open_context_menu.take();
                cx.notify();
            });

        self.open_context_menu = Some((context_menu, position, subscription));
    }

    fn set_variable_value(&mut self, _: &Confirm, cx: &mut ViewContext<Self>) {
        let new_variable_value = self.set_variable_editor.update(cx, |editor, cx| {
            let new_variable_value = editor.text(cx);

            editor.set_text("", cx);

            new_variable_value
        });

        let Some(state) = self.set_variable_state.take() else {
            return;
        };

        if new_variable_value == state.value {
            return;
        }

        let client = self.debug_panel_item.read_with(cx, |p, _| p.client());
        let variables_reference = state.parent_variables_reference;
        let name = state.name;

        cx.spawn(|_, _| async move {
            client
                .request::<SetVariable>(SetVariableArguments {
                    variables_reference,
                    name,
                    value: new_variable_value,
                    format: None,
                })
                .await?;

            // TODO: refetch variables for reference

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);

        let thread_state = self
            .debug_panel_item
            .read_with(cx, |panel, _| panel.current_thread_state());

        self.build_entries(thread_state, false);
        cx.notify();
    }

    fn render_set_variable_editor(
        &self,
        depth: usize,
        state: &SetVariableState,
        cx: &mut ViewContext<Self>,
    ) -> AnyElement {
        div()
            .h_4()
            .size_full()
            .on_action(cx.listener(Self::set_variable_value))
            .child(
                ListItem::new(SharedString::from(state.name.clone()))
                    .indent_level(depth + 1)
                    .indent_step_size(px(20.))
                    .child(self.set_variable_editor.clone()),
            )
            .into_any_element()
    }

    fn render_variable(
        &self,
        ix: usize,
        parent_variables_reference: u64,
        variable: &Variable,
        scope: &Scope,
        depth: usize,
        has_children: bool,
        cx: &mut ViewContext<Self>,
    ) -> AnyElement {
        let variable_reference = variable.variables_reference;
        let variable_id = variable_entry_id(variable, scope, depth);

        let disclosed = has_children.then(|| self.open_entries.binary_search(&variable_id).is_ok());

        div()
            .id(variable_id.clone())
            .group("")
            .h_4()
            .size_full()
            .child(
                ListItem::new(variable_id.clone())
                    .indent_level(depth + 1)
                    .indent_step_size(px(20.))
                    .always_show_disclosure_icon(true)
                    .toggle(disclosed)
                    .on_toggle(cx.listener(move |this, _, cx| {
                        if !has_children {
                            return;
                        }

                        let debug_item = this.debug_panel_item.read(cx);

                        // if we already opened the variable/we already fetched it
                        // we can just toggle it because we already have the nested variable
                        if disclosed.unwrap_or(true)
                            || debug_item
                                .current_thread_state()
                                .vars
                                .contains_key(&variable_reference)
                        {
                            return this.toggle_entry_collapsed(&variable_id, cx);
                        }

                        let Some(entries) = this
                            .stack_frame_entries
                            .get(&debug_item.current_thread_state().current_stack_frame_id)
                        else {
                            return;
                        };

                        let Some(entry) = entries.get(ix) else {
                            return;
                        };

                        if let ThreadEntry::Variable { scope, depth, .. } = entry {
                            let variable_id = variable_id.clone();
                            let client = debug_item.client();
                            let scope = scope.clone();
                            let depth = *depth;

                            cx.spawn(|this, mut cx| async move {
                                let variables = client.variables(variable_reference).await?;

                                this.update(&mut cx, |this, cx| {
                                    let client = client.clone();
                                    let mut thread_states = client.thread_states();
                                    let Some(thread_state) = thread_states
                                        .get_mut(&this.debug_panel_item.read(cx).thread_id())
                                    else {
                                        return;
                                    };

                                    if let Some(state) = thread_state
                                        .variables
                                        .get_mut(&thread_state.current_stack_frame_id)
                                        .and_then(|s| s.get_mut(&scope))
                                    {
                                        let position = state.iter().position(|(d, v)| {
                                            variable_entry_id(v, &scope, *d) == variable_id
                                        });

                                        if let Some(position) = position {
                                            state.splice(
                                                position + 1..position + 1,
                                                variables
                                                    .clone()
                                                    .into_iter()
                                                    .map(|v| (depth + 1, v)),
                                            );
                                        }

                                        thread_state.vars.insert(variable_reference, variables);
                                    }

                                    drop(thread_states);
                                    this.toggle_entry_collapsed(&variable_id, cx);
                                })
                            })
                            .detach_and_log_err(cx);
                        }
                    }))
                    .on_secondary_mouse_down(cx.listener({
                        let variable = variable.clone();
                        move |this, event: &MouseDownEvent, cx| {
                            this.deploy_variable_context_menu(
                                parent_variables_reference,
                                &variable,
                                event.position,
                                cx,
                            )
                        }
                    }))
                    .child(
                        h_flex()
                            .gap_1()
                            .text_ui_sm(cx)
                            .child(variable.name.clone())
                            .child(
                                div()
                                    .text_ui_xs(cx)
                                    .text_color(cx.theme().colors().text_muted)
                                    .child(variable.value.clone()),
                            ),
                    ),
            )
            .into_any()
    }

    fn render_scope(&self, scope: &Scope, cx: &mut ViewContext<Self>) -> AnyElement {
        let element_id = scope.variables_reference;

        let scope_id = scope_entry_id(scope);
        let disclosed = self.open_entries.binary_search(&scope_id).is_ok();

        div()
            .id(element_id as usize)
            .group("")
            .flex()
            .w_full()
            .h_full()
            .child(
                ListItem::new(scope_id.clone())
                    .indent_level(1)
                    .indent_step_size(px(20.))
                    .always_show_disclosure_icon(true)
                    .toggle(disclosed)
                    .on_toggle(
                        cx.listener(move |this, _, cx| this.toggle_entry_collapsed(&scope_id, cx)),
                    )
                    .child(div().text_ui(cx).w_full().child(scope.name.clone())),
            )
            .into_any()
    }
}

impl FocusableView for VariableList {
    fn focus_handle(&self, _: &gpui::AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for VariableList {
    fn render(&mut self, _: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(list(self.list.clone()).gap_1_5().size_full())
            .children(self.open_context_menu.as_ref().map(|(menu, position, _)| {
                deferred(
                    anchored()
                        .position(*position)
                        .anchor(gpui::AnchorCorner::TopLeft)
                        .child(menu.clone()),
                )
                .with_priority(1)
            }))
    }
}

pub fn variable_entry_id(variable: &Variable, scope: &Scope, depth: usize) -> SharedString {
    SharedString::from(format!(
        "variable-{}-{}-{}",
        depth, scope.variables_reference, variable.name
    ))
}

fn scope_entry_id(scope: &Scope) -> SharedString {
    SharedString::from(format!("scope-{}", scope.variables_reference))
}