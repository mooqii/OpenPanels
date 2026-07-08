use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelKind {
    Wiki,
    Canvas,
    Image,
    Diff,
    Preview,
    Files,
}

impl PanelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wiki => "wiki",
            Self::Canvas => "canvas",
            Self::Image => "image",
            Self::Diff => "diff",
            Self::Preview => "preview",
            Self::Files => "files",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "wiki" => Some(Self::Wiki),
            "canvas" => Some(Self::Canvas),
            "image" => Some(Self::Image),
            "diff" => Some(Self::Diff),
            "preview" => Some(Self::Preview),
            "files" => Some(Self::Files),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub panel_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Panel {
    pub id: String,
    pub session_id: String,
    pub kind: PanelKind,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectPanelSnapshot {
    pub panel: Panel,
    pub state: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBootstrap {
    pub active_panel_id: String,
    pub active_panel_kind: PanelKind,
    pub context_dir: String,
    pub context_id: String,
    pub context_id_source: String,
    pub panel: Panel,
    pub panel_dir: String,
    pub panels: Vec<ProjectPanelSnapshot>,
    pub session: Session,
    pub sessions: Vec<Session>,
    pub state: Value,
    pub storage_dir: String,
}
