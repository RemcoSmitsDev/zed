use crate::db::ProjectId;
use anyhow::Result;
use prost::Message;
use rpc::{proto, proto::SetDebuggerPanelItem};
use sea_orm::entity::prelude::*;
use util::ResultExt;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "debug_panel_items")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(primary_key)]
    pub project_id: ProjectId,
    #[sea_orm(primary_key)]
    pub thread_id: i64,
    pub session_id: i64,
    // Below are fields for a debug panel item
    pub active_thread_item: i32,
    pub seassion_name: String,
    pub console: Vec<u8>,
    pub module_list: Vec<u8>,
    pub thread_state: Vec<u8>,
    pub variable_list: Vec<u8>,
    pub stack_frame_list: Vec<u8>,
    pub loaded_source_list: Vec<u8>,
}

impl Model {
    pub fn set_panel_item(&mut self, item: &SetDebuggerPanelItem) -> Result<()> {
        let mut buf = Vec::new();

        self.active_thread_item = item.active_thread_item;

        if let Some(console) = item.console.as_ref() {
            if let Some(()) = console.encode(&mut buf).log_err() {
                self.console.clone_from(&buf);
            }
        }

        buf.clear();
        if let Some(module_list) = item.module_list.as_ref() {
            if let Some(()) = module_list.encode(&mut buf).log_err() {
                self.module_list.clone_from(&buf);
            }
        }

        buf.clear();
        if let Some(thread_state) = item.thread_state.as_ref() {
            if let Some(()) = thread_state.encode(&mut buf).log_err() {
                self.thread_state.clone_from(&buf);
            }
        }

        buf.clear();
        if let Some(variable_list) = item.variable_list.as_ref() {
            if let Some(()) = variable_list.encode(&mut buf).log_err() {
                self.variable_list.clone_from(&buf);
            }
        }

        buf.clear();
        if let Some(stack_frame_list) = item.stack_frame_list.as_ref() {
            if let Some(()) = stack_frame_list.encode(&mut buf).log_err() {
                self.stack_frame_list.clone_from(&buf);
            }
        }

        buf.clear();
        if let Some(loaded_source_list) = item.loaded_source_list.as_ref() {
            if let Some(()) = loaded_source_list.encode(&mut buf).log_err() {
                self.loaded_source_list.clone_from(&buf);
            }
        }

        Ok(())
    }

    pub fn panel_item(&self) -> SetDebuggerPanelItem {
        SetDebuggerPanelItem {
            project_id: self.project_id.to_proto(),
            session_id: self.session_id as u64,
            client_id: self.id as u64,
            thread_id: self.thread_id as u64,
            session_name: self.seassion_name.clone(),
            active_thread_item: self.active_thread_item,
            console: proto::DebuggerConsole::decode(&self.console[..]).log_err(),
            module_list: proto::DebuggerModuleList::decode(&self.module_list[..]).log_err(),
            thread_state: proto::DebuggerThreadState::decode(&self.thread_state[..]).log_err(),
            variable_list: proto::DebuggerVariableList::decode(&self.variable_list[..]).log_err(),
            stack_frame_list: proto::DebuggerStackFrameList::decode(&self.stack_frame_list[..])
                .log_err(),
            loaded_source_list: proto::DebuggerLoadedSourceList::decode(
                &self.loaded_source_list[..],
            )
            .log_err(),
        }
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::project::Entity",
        from = "Column::ProjectId",
        to = "super::project::Column::Id"
    )]
    Project,
    #[sea_orm(
        belongs_to = "super::debug_clients::Entity",
        from = "(Column::Id, Column::ProjectId, Column::SessionId)",
        to = "(super::debug_clients::Column::Id, super::debug_clients::Column::ProjectId, super::debug_clients::Column::SessionId)"
    )]
    DebugClient,
}

impl Related<super::project::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Project.def()
    }
}

impl Related<super::debug_clients::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::DebugClient.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
