use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub id: i64,
    #[serde(deserialize_with = "deserialize_nullable_string", default)]
    pub username: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Member {
    pub user: User,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Status {
    pub status: String,
    #[serde(deserialize_with = "deserialize_nullable_string", default)]
    pub color: String,
    #[serde(rename = "type", deserialize_with = "deserialize_nullable_string", default)]
    pub type_: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct List {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub statuses: Vec<Status>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskStatus {
    pub status: String,
    pub color: Option<String>,
    #[serde(rename = "type")]
    pub type_: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub status: TaskStatus,
    #[serde(deserialize_with = "deserialize_parent", default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub assignees: Vec<User>,
    pub creator: User,
    pub date_created: Option<String>,
    pub date_updated: Option<String>,
    pub date_done: Option<String>,
    pub date_closed: Option<String>,
    pub text_content: Option<String>,
    #[serde(default)]
    pub tags: Vec<Tag>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    #[serde(deserialize_with = "deserialize_nullable_string", default)]
    pub comment_text: String,
    pub user: User,
    #[serde(deserialize_with = "deserialize_nullable_string", default)]
    pub date: String, // Unix ms string
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Activity {
    pub id: String,
    pub user: User,
    #[serde(rename = "type")]
    pub type_: String,
    pub date: String, // Unix ms string
    pub task_id: String,
    pub source: String,
    pub detail: Option<String>,
    pub task_name: Option<String>,
}

fn deserialize_parent<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ParentValue {
        String(String),
        Object { id: String },
        Null,
    }

    match Option::<ParentValue>::deserialize(deserializer)? {
        Some(ParentValue::String(s)) => {
            if s.is_empty() || s == "null" {
                Ok(None)
            } else {
                Ok(Some(s))
            }
        }
        Some(ParentValue::Object { id }) => Ok(Some(id)),
        Some(ParentValue::Null) | None => Ok(None),
    }
}

fn deserialize_nullable_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}
