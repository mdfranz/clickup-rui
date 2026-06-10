use crate::cache::store::{CacheEntry, CacheStore, TaskListCacheEntry};
use crate::cache::ttl::*;
use crate::clickup::api::ClickUpApi;
use crate::clickup::models::*;
use crate::util::errors::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct CachedClient<A: ClickUpApi> {
    api: A,
    store: Arc<Mutex<CacheStore>>,
    bypass_cache: bool,
}

impl<A: ClickUpApi> CachedClient<A> {
    pub fn new(api: A, store: Arc<Mutex<CacheStore>>, bypass_cache: bool) -> Self {
        Self {
            api,
            store,
            bypass_cache,
        }
    }
}

fn compute_max_date_updated(tasks: &[Task]) -> i64 {
    tasks
        .iter()
        .filter_map(|t| t.date_updated.as_deref())
        .filter_map(|s| s.parse::<i64>().ok())
        .max()
        .unwrap_or(0)
}

fn merge_tasks(existing: &mut Vec<Task>, updates: Vec<Task>) {
    for update in updates {
        if let Some(pos) = existing.iter().position(|t| t.id == update.id) {
            existing[pos] = update;
        } else {
            existing.push(update);
        }
    }
}

impl<A: ClickUpApi> ClickUpApi for CachedClient<A> {
    async fn get_teams(&self) -> Result<Vec<Team>> {
        if !self.bypass_cache {
            let store = self.store.lock().await;
            if let Some(entry) = &store.teams {
                if !entry.is_expired() {
                    return Ok(entry.value.clone());
                }
            }
        }

        match self.api.get_teams().await {
            Ok(teams) => {
                let mut store = self.store.lock().await;
                store.teams = Some(CacheEntry {
                    value: teams.clone(),
                    expires_at: now_secs() + TTL_TEAMS,
                });
                // Also populate workspace users from team members
                for team in &teams {
                    let users: Vec<User> = team.members.iter().map(|m| m.user.clone()).collect();
                    store.workspace_users_by_workspace.insert(
                        team.id.clone(),
                        CacheEntry {
                            value: users,
                            expires_at: now_secs() + TTL_WS_USERS,
                        },
                    );
                }
                store.mark_dirty();
                Ok(teams)
            }
            Err(e) => {
                let store = self.store.lock().await;
                if let Some(entry) = &store.teams {
                    tracing::warn!("API get_teams failed, using stale cache: {:?}", e);
                    return Ok(entry.value.clone());
                }
                Err(e)
            }
        }
    }

    async fn get_current_user(&self) -> Result<User> {
        if !self.bypass_cache {
            let store = self.store.lock().await;
            if let Some(entry) = &store.user {
                if !entry.is_expired() {
                    return Ok(entry.value.clone());
                }
            }
        }

        match self.api.get_current_user().await {
            Ok(user) => {
                let mut store = self.store.lock().await;
                store.user = Some(CacheEntry {
                    value: user.clone(),
                    expires_at: now_secs() + TTL_USER,
                });
                store.mark_dirty();
                Ok(user)
            }
            Err(e) => {
                let store = self.store.lock().await;
                if let Some(entry) = &store.user {
                    tracing::warn!("API get_current_user failed, using stale cache: {:?}", e);
                    return Ok(entry.value.clone());
                }
                Err(e)
            }
        }
    }

    async fn get_spaces(&self, team_id: &str) -> Result<Vec<Space>> {
        if !self.bypass_cache {
            let store = self.store.lock().await;
            if let Some(entry) = store.spaces_by_team.get(team_id) {
                if !entry.is_expired() {
                    return Ok(entry.value.clone());
                }
            }
        }

        match self.api.get_spaces(team_id).await {
            Ok(spaces) => {
                let mut store = self.store.lock().await;
                store.spaces_by_team.insert(
                    team_id.to_string(),
                    CacheEntry {
                        value: spaces.clone(),
                        expires_at: now_secs() + TTL_SPACES,
                    },
                );
                store.mark_dirty();
                Ok(spaces)
            }
            Err(e) => {
                let store = self.store.lock().await;
                if let Some(entry) = store.spaces_by_team.get(team_id) {
                    tracing::warn!("API get_spaces failed, using stale cache: {:?}", e);
                    return Ok(entry.value.clone());
                }
                Err(e)
            }
        }
    }

