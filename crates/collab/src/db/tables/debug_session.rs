use crate::db::ProjectId;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "debug_session")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(primary_key)]
    pub project_id: ProjectId,
    #[sea_orm(column_type = "Integer")]
    pub capabilities: u32,
}

impl Model {
    pub fn capabilities(&self) -> DebugClientCapabilities {
        DebugClientCapabilities::from_u32(self.capabilities)
    }

    pub fn set_capabilities(&mut self, capabilities: DebugClientCapabilities) {
        self.capabilities = capabilities.to_u32()
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
}

impl Related<super::project::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Project.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebugClientCapabilities {
    pub supports_loaded_sources_request: bool,
    pub supports_modules_request: bool,
    pub supports_restart_request: bool,
    pub supports_set_expression: bool,
    pub supports_single_thread_execution_requests: bool,
    pub supports_step_back: bool,
    pub supports_stepping_granularity: bool,
    pub supports_terminate_threads_request: bool,
}

const SUPPORTS_LOADED_SOURCES_REQUEST_BIT: u32 = 0;
const SUPPORTS_MODULES_REQUEST_BIT: u32 = 1;
const SUPPORTS_RESTART_REQUEST_BIT: u32 = 2;
const SUPPORTS_SET_EXPRESSION_BIT: u32 = 3;
const SUPPORTS_SINGLE_THREAD_EXECUTION_REQUESTS_BIT: u32 = 4;
const SUPPORTS_STEP_BACK_BIT: u32 = 5;
const SUPPORTS_STEPPING_GRANULARITY_BIT: u32 = 6;
const SUPPORTS_TERMINATE_THREADS_REQUEST_BIT: u32 = 7;

impl DebugClientCapabilities {
    pub fn to_u32(&self) -> u32 {
        let mut result = 0;
        result |=
            (self.supports_loaded_sources_request as u32) << SUPPORTS_LOADED_SOURCES_REQUEST_BIT;
        result |= (self.supports_modules_request as u32) << SUPPORTS_MODULES_REQUEST_BIT;
        result |= (self.supports_restart_request as u32) << SUPPORTS_RESTART_REQUEST_BIT;
        result |= (self.supports_set_expression as u32) << SUPPORTS_SET_EXPRESSION_BIT;
        result |= (self.supports_single_thread_execution_requests as u32)
            << SUPPORTS_SINGLE_THREAD_EXECUTION_REQUESTS_BIT;
        result |= (self.supports_step_back as u32) << SUPPORTS_STEP_BACK_BIT;
        result |= (self.supports_stepping_granularity as u32) << SUPPORTS_STEPPING_GRANULARITY_BIT;
        result |= (self.supports_terminate_threads_request as u32)
            << SUPPORTS_TERMINATE_THREADS_REQUEST_BIT;
        result
    }

    pub fn from_u32(value: u32) -> Self {
        Self {
            supports_loaded_sources_request: (value & (1 << SUPPORTS_LOADED_SOURCES_REQUEST_BIT))
                != 0,
            supports_modules_request: (value & (1 << SUPPORTS_MODULES_REQUEST_BIT)) != 0,
            supports_restart_request: (value & (1 << SUPPORTS_RESTART_REQUEST_BIT)) != 0,
            supports_set_expression: (value & (1 << SUPPORTS_SET_EXPRESSION_BIT)) != 0,
            supports_single_thread_execution_requests: (value
                & (1 << SUPPORTS_SINGLE_THREAD_EXECUTION_REQUESTS_BIT))
                != 0,
            supports_step_back: (value & (1 << SUPPORTS_STEP_BACK_BIT)) != 0,
            supports_stepping_granularity: (value & (1 << SUPPORTS_STEPPING_GRANULARITY_BIT)) != 0,
            supports_terminate_threads_request: (value
                & (1 << SUPPORTS_TERMINATE_THREADS_REQUEST_BIT))
                != 0,
        }
    }
}
