use crate::clickup::models::*;
use crate::util::errors::Result;

#[allow(async_fn_in_trait)]
pub trait ClickUpApi: Send + Sync {
    async fn get_teams(&self) -> Result<Vec<Team>>;
    async fn get_current_user(&self) -> Result<User>;
    async fn get_spaces(&self, team_id: &str) -> Result<Vec<Space>>;
    async fn get_folders(&self, space_id: &str) -> Result<Vec<Folder>>;
    async fn get_lists(&self, folder_id: &str) -> Result<Vec<List>>;
    async fn get_list_detail(&self, list_id: &str) -> Result<List>;
    async fn get_tasks(&self, list_id: &str, include_closed: bool) -> Result<Vec<Task>>;
    async fn get_tasks_incremental(&self, list_id: &str, date_updated_gt: i64) -> Result<Vec<Task>>;
    fn get_task_detail(&self, task_id: &str) -> impl std::future::Future<Output = Result<Task>> + Send;
    fn get_task_comments(&self, task_id: &str) -> impl std::future::Future<Output = Result<Vec<Comment>>> + Send;
    async fn update_task_status(&self, task_id: &str, status: &str) -> Result<Task>;
    async fn create_task_comment(&self, task_id: &str, comment_text: &str) -> Result<Comment>;
    async fn create_task(
        &self,
        list_id: &str,
        name: &str,
        description: Option<&str>,
        status: Option<&str>,
        assignees: Option<&[i64]>,
    ) -> Result<Task>;
}
