use crate::clickup::models::*;
use crate::util::env::get_gemini_api_key;
use crate::util::errors::{AppError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<Content>,
}

#[derive(Serialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Serialize)]
struct Part {
    text: String,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<Candidate>>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<GeminiContent>,
}

#[derive(Deserialize)]
struct GeminiContent {
    parts: Option<Vec<GeminiPart>>,
}

#[derive(Deserialize)]
struct GeminiPart {
    text: Option<String>,
}

pub struct GeminiSummarizer {
    client: Client,
    model: String,
}

impl GeminiSummarizer {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
            model: "gemini-3.5-flash".to_string(),
        }
    }

    async fn generate_content(&self, prompt: &str) -> Result<String> {
        let key = get_gemini_api_key()?;
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, key
        );

        let req_body = GeminiRequest {
            contents: vec![Content {
                parts: vec![Part {
                    text: prompt.to_string(),
                }],
            }],
        };

        let res = self.client.post(&url).json(&req_body).send().await?;
        if !res.status().is_success() {
            let status = res.status();
            let err_body = res.text().await.unwrap_or_default();
            return Err(AppError::AiError(format!(
                "Gemini API returned status {}: {}",
                status,
                err_body
            )));
        }

        let resp_body: GeminiResponse = res.json().await?;
        if let Some(candidates) = resp_body.candidates {
            if let Some(candidate) = candidates.first() {
                if let Some(content) = &candidate.content {
                    if let Some(parts) = &content.parts {
                        if let Some(part) = parts.first() {
                            if let Some(text) = &part.text {
                                return Ok(text.trim().to_string());
                            }
                        }
                    }
                }
            }
        }

        Ok("No summary generated.".to_string())
    }

    pub async fn summarize_task(&self, task: &Task, comments: &[Comment]) -> Result<String> {
        let mut info = format!(
            "Task: {}\nDescription: {}\nStatus: {}\n",
            task.name,
            task.text_content.as_deref().unwrap_or("No description"),
            task.status.status
        );
        if !comments.is_empty() {
            info.push_str("Comments:\n");
            for c in comments {
                info.push_str(&format!("- {}: {}\n", c.user.username, c.comment_text));
            }
        }

        let prompt = format!(
            "You are a factual, concise project assistant. Summarize this ClickUp task and its comments. Keep your output short, factual, and direct without introductory or concluding pleasantries:\n\n{}",
            info
        );

        self.generate_content(&prompt).await
    }

    pub async fn summarize_tasks(&self, folder_name: &str, tasks: &[Task]) -> Result<String> {
        let mut info = format!("Folder: {}\nTasks:\n", folder_name);
        for t in tasks {
            info.push_str(&format!(
                "- [{}] {} (Assignees: {})\n",
                t.status.status,
                t.name,
                t.assignees
                    .iter()
                    .map(|u| u.username.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        let prompt = format!(
            "You are a factual, concise project manager. Generate a concise, factual markdown summary of the following tasks under folder '{}'. Keep your response direct and formatted with clear headings or bullet points where useful:\n\n{}",
            folder_name, info
        );

        self.generate_content(&prompt).await
    }

    pub async fn summarize_user_activity(
        &self,
        user_name: &str,
        day: &str,
        activities: &[Activity],
        _task_details: &[Task],
        _task_comments: &[Comment],
    ) -> Result<String> {
        let mut info = format!("User: {}\nDay: {}\nActivities:\n", user_name, day);
        for act in activities {
            let t_name = act.task_name.as_deref().unwrap_or("Unknown Task");
            info.push_str(&format!(
                "- {} on task \"{}\" (ID: {}) via {} [Detail: {}]\n",
                act.type_,
                t_name,
                act.task_id,
                act.source,
                act.detail.as_deref().unwrap_or("N/A")
            ));
        }

        let prompt = format!(
            "You are a factual, concise project assistant. Generate a concise, factual markdown summary of the user '{}''s activities for {}. Keep it concise, helpful, and direct:\n\n{}",
            user_name, day, info
        );

        self.generate_content(&prompt).await
    }

    pub async fn summarize_team_activity(
        &self,
        days: u32,
        user_activities: &[(String, Vec<Activity>)],
        _task_details: &[Task],
    ) -> Result<String> {
        let mut info = format!("Team Activity Report for last {} days\n", days);
        for (username, activities) in user_activities {
            info.push_str(&format!("\nUser: {}\n", username));
            for act in activities {
                let t_name = act.task_name.as_deref().unwrap_or("Unknown Task");
                let detail_str = act.detail.as_deref().unwrap_or("N/A");
                info.push_str(&format!(
                    "- {} on task \"{}\" (ID: {}) [Detail: {}] via {}\n",
                    act.type_, t_name, act.task_id, detail_str, act.source
                ));
            }
        }

        let prompt = format!(
            "You are a factual, concise project director. Generate a concise, factual markdown summary of team activities for the last {} days. Avoid hyperbolic or overly praise-filled language. Focus on raw accomplishments, updates, and status changes:\n\n{}",
            days, info
        );

        self.generate_content(&prompt).await
    }
}
