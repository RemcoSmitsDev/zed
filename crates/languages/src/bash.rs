use project::ContextProviderWithTasks;
use task::{TaskTemplate, TaskTemplates, TemplateType, VariableName};

pub(super) fn bash_task_context() -> ContextProviderWithTasks {
    ContextProviderWithTasks::new(TaskTemplates(vec![
        TemplateType::Task(TaskTemplate {
            label: "execute selection".to_owned(),
            command: VariableName::SelectedText.template_value(),
            ..TaskTemplate::default()
        }),
        TemplateType::Task(TaskTemplate {
            label: format!("run '{}'", VariableName::File.template_value()),
            command: VariableName::File.template_value(),
            ..TaskTemplate::default()
        }),
    ]))
}
