use crate::clickup::api::ClickUpApi;
use crate::clickup::models::*;
use crate::util::errors::{AppError, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::Instant;

pub struct ClickUpClient {
    client: reqwest::Client,
    pat: String,
    base_url: String,
}

impl ClickUpClient {
    pub fn new(pat: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            pat,
            base_url: "https://api.clickup.com/api/v2".to_string(),
        }
    }

    pub fn new_with_url(pat: String, base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            pat,
            base_url,
        }
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(mut val) = HeaderValue::from_str(&self.pat) {
            val.set_sensitive(true);
            headers.insert(AUTHORIZATION, val);
        }
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers
    }

    async fn request<T, R>(&self, method: reqwest::Method, path: &str, body: Option<&T>) -> Result<R>
    where
        T: Serialize + ?Sized,
        R: for<'de> Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url, path);
        let mut builder = self.client.request(method.clone(), &url).headers(self.headers());

        if let Some(b) = body {
            builder = builder.json(b);
        }

        let request_log_body = if let Some(b) = body {
            if crate::util::env::is_log_sensitive_data() {
                serde_json::to_string(b).unwrap_or_default()
            } else {
                "[redacted]".to_string()
            }
        } else {
            String::new()
        };

        tracing::debug!(
            "Sending request: method={} url={} body={}",
            method,
            url,
            request_log_body
        );

        let start = Instant::now();
        let res = builder.send().await?;
        let elapsed = start.elapsed();

        let status = res.status();
        tracing::debug!(
            "Received response: method={} url={} status={} latency={:?}",
            method,
            url,
            status,
            elapsed
        );

        if !status.is_success() {
            let err_body = res.text().await.unwrap_or_default();
            tracing::error!(
                "API Error: status={} body={} url={}",
                status,
                err_body,
                url
            );
            return Err(AppError::ApiError {
                status: status.as_u16(),
                message: err_body,
            });
        }

        let body_bytes = res.bytes().await?;
        if crate::util::env::is_log_response_bodies() {
            if let Ok(body_str) = std::str::from_utf8(&body_bytes) {
                tracing::debug!("Response body: {}", body_str);
            }
        } else {
            tracing::debug!("Response body: [redacted]");
        }

        let parsed: R = serde_json::from_slice(&body_bytes)?;
        Ok(parsed)
    }
}

// Wrapper types for deserializing ClickUp's responses
#[derive(Deserialize)]
struct TeamsResponse {
    teams: Vec<Team>,
}

#[derive(Deserialize)]
struct UserResponse {
    user: User,
}

#[derive(Deserialize)]
struct SpacesResponse {
    spaces: Vec<Space>,
}

#[derive(Deserialize)]
struct FoldersResponse {
    folders: Vec<Folder>,
}

#[derive(Deserialize)]
struct ListsResponse {
    lists: Vec<List>,
}

#[derive(Deserialize)]
struct TasksResponse {
    tasks: Vec<Task>,
}

#[derive(Deserialize)]
struct CommentsResponse {
    comments: Vec<Comment>,
}

#[derive(Serialize)]
struct UpdateStatusRequest<'a> {
    status: &'a str,
}

#[derive(Serialize)]
struct CreateCommentRequest<'a> {
    comment_text: &'a str,
}

#[derive(Serialize)]
struct CreateTaskRequest<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assignees: Option<&'a [i64]>,
}

impl ClickUpApi for ClickUpClient {
    async fn get_teams(&self) -> Result<Vec<Team>> {
        let resp: TeamsResponse = self.request(reqwest::Method::GET, "/team", None::<&()>).await?;
        Ok(resp.teams)
    }

    async fn get_current_user(&self) -> Result<User> {
        let resp: UserResponse = self.request(reqwest::Method::GET, "/user", None::<&()>).await?;
        Ok(resp.user)
    }

    async fn get_spaces(&self, team_id: &str) -> Result<Vec<Space>> {
        let path = format!("/team/{}/space?archived=false", team_id);
        let resp: SpacesResponse = self.request(reqwest::Method::GET, &path, None::<&()>).await?;
        Ok(resp.spaces)
    }

    async fn get_folders(&self, space_id: &str) -> Result<Vec<Folder>> {
        let path = format!("/space/{}/folder?archived=false", space_id);
        let resp: FoldersResponse = self.request(reqwest::Method::GET, &path, None::<&()>).await?;
        Ok(resp.folders)
    }