    async fn get_folders(&self, space_id: &str) -> Result<Vec<Folder>> {
        if !self.bypass_cache {
            let store = self.store.lock().await;
            if let Some(entry) = store.folders_by_space.get(space_id) {
                if !entry.is_expired() {
                    return Ok(entry.value.clone());
                }
            }
        }

        match self.api.get_folders(space_id).await {
            Ok(folders) => {
                let mut store = self.store.lock().await;
                store.folders_by_space.insert(
                    space_id.to_string(),
                    CacheEntry {
                        value: folders.clone(),
                        expires_at: now_secs() + TTL_FOLDERS,
                    },
                );
                store.mark_dirty();
                Ok(folders)
            }
            Err(e) => {
                let store = self.store.lock().await;
                if let Some(entry) = store.folders_by_space.get(space_id) {
                    tracing::warn!("API get_folders failed, using stale cache: {:?}", e);
                    return Ok(entry.value.clone());
                }
                Err(e)
            }
        }
    }

    async fn get_lists(&self, folder_id: &str) -> Result<Vec<List>> {
        if !self.bypass_cache {
            let store = self.store.lock().await;
            if let Some(entry) = store.lists_by_folder.get(folder_id) {
                if !entry.is_expired() {
                    return Ok(entry.value.clone());
                }
            }
        }

        match self.api.get_lists(folder_id).await {
            Ok(lists) => {
                let mut store = self.store.lock().await;
                store.lists_by_folder.insert(
                    folder_id.to_string(),
                    CacheEntry {
                        value: lists.clone(),
                        expires_at: now_secs() + TTL_LISTS,
                    },
                );
                store.mark_dirty();
                Ok(lists)
            }
            Err(e) => {
                let store = self.store.lock().await;
                if let Some(entry) = store.lists_by_folder.get(folder_id) {
                    tracing::warn!("API get_lists failed, using stale cache: {:?}", e);
                    return Ok(entry.value.clone());
                }
                Err(e)
            }
        }
    }

    async fn get_list_detail(&self, list_id: &str) -> Result<List> {
        if !self.bypass_cache {
            let store = self.store.lock().await;
            if let Some(entry) = store.list_detail_by_list.get(list_id) {
                if !entry.is_expired() {
                    return Ok(entry.value.clone());
                }
            }
        }

        match self.api.get_list_detail(list_id).await {
            Ok(list) => {
                let mut store = self.store.lock().await;
                store.list_detail_by_list.insert(
                    list_id.to_string(),
                    CacheEntry {
                        value: list.clone(),
                        expires_at: now_secs() + TTL_LIST_DETAIL,
                    },
                );
                store.mark_dirty();
                Ok(list)
            }
            Err(e) => {
                let store = self.store.lock().await;
                if let Some(entry) = store.list_detail_by_list.get(list_id) {
                    tracing::warn!("API get_list_detail failed, using stale cache: {:?}", e);
                    return Ok(entry.value.clone());
                }
                Err(e)
            }
        }
    }

