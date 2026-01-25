use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use schemars::JsonSchema;
use thiserror::Error;
use utoipa::ToSchema;

pub use sandbox_agent_agent_schema::{amp, claude, codex, opencode};

pub mod agents;

pub use agents::{amp as convert_amp, claude as convert_claude, codex as convert_codex, opencode as convert_opencode};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UniversalEvent {
    pub id: u64,
    pub timestamp: String,
    pub session_id: String,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    pub data: UniversalEventData,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(untagged)]
pub enum UniversalEventData {
    Message { message: UniversalMessage },
    Started { started: Started },
    Error { error: CrashInfo },
    QuestionAsked {
        #[serde(rename = "questionAsked")]
        question_asked: QuestionRequest,
    },
    PermissionAsked {
        #[serde(rename = "permissionAsked")]
        permission_asked: PermissionRequest,
    },
    Unknown { raw: Value },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CrashInfo {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct UniversalMessageParsed {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: Map<String, Value>,
    pub parts: Vec<UniversalMessagePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(untagged)]
pub enum UniversalMessage {
    Parsed(UniversalMessageParsed),
    Unparsed {
        raw: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UniversalMessagePart {
    Text { text: String },
    ToolCall {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        name: String,
        input: Value,
    },
    ToolResult {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        output: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    FunctionCall {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        arguments: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw: Option<Value>,
    },
    FunctionResult {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        result: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw: Option<Value>,
    },
    File {
        source: AttachmentSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw: Option<Value>,
    },
    Image {
        source: AttachmentSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alt: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw: Option<Value>,
    },
    Error { message: String },
    Unknown { raw: Value },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AttachmentSource {
    Path { path: String },
    Url { url: String },
    Data {
        data: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        encoding: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuestionRequest {
    pub id: String,
    pub session_id: String,
    pub questions: Vec<QuestionInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<QuestionToolRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuestionInfo {
    pub question: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    pub options: Vec<QuestionOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_select: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuestionToolRef {
    pub message_id: String,
    pub call_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub id: String,
    pub session_id: String,
    pub permission: String,
    pub patterns: Vec<String>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: Map<String, Value>,
    pub always: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<PermissionToolRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PermissionToolRef {
    pub message_id: String,
    pub call_id: String,
}

#[derive(Debug, Error)]
pub enum ConversionError {
    #[error("unsupported conversion: {0}")]
    Unsupported(&'static str),
    #[error("missing field: {0}")]
    MissingField(&'static str),
    #[error("invalid value: {0}")]
    InvalidValue(String),
    #[error("serde error: {0}")]
    Serde(String),
}

impl From<serde_json::Error> for ConversionError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serde(err.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct EventConversion {
    pub data: UniversalEventData,
    pub agent_session_id: Option<String>,
}

impl EventConversion {
    pub fn new(data: UniversalEventData) -> Self {
        Self {
            data,
            agent_session_id: None,
        }
    }

    pub fn with_session(mut self, session_id: Option<String>) -> Self {
        self.agent_session_id = session_id;
        self
    }
}

fn message_from_text(role: &str, text: String) -> UniversalMessage {
    UniversalMessage::Parsed(UniversalMessageParsed {
        role: role.to_string(),
        id: None,
        metadata: Map::new(),
        parts: vec![UniversalMessagePart::Text { text }],
    })
}

fn message_from_parts(role: &str, parts: Vec<UniversalMessagePart>) -> UniversalMessage {
    UniversalMessage::Parsed(UniversalMessageParsed {
        role: role.to_string(),
        id: None,
        metadata: Map::new(),
        parts,
    })
}

fn text_only_from_parts(parts: &[UniversalMessagePart]) -> Result<String, ConversionError> {
    let mut text = String::new();
    for part in parts {
        match part {
            UniversalMessagePart::Text { text: part_text } => {
                if !text.is_empty() {
                    text.push_str("\n");
                }
                text.push_str(part_text);
            }
            UniversalMessagePart::ToolCall { .. } => {
                return Err(ConversionError::Unsupported("tool call part"))
            }
            UniversalMessagePart::ToolResult { .. } => {
                return Err(ConversionError::Unsupported("tool result part"))
            }
            UniversalMessagePart::FunctionCall { .. } => {
                return Err(ConversionError::Unsupported("function call part"))
            }
            UniversalMessagePart::FunctionResult { .. } => {
                return Err(ConversionError::Unsupported("function result part"))
            }
            UniversalMessagePart::File { .. } => {
                return Err(ConversionError::Unsupported("file part"))
            }
            UniversalMessagePart::Image { .. } => {
                return Err(ConversionError::Unsupported("image part"))
            }
            UniversalMessagePart::Error { .. } => {
                return Err(ConversionError::Unsupported("error part"))
            }
            UniversalMessagePart::Unknown { .. } => {
                return Err(ConversionError::Unsupported("unknown part"))
            }
        }
    }
    if text.is_empty() {
        Err(ConversionError::MissingField("text part"))
    } else {
        Ok(text)
    }
}

fn extract_message_from_value(value: &Value) -> Option<String> {
    if let Some(message) = value.get("message").and_then(Value::as_str) {
        return Some(message.to_string());
    }
    if let Some(message) = value.get("error").and_then(|v| v.get("message")).and_then(Value::as_str) {
        return Some(message.to_string());
    }
    if let Some(message) = value.get("data").and_then(|v| v.get("message")).and_then(Value::as_str) {
        return Some(message.to_string());
    }
    None
}




