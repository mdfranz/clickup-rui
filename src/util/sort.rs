use crate::clickup::models::{Comment, Task};

pub fn sort_tasks_by_updated_desc(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| {
        let a_time = a
            .date_updated
            .as_deref()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let b_time = b
            .date_updated
            .as_deref()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        b_time.cmp(&a_time)
    });
}

pub fn sort_comments_by_date_desc(comments: &mut [Comment]) {
    comments.sort_by(|a, b| {
        let a_time = a.date.parse::<i64>().ok().unwrap_or(0);
        let b_time = b.date.parse::<i64>().ok().unwrap_or(0);
        b_time.cmp(&a_time)
    });
}
