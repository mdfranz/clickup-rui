use crate::clickup::api::ClickUpApi;
use crate::clickup::models::{Activity, Task, User};
use crate::config::FolderConfig;

pub enum ActivityScope {
    Team,
    User(User),
}

pub async fn collect_activities<A: ClickUpApi>(
    api: &A,
    folders: &[FolderConfig],
    date_from: i64,
    scope: ActivityScope,
) -> Vec<Activity> {
    let mut activities = Vec::new();

    for folder in folders {
        let Ok(lists) = api.get_lists(&folder.id).await else {
            continue;
        };

        for list in lists {
            let Ok(tasks) = api.get_tasks_incremental(&list.id, date_from).await else {
                continue;
            };

            for task in tasks {
                collect_task_activities(api, &task, date_from, &scope, &mut activities).await;
            }
        }
    }

    activities.sort_by(|a, b| {
        let a_time = a.date.parse::<i64>().ok().unwrap_or(0);
        let b_time = b.date.parse::<i64>().ok().unwrap_or(0);
        b_time.cmp(&a_time)
    });
    activities
}

async fn collect_task_activities<A: ClickUpApi>(
    api: &A,
    task: &Task,
    date_from: i64,
    scope: &ActivityScope,
    activities: &mut Vec<Activity>,
) {
    let created_ms = timestamp(&task.date_created);
    let updated_ms = timestamp(&task.date_updated);

    match scope {
        ActivityScope::Team if created_ms >= date_from => {
            activities.push(task_activity(
                task,
                "created",
                "created task",
                created_ms,
                task.creator.clone(),
            ));
        }
        ActivityScope::User(user) if created_ms >= date_from && task.creator.id == user.id => {
            activities.push(task_activity(
                task,
                "created",
                "created task",
                created_ms,
                user.clone(),
            ));
        }
        _ => {}
    }

    let status_user = match scope {
        ActivityScope::Team => Some(task.creator.clone()),
        ActivityScope::User(user)
            if task.assignees.iter().any(|assignee| assignee.id == user.id) =>
        {
            Some(user.clone())
        }
        ActivityScope::User(_) => None,
    };

    if let Some(user) = status_user {
        let done_ms = timestamp(&task.date_done);
        let closed_ms = timestamp(&task.date_closed);
        if done_ms >= date_from {
            activities.push(task_activity(task, "done", "completed task", done_ms, user));
        } else if closed_ms >= date_from {
            activities.push(task_activity(
                task,
                "closed",
                "closed task",
                closed_ms,
                user,
            ));
        } else if updated_ms >= date_from && updated_ms > created_ms {
            activities.push(task_activity(
                task,
                "updated",
                "updated task",
                updated_ms,
                user,
            ));
        }
    }

    if updated_ms < date_from {
        return;
    }

    let Ok(comments) = api.get_task_comments(&task.id).await else {
        return;
    };
    for comment in comments {
        let comment_ms = comment.date.parse::<i64>().ok().unwrap_or(0);
        if comment_ms < date_from {
            continue;
        }
        let user = match scope {
            ActivityScope::Team => comment.user,
            ActivityScope::User(user) if comment.user.id == user.id => user.clone(),
            ActivityScope::User(_) => continue,
        };
        activities.push(Activity {
            id: format!("{}-comment-{}", task.id, comment.id),
            user,
            type_: "commented on task".to_string(),
            date: comment_ms.to_string(),
            task_id: task.id.clone(),
            source: "api".to_string(),
            detail: Some(comment.comment_text),
            task_name: Some(task.name.clone()),
        });
    }
}

fn timestamp(value: &Option<String>) -> i64 {
    value
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
}

fn task_activity(task: &Task, id_suffix: &str, type_: &str, date: i64, user: User) -> Activity {
    Activity {
        id: format!("{}-{}", task.id, id_suffix),
        user,
        type_: type_.to_string(),
        date: date.to_string(),
        task_id: task.id.clone(),
        source: "api".to_string(),
        detail: Some(task.status.status.clone()),
        task_name: Some(task.name.clone()),
    }
}
