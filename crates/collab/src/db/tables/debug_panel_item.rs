// use crate::db::ProjectId;
// use sea_orm::entity::prelude::*;

// #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
// #[sea_orm(table_name = "debug_panel_item")]
// pub struct Model {
//     #[sea_orm(primary_key)]
//     pub id: i32,
//     #[sea_orm(primary_key)]
//     pub project_id: ProjectId,
//     #[sea_orm(column_type = "Binary")]
//     pub panel_item: Vec<u8>,
// }
