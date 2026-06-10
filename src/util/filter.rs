use crate::clickup::models::Task;

pub fn should_include_task(task: &Task, user_id: i64, show_all: bool, mine_only: bool) -> bool {
    if mine_only && !task.assignees.iter().any(|u| u.id == user_id) {
        return false;
    }

    let status = task.status.status.to_lowercase();
    if show_all {
        status != "completed" && status != "closed"
    } else {
        matches!(
            status.as_str(),
            "in progress" | "in review" | "blocked" | "scoping"
        )
    }
}
