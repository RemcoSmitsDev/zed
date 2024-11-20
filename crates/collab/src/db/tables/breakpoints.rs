use crate::db::ProjectId;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "breakpoints")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub project_id: ProjectId,
    #[sea_orm(primary_key)]
    pub id: i64,
    pub worktree_id: i64,
    pub path: String,
    pub kind: i64,
    pub log_message: Option<String>,
    pub position: u64,
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