    async fn get_tasks(&self, list_id: &str, include_closed: bool) -> Result<Vec<Task>> {
        let mut do_incremental = false;
        let mut max_date_updated = 0;
        let mut existing_tasks = Vec::new();

        if !self.bypass_cache {
            let store = self.store.lock().await;
            if let Some(entry) = store.tasks.get(list_id) {
                if include_closed && !entry.includes_closed {
                    // Need full fetch to get closed tasks
                } else if now_secs() - entry.fetched_at < TTL_TASKS_FULL {
                    do_incremental = true;
                    max_date_updated = entry.max_date_updated;
                    existing_tasks = entry.tasks.clone();
                }
            }
        }

        if do_incremental {
            match self
                .api
                .get_tasks_incremental(list_id, max_date_updated)
                .await
            {
                Ok(updates) => {
                    let mut store = self.store.lock().await;
                    let mut result = {
                        let entry = store.tasks.entry(list_id.to_string()).or_insert_with(|| {
                            TaskListCacheEntry {
                                tasks: existing_tasks,
                                fetched_at: now_secs(),
                                max_date_updated: 0,
                                includes_closed: include_closed,
                            }
                        });

                        if !updates.is_empty() {
                            merge_tasks(&mut entry.tasks, updates);
                            entry.max_date_updated = compute_max_date_updated(&entry.tasks);
                        }
                        entry.fetched_at = now_secs();
                        if include_closed {
                            entry.includes_closed = true;
                        }
                        entry.tasks.clone()
                    };
                    store.mark_dirty();

                    if !include_closed {
                        result.retain(|t| t.status.status.to_lowercase() != "closed");
                    }
                    return Ok(result);
                }
                Err(e) => {
                    let store = self.store.lock().await;
                    if let Some(entry) = store.tasks.get(list_id) {
                        tracing::warn!("Incremental task fetch failed, using stale cache: {:?}", e);
                        let mut result = entry.tasks.clone();
                        if !include_closed {
                            result.retain(|t| t.status.status.to_lowercase() != "closed");
                        }
                        return Ok(result);
                    }
                    return Err(e);
                }
            }
        }

        match self.api.get_tasks(list_id, include_closed).await {
            Ok(tasks) => {
                let max_date = compute_max_date_updated(&tasks);
                let mut store = self.store.lock().await;
                store.tasks.insert(
                    list_id.to_string(),
                    TaskListCacheEntry {
                        tasks: tasks.clone(),
                        fetched_at: now_secs(),
                        max_date_updated: max_date,
                        includes_closed: include_closed,
                    },
                );
                store.mark_dirty();

                let mut result = tasks;
                if !include_closed {
                    result.retain(|t| t.status.status.to_lowercase() != "closed");
                }
                Ok(result)
            }
            Err(e) => {
                let store = self.store.lock().await;
                if let Some(entry) = store.tasks.get(list_id) {
                    tracing::warn!("Full task fetch failed, using stale cache: {:?}", e);
                    let mut result = entry.tasks.clone();
                    if !include_closed {
                        result.retain(|t| t.status.status.to_lowercase() != "closed");
                    }
                    return Ok(result);
                }
                Err(e)
            }
        }
    }

    async fn get_tasks_incremental(&self, list_id: &str, date_updated_gt: i64) -> Result<Vec<Task>> {
        self.api.get_tasks_incremental(list_id, date_updated_gt).await
    }

    async fn get_task_detail(&self, task_id: &str) -> Result<Task> {
        if !self.bypass_cache {
            let store = self.store.lock().await;
            if let Some(entry) = store.task_detail_by_task.get(task_id) {
                if !entry.is_expired() {
                    return Ok(entry.value.clone());
                }
            }
        }

        match self.api.get_task_detail(task_id).await {
            Ok(task) => {
                let mut store = self.store.lock().await;
                store.task_detail_by_task.insert(
                    task_id.to_string(),
                    CacheEntry {
                        value: task.clone(),
                        expires_at: now_secs() + TTL_TASK_DETAIL,
                    },
                );
                store.mark_dirty();
                Ok(task)
            }
            Err(e) => {
                let store = self.store.lock().await;
                if let Some(entry) = store.task_detail_by_task.get(task_id) {
                    tracing::warn!("API get_task_detail failed, using stale cache: {:?}", e);
                    return Ok(entry.value.clone());
                }
                Err(e)
            }
        }
    }

    async fn get_task_comments(&self, task_id: &str) -> Result<Vec<Comment>> {
        if !self.bypass_cache {
            let store = self.store.lock().await;
            if let Some(entry) = store.comments_by_task.get(task_id) {
                if !entry.is_expired() {
                    return Ok(entry.value.clone());
                }
            }
        }

        match self.api.get_task_comments(task_id).await {
            Ok(comments) => {
                let mut store = self.store.lock().await;
                store.comments_by_task.insert(
                    task_id.to_string(),
                    CacheEntry {
                        value: comments.clone(),
                        expires_at: now_secs() + TTL_COMMENTS,
                    },
                );
                store.mark_dirty();
                Ok(comments)
            }
            Err(e) => {
                let store = self.store.lock().await;
                if let Some(entry) = store.comments_by_task.get(task_id) {
                    tracing::warn!("API get_task_comments failed, using stale cache: {:?}", e);
                    return Ok(entry.value.clone());
                }
                Err(e)
            }
        }
    }

