use crate::clickup::models::*;
use crate::util::env::get_gemini_api_key;
use crate::util::errors::{AppError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::Config;

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

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

pub struct AiSummarizer {
    client: Client,
    config: Config,
}

pub type GeminiSummarizer = AiSummarizer;

impl AiSummarizer {
    pub fn new() -> Self {
        let config = Config::load().unwrap_or_else(|_| Config {
            workspace_id: String::new(),
            workspace_name: String::new(),
            space_id: String::new(),
            space_name: String::new(),
            folders: vec![],
            ai_provider: "gemini".to_string(),
            ai_model: "gemini-3.5-flash".to_string(),
            ollama_url: "http://localhost:11434".to_string(),
        });

        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .unwrap_or_else(|_| Client::new()),
            config,
        }
    }

    fn get_effective_model(&self) -> &str {
        if self.config.ai_provider == "ollama" && self.config.ai_model == "gemini-3.5-flash" {
            "granite4.1:8b"
        } else {
            &self.config.ai_model
        }
    }

    async fn generate_content(&self, prompt: &str) -> Result<String> {
        match self.config.ai_provider.as_str() {
            "ollama" => {
                let model = self.get_effective_model().to_string();
                let url = format!("{}/api/generate", self.config.ollama_url.trim_end_matches('/'));
                let req_body = OllamaRequest {
                    model,
                    prompt: prompt.to_string(),
                    stream: false,
                };

                let res = self.client.post(&url).json(&req_body).send().await?;
                if !res.status().is_success() {
                    let status = res.status();
                    let err_body = res.text().await.unwrap_or_default();
                    return Err(AppError::AiError(format!(
                        "Ollama API returned status {}: {}",
                        status,
                        err_body
                    )));
                }

                let resp_body: OllamaResponse = res.json().await?;
                Ok(resp_body.response.trim().to_string())
            }
            _ => {
                let key = get_gemini_api_key()?;
                let model = self.get_effective_model();
                let url = format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                    model, key
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
        }
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
            "You are a factual, concise project assistant. Summarize this ClickUp task and its comments. Keep your output short, factual, and direct without introductory or concluding pleasantries. Do NOT include task IDs (e.g. ID: ...) in your response:\n\n{}",
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
            "You are a factual, concise project manager. Generate a concise, factual markdown summary of the following tasks under folder '{}'. Keep your response direct and formatted with clear headings or bullet points where useful. Do NOT include task IDs (e.g. ID: ...) in your response:\n\n{}",
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
                "- {} on task \"{}\" via {} [Detail: {}]\n",
                act.type_,
                t_name,
                act.source,
                act.detail.as_deref().unwrap_or("N/A")
            ));
        }

        let prompt = format!(
            "You are a factual, concise project assistant. Generate a concise, factual markdown summary of the user '{}''s activities for {}. Keep it concise, helpful, and direct. Do NOT include task IDs (e.g. ID: ...) in your response:\n\n{}",
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
        let mut info = String::new();
        for (username, activities) in user_activities {
            info.push_str(&format!("\nUser: {}\n", username));
            for act in activities {
                let dt = crate::util::format::format_comment_date(&act.date);
                let t_name = act.task_name.as_deref().unwrap_or("Unknown Task");
                let detail_str = act.detail.as_deref().unwrap_or("N/A");
                info.push_str(&format!(
                    "[{}] {} {} \"{}\" [Detail: {}] via {}\n",
                    dt, act.user.username, act.type_, t_name, detail_str, act.source
                ));
            }
        }

        let prompt = format!(
            r#"Analyze the following ClickUp activity logs for the team over the last {days} days and generate a factual, objective, and concise **Team Activity Summary** in Markdown.

Strict Guidelines:
1. Do not use hyperbolic, grandiose, or embellished language. Avoid adjectives like "instrumental," "vital," "significant," "collaboration has been high," etc.
2. Be strictly factual and base everything directly on the logs.
3. Do not use emojis in headers.
4. Capture and include specific dates/times (e.g., "on 05/29") when describing when specific tasks were completed, updated, or commented on, based directly on the timestamp bracketed in the logs.
5. Do NOT include task IDs (e.g. ID: ..., or any numeric ClickUp task ID strings) anywhere in your response.
6. Include activities and comments if they are provided. Indicate when no details have been provided on state changes.

Format the summary with the following structure:

# Team Status Report (Last {days} Days)

Provide a 3-5 sentence summary of the period of review.

## Key Achievements & Completed Work
- List specific tasks that were completed or closed in the last {days} days based directly on the logs. Keep description of achievements factual and objective.

## Individual Activity & Progress
For each active team member, provide a concise bulleted list or a direct, factual 1-2 sentence summary of the specific tasks they created, updated, or commented on. Do not embellish their role or impact.
Format:
- **[Member Name]**: [Factual summary of what tasks they updated, created, or commented on]

## Discussions & Comments
- Summarize specific key points discussed in task comments based on the log (e.g., vendor meetings, SOC2 collection, groups). If no relevant discussion, state "No discussion logs available."

## Blockers, Risks & Friction
- Factual list of tasks currently blocked or showing delays, with the reported reason. If none, state "No blockers reported."
- Capture open/in progress/blocked tasks that have not been updated during the review period that may need attention.

Team Activity Logs:
{info}"#,
            days = days,
            info = info
        );

        self.generate_content(&prompt).await
    }
}