    async fn get_lists(&self, folder_id: &str) -> Result<Vec<List>> {
        let path = format!("/folder/{}/list?archived=false", folder_id);
        let resp: ListsResponse = self.request(reqwest::Method::GET, &path, None::<&()>).await?;
        Ok(resp.lists)
    }

    async fn get_list_detail(&self, list_id: &str) -> Result<List> {
        let path = format!("/list/{}", list_id);
        self.request(reqwest::Method::GET, &path, None::<&()>).await
    }

    async fn get_tasks(&self, list_id: &str, include_closed: bool) -> Result<Vec<Task>> {
        let path = format!(
            "/list/{}/task?archived=false&include_closed={}&subtasks=true",
            list_id, include_closed
        );
        let resp: TasksResponse = self.request(reqwest::Method::GET, &path, None::<&()>).await?;
        Ok(resp.tasks)
    }

    async fn get_tasks_incremental(&self, list_id: &str, date_updated_gt: i64) -> Result<Vec<Task>> {
        let path = format!(
            "/list/{}/task?archived=false&include_closed=true&subtasks=true&date_updated_gt={}",
            list_id, date_updated_gt
        );
        let resp: TasksResponse = self.request(reqwest::Method::GET, &path, None::<&()>).await?;
        Ok(resp.tasks)
    }

    async fn get_task_detail(&self, task_id: &str) -> Result<Task> {
        let path = format!("/task/{}", task_id);
        self.request(reqwest::Method::GET, &path, None::<&()>).await
    }

    async fn get_task_comments(&self, task_id: &str) -> Result<Vec<Comment>> {
        let path = format!("/task/{}/comment", task_id);
        let resp: CommentsResponse = self.request(reqwest::Method::GET, &path, None::<&()>).await?;
        Ok(resp.comments)
    }

    async fn update_task_status(&self, task_id: &str, status: &str) -> Result<Task> {
        let path = format!("/task/{}", task_id);
        let body = UpdateStatusRequest { status };
        self.request(reqwest::Method::PUT, &path, Some(&body)).await
    }

    async fn create_task_comment(&self, task_id: &str, comment_text: &str) -> Result<Comment> {
        let path = format!("/task/{}/comment", task_id);
        let body = CreateCommentRequest { comment_text };
        self.request(reqwest::Method::POST, &path, Some(&body)).await
    }

    async fn create_task(
        &self,
        list_id: &str,
        name: &str,
        description: Option<&str>,
        status: Option<&str>,
        assignees: Option<&[i64]>,
    ) -> Result<Task> {
        let path = format!("/list/{}/task", list_id);
        let body = CreateTaskRequest {
            name,
            description,
            status,
            assignees,
        };
        self.request(reqwest::Method::POST, &path, Some(&body)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_client_get_teams() {
        // Start a mock server
        let mock_server = MockServer::start().await;

        // Mock response body for get_teams
        let teams_json = serde_json::json!({
            "teams": [
                {
                    "id": "team_1",
                    "name": "My Awesome Team",
                    "members": [
                        {
                            "user": {
                                "id": 123,
                                "username": "matthew",
                                "email": "matthew@example.com"
                            }
                        }
                    ]
                }
            ]
        });

        // Set up the mock expectation
        Mock::given(method("GET"))
            .and(path("/team"))
            .and(header("Authorization", "pk_test_token"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(teams_json))
            .mount(&mock_server)
            .await;

        // Create client pointing to mock server
        let client = ClickUpClient::new_with_url("pk_test_token".to_string(), mock_server.uri());

        // Invoke get_teams
        let teams = client.get_teams().await.unwrap();

        // Assertions
        assert_eq!(teams.len(), 1);
        assert_eq!(teams[0].id, "team_1");
        assert_eq!(teams[0].name, "My Awesome Team");
        assert_eq!(teams[0].members[0].user.id, 123);
        assert_eq!(teams[0].members[0].user.username, "matthew");
    }

    #[tokio::test]
    async fn test_client_api_error_handling() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/team"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized access"))
            .mount(&mock_server)
            .await;

        let client = ClickUpClient::new_with_url("invalid_token".to_string(), mock_server.uri());
        let res = client.get_teams().await;

        assert!(res.is_err());
        match res.unwrap_err() {
            AppError::ApiError { status, message } => {
                assert_eq!(status, 401);
                assert_eq!(message, "Unauthorized access");
            }
            other => panic!("Expected ApiError, got {:?}", other),
        }
    }
}