    async fn update_task_status(&self, task_id: &str, status: &str) -> Result<Task> {
        let task = self.api.update_task_status(task_id, status).await?;

        let mut store = self.store.lock().await;
        // Invalidate task detail cache
        store.task_detail_by_task.remove(task_id);
        // Remove that task from first cached list containing it
        for list_entry in store.tasks.values_mut() {
            if let Some(pos) = list_entry.tasks.iter().position(|t| t.id == task_id) {
                list_entry.tasks.remove(pos);
                break;
            }
        }
        store.mark_dirty();

        Ok(task)
    }

    async fn create_task_comment(&self, task_id: &str, comment_text: &str) -> Result<Comment> {
        let comment = self.api.create_task_comment(task_id, comment_text).await?;

        let mut store = self.store.lock().await;
        // Invalidate comments cache
        store.comments_by_task.remove(task_id);
        store.mark_dirty();

        Ok(comment)
    }

    async fn create_task(
        &self,
        list_id: &str,
        name: &str,
        description: Option<&str>,
        status: Option<&str>,
        assignees: Option<&[i64]>,
    ) -> Result<Task> {
        let task = self
            .api
            .create_task(list_id, name, description, status, assignees)
            .await?;

        let mut store = self.store.lock().await;
        // Invalidate the cache for the target list
        store.tasks.remove(list_id);
        store.mark_dirty();

        Ok(task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use crate::util::errors::AppError;

    struct MockApi {
        tasks: std::sync::Mutex<Vec<Task>>,
        incremental_tasks: std::sync::Mutex<Vec<Task>>,
        should_fail: AtomicBool,
    }

    impl ClickUpApi for MockApi {
        async fn get_teams(&self) -> Result<Vec<Team>> { unimplemented!() }
        async fn get_current_user(&self) -> Result<User> { unimplemented!() }
        async fn get_spaces(&self, _team_id: &str) -> Result<Vec<Space>> { unimplemented!() }
        async fn get_folders(&self, _space_id: &str) -> Result<Vec<Folder>> { unimplemented!() }
        async fn get_lists(&self, _folder_id: &str) -> Result<Vec<List>> { unimplemented!() }
        async fn get_list_detail(&self, _list_id: &str) -> Result<List> { unimplemented!() }
        
        async fn get_tasks(&self, _list_id: &str, _include_closed: bool) -> Result<Vec<Task>> {
            if self.should_fail.load(Ordering::SeqCst) {
                return Err(AppError::Other("Network error".to_string()));
            }
            Ok(self.tasks.lock().unwrap().clone())
        }

        async fn get_tasks_incremental(&self, _list_id: &str, _date_updated_gt: i64) -> Result<Vec<Task>> {
            if self.should_fail.load(Ordering::SeqCst) {
                return Err(AppError::Other("Network error".to_string()));
            }
            Ok(self.incremental_tasks.lock().unwrap().clone())
        }

        async fn get_task_detail(&self, _task_id: &str) -> Result<Task> { unimplemented!() }
        async fn get_task_comments(&self, _task_id: &str) -> Result<Vec<Comment>> { unimplemented!() }
        async fn update_task_status(&self, _task_id: &str, _status: &str) -> Result<Task> { unimplemented!() }
        async fn create_task_comment(&self, _task_id: &str, _comment_text: &str) -> Result<Comment> { unimplemented!() }
        async fn create_task(
            &self,
            _list_id: &str,
            _name: &str,
            _description: Option<&str>,
            _status: Option<&str>,
            _assignees: Option<&[i64]>,
        ) -> Result<Task> { unimplemented!() }
    }

    fn make_test_task(id: &str, name: &str, date_updated: &str, status: &str) -> Task {
        let user = User {
            id: 123,
            username: "test_user".to_string(),
            email: "test@example.com".to_string(),
        };
        Task {
            id: id.to_string(),
            name: name.to_string(),
            status: TaskStatus {
                status: status.to_string(),
                color: Some("#000000".to_string()),
                type_: Some("".to_string()),
            },
            parent_id: None,
            assignees: vec![],
            creator: user,
            date_created: None,
            date_updated: Some(date_updated.to_string()),
            date_done: None,
            date_closed: None,
            text_content: None,
        }
    }

    #[tokio::test]
    async fn test_cache_full_fetch_and_incremental_merge() {
        let store = Arc::new(Mutex::new(CacheStore::new()));
        
        let t1 = make_test_task("1", "Task One", "1000", "open");
        let t2 = make_test_task("2", "Task Two", "2000", "open");
        
        let mock_api = MockApi {
            tasks: std::sync::Mutex::new(vec![t1.clone(), t2.clone()]),
            incremental_tasks: std::sync::Mutex::new(vec![]),
            should_fail: AtomicBool::new(false),
        };
        
        let cached_client = CachedClient::new(mock_api, store.clone(), false);
        
        // 1. Initial fetch -> Cache is empty, so should do full fetch
        let res = cached_client.get_tasks("list_123", false).await.unwrap();
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].id, "1");
        assert_eq!(res[1].id, "2");
        
        // Check cache store is updated
        {
            let s = store.lock().await;
            let entry = s.tasks.get("list_123").unwrap();
            assert_eq!(entry.tasks.len(), 2);
            assert_eq!(entry.max_date_updated, 2000);
        }
        
        // 2. Incremental fetch:
        // Set fetched_at to a past time so it triggers incremental fetch
        {
            let mut s = store.lock().await;
            let entry = s.tasks.get_mut("list_123").unwrap();
            entry.fetched_at = now_secs() - 10; // Trigger incremental
        }
        
        // Mock updated task t1 (updated name & date_updated) and new task t3
        let t1_updated = make_test_task("1", "Task One Updated", "3000", "in progress");
        let t3 = make_test_task("3", "Task Three", "4000", "open");
        
        *cached_client.api.incremental_tasks.lock().unwrap() = vec![t1_updated, t3];
        
        // Get tasks again
        let res2 = cached_client.get_tasks("list_123", false).await.unwrap();
        
        // Check returned tasks: t1 should be updated, t2 remains, t3 should be added!
        assert_eq!(res2.len(), 3);
        
        let task1 = res2.iter().find(|t| t.id == "1").unwrap();
        assert_eq!(task1.name, "Task One Updated");
        assert_eq!(task1.status.status, "in progress");
        assert_eq!(task1.date_updated.as_deref(), Some("3000"));
        
        let task2 = res2.iter().find(|t| t.id == "2").unwrap();
        assert_eq!(task2.name, "Task Two");
        
        let task3 = res2.iter().find(|t| t.id == "3").unwrap();
        assert_eq!(task3.name, "Task Three");
        assert_eq!(task3.date_updated.as_deref(), Some("4000"));
        
        // Check cache store has been updated correctly with max_date_updated
        {
            let s = store.lock().await;
            let entry = s.tasks.get("list_123").unwrap();
            assert_eq!(entry.max_date_updated, 4000);
        }
    }

    #[tokio::test]
    async fn test_cache_stale_fallback() {
        let store = Arc::new(Mutex::new(CacheStore::new()));
        
        let t1 = make_test_task("1", "Task One", "1000", "open");
        
        // Populate cache store directly
        {
            let mut s = store.lock().await;
            s.tasks.insert("list_123".to_string(), TaskListCacheEntry {
                tasks: vec![t1.clone()],
                fetched_at: now_secs() - 10,
                max_date_updated: 1000,
                includes_closed: false,
            });
        }
        
        let mock_api = MockApi {
            tasks: std::sync::Mutex::new(vec![]),
            incremental_tasks: std::sync::Mutex::new(vec![]),
            should_fail: AtomicBool::new(true), // API fails!
        };
        
        let cached_client = CachedClient::new(mock_api, store.clone(), false);
        
        // This should fall back to cached tasks despite API failing
        let res = cached_client.get_tasks("list_123", false).await.unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].id, "1");
        assert_eq!(res[0].name, "Task One");
    }
}

