use crate::{
    extract_message_from_value,
    AttachmentSource,
    ConversionError,
    CrashInfo,
    EventConversion,
    PermissionRequest,
    PermissionToolRef,
    QuestionInfo,
    QuestionOption,
    QuestionRequest,
    QuestionToolRef,
    Started,
    UniversalEventData,
    UniversalMessage,
    UniversalMessageParsed,
    UniversalMessagePart,
};
use crate::opencode as schema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub fn event_to_universal(event: &schema::Event) -> EventConversion {
    match event {
        schema::Event::MessageUpdated(updated) => {
            let schema::EventMessageUpdated { properties, type_: _ } = updated;
            let schema::EventMessageUpdatedProperties { info } = properties;
            let (message, session_id) = message_from_opencode(info);
            EventConversion::new(UniversalEventData::Message { message })
                .with_session(session_id)
        }
        schema::Event::MessagePartUpdated(updated) => {
            let schema::EventMessagePartUpdated { properties, type_: _ } = updated;
            let schema::EventMessagePartUpdatedProperties { part, delta } = properties;
            let (message, session_id) = part_to_message(part, delta.as_ref());
            EventConversion::new(UniversalEventData::Message { message })
                .with_session(session_id)
        }
        schema::Event::QuestionAsked(asked) => {
            let schema::EventQuestionAsked { properties, type_: _ } = asked;
            let question = question_request_from_opencode(properties);
            let session_id = question.session_id.clone();
            EventConversion::new(UniversalEventData::QuestionAsked { question_asked: question })
                .with_session(Some(session_id))
        }
        schema::Event::PermissionAsked(asked) => {
            let schema::EventPermissionAsked { properties, type_: _ } = asked;
            let permission = permission_request_from_opencode(properties);
            let session_id = permission.session_id.clone();
            EventConversion::new(UniversalEventData::PermissionAsked { permission_asked: permission })
                .with_session(Some(session_id))
        }
        schema::Event::SessionCreated(created) => {
            let schema::EventSessionCreated { properties, type_: _ } = created;
            let schema::EventSessionCreatedProperties { info } = properties;
            let details = serde_json::to_value(info).ok();
            let started = Started {
                message: Some("session.created".to_string()),
                details,
            };
            EventConversion::new(UniversalEventData::Started { started })
        }
        schema::Event::SessionError(error) => {
            let schema::EventSessionError { properties, type_: _ } = error;
            let schema::EventSessionErrorProperties {
                error: _error,
                session_id,
            } = properties;
            let message = extract_message_from_value(&serde_json::to_value(properties).unwrap_or(Value::Null))
                .unwrap_or_else(|| "opencode session error".to_string());
            let crash = CrashInfo {
                message,
                kind: Some("session.error".to_string()),
                details: serde_json::to_value(properties).ok(),
            };
            EventConversion::new(UniversalEventData::Error { error: crash })
                .with_session(session_id.clone())
        }
        _ => EventConversion::new(UniversalEventData::Unknown {
            raw: serde_json::to_value(event).unwrap_or(Value::Null),
        }),
    }
}

pub fn universal_event_to_opencode(event: &UniversalEventData) -> Result<schema::Event, ConversionError> {
    match event {
        UniversalEventData::QuestionAsked { question_asked } => {
            let properties = question_request_to_opencode(question_asked)?;
            Ok(schema::Event::QuestionAsked(schema::EventQuestionAsked {
                properties,
                type_: "question.asked".to_string(),
            }))
        }
        UniversalEventData::PermissionAsked { permission_asked } => {
            let properties = permission_request_to_opencode(permission_asked)?;
            Ok(schema::Event::PermissionAsked(schema::EventPermissionAsked {
                properties,
                type_: "permission.asked".to_string(),
            }))
        }
        _ => Err(ConversionError::Unsupported("opencode event")),
    }
}

pub fn universal_message_to_parts(
    message: &UniversalMessage,
) -> Result<Vec<schema::TextPartInput>, ConversionError> {
    let parsed = match message {
        UniversalMessage::Parsed(parsed) => parsed,
        UniversalMessage::Unparsed { .. } => {
            return Err(ConversionError::Unsupported("unparsed message"))
        }
    };
    let mut parts = Vec::new();
    for part in &parsed.parts {
        match part {
            UniversalMessagePart::Text { text } => {
                parts.push(text_part_input_from_text(text));
            }
            UniversalMessagePart::ToolCall { .. }
            | UniversalMessagePart::ToolResult { .. }
            | UniversalMessagePart::FunctionCall { .. }
            | UniversalMessagePart::FunctionResult { .. }
            | UniversalMessagePart::File { .. }
            | UniversalMessagePart::Image { .. }
            | UniversalMessagePart::Error { .. }
            | UniversalMessagePart::Unknown { .. } => {
                return Err(ConversionError::Unsupported("non-text part"))
            }
        }
    }
    if parts.is_empty() {
        return Err(ConversionError::MissingField("parts"));
    }
    Ok(parts)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpencodePartInput {
    Text(schema::TextPartInput),
    File(schema::FilePartInput),
}

pub fn universal_message_to_part_inputs(
    message: &UniversalMessage,
) -> Result<Vec<OpencodePartInput>, ConversionError> {
    let parsed = match message {
        UniversalMessage::Parsed(parsed) => parsed,
        UniversalMessage::Unparsed { .. } => {
            return Err(ConversionError::Unsupported("unparsed message"))
        }
    };
    universal_parts_to_part_inputs(&parsed.parts)
}

pub fn universal_parts_to_part_inputs(
    parts: &[UniversalMessagePart],
) -> Result<Vec<OpencodePartInput>, ConversionError> {
    let mut inputs = Vec::new();
    for part in parts {
        inputs.push(universal_part_to_opencode_input(part)?);
    }
    if inputs.is_empty() {
        return Err(ConversionError::MissingField("parts"));
    }
    Ok(inputs)
}

pub fn universal_part_to_opencode_input(
    part: &UniversalMessagePart,
) -> Result<OpencodePartInput, ConversionError> {
    match part {
        UniversalMessagePart::Text { text } => Ok(OpencodePartInput::Text(
            text_part_input_from_text(text),
        )),
        UniversalMessagePart::File {
            source,
            mime_type,
            filename,
            ..
        } => Ok(OpencodePartInput::File(file_part_input_from_universal(
            source,
            mime_type.as_deref(),
            filename.as_ref(),
        )?)),
        UniversalMessagePart::Image {
            source, mime_type, ..
        } => Ok(OpencodePartInput::File(file_part_input_from_universal(
            source,
            mime_type.as_deref(),
            None,
        )?)),
        UniversalMessagePart::ToolCall { .. }
        | UniversalMessagePart::ToolResult { .. }
        | UniversalMessagePart::FunctionCall { .. }
        | UniversalMessagePart::FunctionResult { .. }
        | UniversalMessagePart::Error { .. }
        | UniversalMessagePart::Unknown { .. } => {
            Err(ConversionError::Unsupported("unsupported part"))
        }
    }
}

fn text_part_input_from_text(text: &str) -> schema::TextPartInput {
    schema::TextPartInput {
        id: None,
        ignored: None,
        metadata: Map::new(),
        synthetic: None,
        text: text.to_string(),
        time: None,
        type_: "text".to_string(),
    }
}

pub fn text_part_input_to_universal(part: &schema::TextPartInput) -> UniversalMessage {
    let schema::TextPartInput {
        id,
        ignored,
        metadata,
        synthetic,
        text,
        time,
        type_,
    } = part;
    let mut metadata = metadata.clone();
    if let Some(id) = id {
        metadata.insert("partId".to_string(), Value::String(id.clone()));
    }
    if let Some(ignored) = ignored {
        metadata.insert("ignored".to_string(), Value::Bool(*ignored));
    }
    if let Some(synthetic) = synthetic {
        metadata.insert("synthetic".to_string(), Value::Bool(*synthetic));
    }
    if let Some(time) = time {
        metadata.insert(
            "time".to_string(),
            serde_json::to_value(time).unwrap_or(Value::Null),
        );
    }
    metadata.insert("type".to_string(), Value::String(type_.clone()));
    UniversalMessage::Parsed(UniversalMessageParsed {
        role: "user".to_string(),
        id: None,
        metadata,
        parts: vec![UniversalMessagePart::Text { text: text.clone() }],
    })
}

fn file_part_input_from_universal(
    source: &AttachmentSource,
    mime_type: Option<&str>,
    filename: Option<&String>,
) -> Result<schema::FilePartInput, ConversionError> {
    let mime = mime_type.ok_or(ConversionError::MissingField("mime_type"))?;
    let url = attachment_source_to_opencode_url(source, mime)?;
    Ok(schema::FilePartInput {
        filename: filename.cloned(),
        id: None,
        mime: mime.to_string(),
        source: None,
        type_: "file".to_string(),
        url,
    })
}

fn attachment_source_to_opencode_url(
    source: &AttachmentSource,
    mime_type: &str,
) -> Result<String, ConversionError> {
    match source {
        AttachmentSource::Url { url } => Ok(url.clone()),
        AttachmentSource::Path { path } => Ok(format!("file://{}", path)),
        AttachmentSource::Data { data, encoding } => {
            let encoding = encoding.as_deref().unwrap_or("base64");
            if encoding != "base64" {
                return Err(ConversionError::Unsupported("opencode data encoding"));
            }
            Ok(format!("data:{};base64,{}", mime_type, data))
        }
    }
}

fn message_from_opencode(message: &schema::Message) -> (UniversalMessage, Option<String>) {
    match message {
        schema::Message::UserMessage(user) => {
            let schema::UserMessage {
                agent,
                id,
                model,
                role,
                session_id,
                summary,
                system,
                time,
                tools,
                variant,
            } = user;
            let mut metadata = Map::new();
            metadata.insert("agent".to_string(), Value::String(agent.clone()));
            metadata.insert(
                "model".to_string(),
                serde_json::to_value(model).unwrap_or(Value::Null),
            );
            metadata.insert(
                "time".to_string(),
                serde_json::to_value(time).unwrap_or(Value::Null),
            );
            metadata.insert(
                "tools".to_string(),
                serde_json::to_value(tools).unwrap_or(Value::Null),
            );
            if let Some(summary) = summary {
                metadata.insert(
                    "summary".to_string(),
                    serde_json::to_value(summary).unwrap_or(Value::Null),
                );
            }
            if let Some(system) = system {
                metadata.insert("system".to_string(), Value::String(system.clone()));
            }
            if let Some(variant) = variant {
                metadata.insert("variant".to_string(), Value::String(variant.clone()));
            }
            let parsed = UniversalMessageParsed {
                role: role.clone(),
                id: Some(id.clone()),
                metadata,
                parts: Vec::new(),
            };
            (
                UniversalMessage::Parsed(parsed),
                Some(session_id.clone()),
            )
        }
        schema::Message::AssistantMessage(assistant) => {
            let schema::AssistantMessage {
                agent,
                cost,
                error,
                finish,
                id,
                mode,
                model_id,
                parent_id,
                path,
                provider_id,
                role,
                session_id,
                summary,
                time,
                tokens,
            } = assistant;
            let mut metadata = Map::new();
            metadata.insert("agent".to_string(), Value::String(agent.clone()));
            metadata.insert(
                "cost".to_string(),
                serde_json::to_value(cost).unwrap_or(Value::Null),
            );
            metadata.insert("mode".to_string(), Value::String(mode.clone()));
            metadata.insert("modelId".to_string(), Value::String(model_id.clone()));
            metadata.insert("providerId".to_string(), Value::String(provider_id.clone()));
            metadata.insert("parentId".to_string(), Value::String(parent_id.clone()));
            metadata.insert(
                "path".to_string(),
                serde_json::to_value(path).unwrap_or(Value::Null),
            );
            metadata.insert(
                "tokens".to_string(),
                serde_json::to_value(tokens).unwrap_or(Value::Null),
            );
            metadata.insert(
                "time".to_string(),
                serde_json::to_value(time).unwrap_or(Value::Null),
            );
            if let Some(error) = error {
                metadata.insert(
                    "error".to_string(),
                    serde_json::to_value(error).unwrap_or(Value::Null),
                );
            }
            if let Some(finish) = finish {
                metadata.insert("finish".to_string(), Value::String(finish.clone()));
            }
            if let Some(summary) = summary {
                metadata.insert(
                    "summary".to_string(),
                    serde_json::to_value(summary).unwrap_or(Value::Null),
                );
            }
            let parsed = UniversalMessageParsed {
                role: role.clone(),
                id: Some(id.clone()),
                metadata,
                parts: Vec::new(),
            };
            (
                UniversalMessage::Parsed(parsed),
                Some(session_id.clone()),
            )
        }
    }
}

fn part_to_message(part: &schema::Part, delta: Option<&String>) -> (UniversalMessage, Option<String>) {
    match part {
        schema::Part::Variant0(text_part) => {
            let schema::TextPart {
                id,
                ignored,
                message_id,
                metadata,
                session_id,
                synthetic,
                text,
                time,
                type_,
            } = text_part;
            let mut part_metadata = base_part_metadata(message_id, id, delta);
            part_metadata.insert("type".to_string(), Value::String(type_.clone()));
            if let Some(ignored) = ignored {
                part_metadata.insert("ignored".to_string(), Value::Bool(*ignored));
            }
            if let Some(synthetic) = synthetic {
                part_metadata.insert("synthetic".to_string(), Value::Bool(*synthetic));
            }
            if let Some(time) = time {
                part_metadata.insert(
                    "time".to_string(),
                    serde_json::to_value(time).unwrap_or(Value::Null),
                );
            }
            if !metadata.is_empty() {
                part_metadata.insert(
                    "partMetadata".to_string(),
                    Value::Object(metadata.clone()),
                );
            }
            let parsed = UniversalMessageParsed {
                role: "assistant".to_string(),
                id: Some(message_id.clone()),
                metadata: part_metadata,
                parts: vec![UniversalMessagePart::Text { text: text.clone() }],
            };
            (UniversalMessage::Parsed(parsed), Some(session_id.clone()))
        }
        schema::Part::Variant1 {
            agent: _agent,
            command: _command,
            description: _description,
            id,
            message_id,
            model: _model,
            prompt: _prompt,
            session_id,
            type_: _type,
        } => unknown_part_message(message_id, id, session_id, serde_json::to_value(part).unwrap_or(Value::Null), delta),
        schema::Part::Variant2(reasoning_part) => {
            let schema::ReasoningPart {
                id,
                message_id,
                metadata: _metadata,
                session_id,
                text: _text,
                time: _time,
                type_: _type,
            } = reasoning_part;
            unknown_part_message(
                message_id,
                id,
                session_id,
                serde_json::to_value(reasoning_part).unwrap_or(Value::Null),
                delta,
            )
        }
        schema::Part::Variant3(file_part) => {
            let schema::FilePart {
                filename: _filename,
                id,
                message_id,
                mime: _mime,
                session_id,
                source: _source,
                type_: _type,
                url: _url,
            } = file_part;
            let part_metadata = base_part_metadata(message_id, id, delta);
            let part = file_part_to_universal_part(file_part);
            let parsed = UniversalMessageParsed {
                role: "assistant".to_string(),
                id: Some(message_id.clone()),
                metadata: part_metadata,
                parts: vec![part],
            };
            (UniversalMessage::Parsed(parsed), Some(session_id.clone()))
        }
        schema::Part::Variant4(tool_part) => {
            let schema::ToolPart {
                call_id,
                id,
                message_id,
                metadata,
                session_id,
                state,
                tool,
                type_,
            } = tool_part;
            let mut part_metadata = base_part_metadata(message_id, id, delta);
            part_metadata.insert("type".to_string(), Value::String(type_.clone()));
            part_metadata.insert("callId".to_string(), Value::String(call_id.clone()));
            part_metadata.insert("tool".to_string(), Value::String(tool.clone()));
            if !metadata.is_empty() {
                part_metadata.insert(
                    "partMetadata".to_string(),
                    Value::Object(metadata.clone()),
                );
            }
            let (mut parts, state_meta) = tool_state_to_parts(call_id, tool, state);
            if let Some(state_meta) = state_meta {
                part_metadata.insert("toolState".to_string(), state_meta);
            }
            let parsed = UniversalMessageParsed {
                role: "assistant".to_string(),
                id: Some(message_id.clone()),
                metadata: part_metadata,
                parts: parts.drain(..).collect(),
            };
            (UniversalMessage::Parsed(parsed), Some(session_id.clone()))
        }
        schema::Part::Variant5(step_start) => {
            let schema::StepStartPart {
                id,
                message_id,
                session_id,
                snapshot: _snapshot,
                type_: _type,
            } = step_start;
            unknown_part_message(
                message_id,
                id,
                session_id,
                serde_json::to_value(step_start).unwrap_or(Value::Null),
                delta,
            )
        }
        schema::Part::Variant6(step_finish) => {
            let schema::StepFinishPart {
                cost: _cost,
                id,
                message_id,
                reason: _reason,
                session_id,
                snapshot: _snapshot,
                tokens: _tokens,
                type_: _type,
            } = step_finish;
            unknown_part_message(
                message_id,
                id,
                session_id,
                serde_json::to_value(step_finish).unwrap_or(Value::Null),
                delta,
            )
        }
        schema::Part::Variant7(snapshot_part) => {
            let schema::SnapshotPart {
                id,
                message_id,
                session_id,
                snapshot: _snapshot,
                type_: _type,
            } = snapshot_part;
            unknown_part_message(
                message_id,
                id,
                session_id,
                serde_json::to_value(snapshot_part).unwrap_or(Value::Null),
                delta,
            )
        }
        schema::Part::Variant8(patch_part) => {
            let schema::PatchPart {
                files: _files,
                hash: _hash,
                id,
                message_id,
                session_id,
                type_: _type,
            } = patch_part;
            unknown_part_message(
                message_id,
                id,
                session_id,
                serde_json::to_value(patch_part).unwrap_or(Value::Null),
                delta,
            )
        }
        schema::Part::Variant9(agent_part) => {
            let schema::AgentPart {
                id,
                message_id,
                name: _name,
                session_id,
                source: _source,
                type_: _type,
            } = agent_part;
            unknown_part_message(
                message_id,
                id,
                session_id,
                serde_json::to_value(agent_part).unwrap_or(Value::Null),
                delta,
            )
        }
        schema::Part::Variant10(retry_part) => {
            let schema::RetryPart {
                attempt: _attempt,
                error: _error,
                id,
                message_id,
                session_id,
                time: _time,
                type_: _type,
            } = retry_part;
            unknown_part_message(
                message_id,
                id,
                session_id,
                serde_json::to_value(retry_part).unwrap_or(Value::Null),
                delta,
            )
        }
        schema::Part::Variant11(compaction_part) => {
            let schema::CompactionPart {
                auto: _auto,
                id,
                message_id,
                session_id,
                type_: _type,
            } = compaction_part;
            unknown_part_message(
                message_id,
                id,
                session_id,
                serde_json::to_value(compaction_part).unwrap_or(Value::Null),
                delta,
            )
        }
    }
}

fn base_part_metadata(message_id: &str, part_id: &str, delta: Option<&String>) -> Map<String, Value> {
    let mut metadata = Map::new();
    metadata.insert("messageId".to_string(), Value::String(message_id.to_string()));
    metadata.insert("partId".to_string(), Value::String(part_id.to_string()));
    if let Some(delta) = delta {
        metadata.insert("delta".to_string(), Value::String(delta.clone()));
    }
    metadata
}

fn unknown_part_message(
    message_id: &str,
    part_id: &str,
    session_id: &str,
    raw: Value,
    delta: Option<&String>,
) -> (UniversalMessage, Option<String>) {
    let metadata = base_part_metadata(message_id, part_id, delta);
    let parsed = UniversalMessageParsed {
        role: "assistant".to_string(),
        id: Some(message_id.to_string()),
        metadata,
        parts: vec![UniversalMessagePart::Unknown { raw }],
    };
    (UniversalMessage::Parsed(parsed), Some(session_id.to_string()))
}

fn file_part_to_universal_part(file_part: &schema::FilePart) -> UniversalMessagePart {
    let schema::FilePart {
        filename,
        id: _id,
        message_id: _message_id,
        mime,
        session_id: _session_id,
        source: _source,
        type_: _type,
        url,
    } = file_part;
    let raw = serde_json::to_value(file_part).unwrap_or(Value::Null);
    let source = AttachmentSource::Url { url: url.clone() };
    if mime.starts_with("image/") {
        UniversalMessagePart::Image {
            source,
            mime_type: Some(mime.clone()),
            alt: filename.clone(),
            raw: Some(raw),
        }
    } else {
        UniversalMessagePart::File {
            source,
            mime_type: Some(mime.clone()),
            filename: filename.clone(),
            raw: Some(raw),
        }
    }
}

fn tool_state_to_parts(
    call_id: &str,
    tool: &str,
    state: &schema::ToolState,
) -> (Vec<UniversalMessagePart>, Option<Value>) {
    match state {
        schema::ToolState::Pending(state) => {
            let schema::ToolStatePending { input, raw, status } = state;
            let mut meta = Map::new();
            meta.insert("status".to_string(), Value::String(status.clone()));
            meta.insert("raw".to_string(), Value::String(raw.clone()));
            meta.insert("input".to_string(), Value::Object(input.clone()));
            (
                vec![UniversalMessagePart::ToolCall {
                    id: Some(call_id.to_string()),
                    name: tool.to_string(),
                    input: Value::Object(input.clone()),
                }],
                Some(Value::Object(meta)),
            )
        }
        schema::ToolState::Running(state) => {
            let schema::ToolStateRunning {
                input,
                metadata,
                status,
                time,
                title,
            } = state;
            let mut meta = Map::new();
            meta.insert("status".to_string(), Value::String(status.clone()));
            meta.insert("input".to_string(), Value::Object(input.clone()));
            meta.insert("metadata".to_string(), Value::Object(metadata.clone()));
            meta.insert(
                "time".to_string(),
                serde_json::to_value(time).unwrap_or(Value::Null),
            );
            if let Some(title) = title {
                meta.insert("title".to_string(), Value::String(title.clone()));
            }
            (
                vec![UniversalMessagePart::ToolCall {
                    id: Some(call_id.to_string()),
                    name: tool.to_string(),
                    input: Value::Object(input.clone()),
                }],
                Some(Value::Object(meta)),
            )
        }
        schema::ToolState::Completed(state) => {
            let schema::ToolStateCompleted {
                attachments,
                input,
                metadata,
                output,
                status,
                time,
                title,
            } = state;
            let mut meta = Map::new();
            meta.insert("status".to_string(), Value::String(status.clone()));
            meta.insert("input".to_string(), Value::Object(input.clone()));
            meta.insert("metadata".to_string(), Value::Object(metadata.clone()));
            meta.insert(
                "time".to_string(),
                serde_json::to_value(time).unwrap_or(Value::Null),
            );
            meta.insert("title".to_string(), Value::String(title.clone()));
            if !attachments.is_empty() {
                meta.insert(
                    "attachments".to_string(),
                    serde_json::to_value(attachments).unwrap_or(Value::Null),
                );
            }
            let mut parts = vec![UniversalMessagePart::ToolResult {
                id: Some(call_id.to_string()),
                name: Some(tool.to_string()),
                output: Value::String(output.clone()),
                is_error: Some(false),
            }];
            for attachment in attachments {
                parts.push(file_part_to_universal_part(attachment));
            }
            (parts, Some(Value::Object(meta)))
        }
        schema::ToolState::Error(state) => {
            let schema::ToolStateError {
                error,
                input,
                metadata,
                status,
                time,
            } = state;
            let mut meta = Map::new();
            meta.insert("status".to_string(), Value::String(status.clone()));
            meta.insert("error".to_string(), Value::String(error.clone()));
            meta.insert("input".to_string(), Value::Object(input.clone()));
            meta.insert("metadata".to_string(), Value::Object(metadata.clone()));
            meta.insert(
                "time".to_string(),
                serde_json::to_value(time).unwrap_or(Value::Null),
            );
            (
                vec![UniversalMessagePart::ToolResult {
                    id: Some(call_id.to_string()),
                    name: Some(tool.to_string()),
                    output: Value::String(error.clone()),
                    is_error: Some(true),
                }],
                Some(Value::Object(meta)),
            )
        }
    }
}

fn question_request_from_opencode(request: &schema::QuestionRequest) -> QuestionRequest {
    let schema::QuestionRequest {
        id,
        questions,
        session_id,
        tool,
    } = request;
    QuestionRequest {
        id: id.clone().into(),
        session_id: session_id.clone().into(),
        questions: questions
            .iter()
            .map(|question| {
                let schema::QuestionInfo {
                    custom,
                    header,
                    multiple,
                    options,
                    question,
                } = question;
                QuestionInfo {
                    question: question.clone(),
                    header: Some(header.clone()),
                    options: options
                        .iter()
                        .map(|opt| {
                            let schema::QuestionOption { description, label } = opt;
                            QuestionOption {
                                label: label.clone(),
                                description: Some(description.clone()),
                            }
                        })
                        .collect(),
                    multi_select: *multiple,
                    custom: *custom,
                }
            })
            .collect(),
        tool: tool.as_ref().map(|tool| {
            let schema::QuestionRequestTool { message_id, call_id } = tool;
            QuestionToolRef {
                message_id: message_id.clone(),
                call_id: call_id.clone(),
            }
        }),
    }
}

fn permission_request_from_opencode(request: &schema::PermissionRequest) -> PermissionRequest {
    let schema::PermissionRequest {
        always,
        id,
        metadata,
        patterns,
        permission,
        session_id,
        tool,
    } = request;
    PermissionRequest {
        id: id.clone().into(),
        session_id: session_id.clone().into(),
        permission: permission.clone(),
        patterns: patterns.clone(),
        metadata: metadata.clone(),
        always: always.clone(),
        tool: tool.as_ref().map(|tool| {
            let schema::PermissionRequestTool { message_id, call_id } = tool;
            PermissionToolRef {
                message_id: message_id.clone(),
                call_id: call_id.clone(),
            }
        }),
    }
}

fn question_request_to_opencode(request: &QuestionRequest) -> Result<schema::QuestionRequest, ConversionError> {
    let id = schema::QuestionRequestId::try_from(request.id.as_str())
        .map_err(|err| ConversionError::InvalidValue(err.to_string()))?;
    let session_id = schema::QuestionRequestSessionId::try_from(request.session_id.as_str())
        .map_err(|err| ConversionError::InvalidValue(err.to_string()))?;
    let questions = request
        .questions
        .iter()
        .map(|question| schema::QuestionInfo {
            question: question.question.clone(),
            header: question
                .header
                .clone()
                .unwrap_or_else(|| "Question".to_string()),
            options: question
                .options
                .iter()
                .map(|opt| schema::QuestionOption {
                    label: opt.label.clone(),
                    description: opt.description.clone().unwrap_or_default(),
                })
                .collect(),
            multiple: question.multi_select,
            custom: question.custom,
        })
        .collect();

    Ok(schema::QuestionRequest {
        id,
        session_id,
        questions,
        tool: request.tool.as_ref().map(|tool| schema::QuestionRequestTool {
            message_id: tool.message_id.clone(),
            call_id: tool.call_id.clone(),
        }),
    })
}

fn permission_request_to_opencode(
    request: &PermissionRequest,
) -> Result<schema::PermissionRequest, ConversionError> {
    let id = schema::PermissionRequestId::try_from(request.id.as_str())
        .map_err(|err| ConversionError::InvalidValue(err.to_string()))?;
    let session_id = schema::PermissionRequestSessionId::try_from(request.session_id.as_str())
        .map_err(|err| ConversionError::InvalidValue(err.to_string()))?;
    Ok(schema::PermissionRequest {
        id,
        session_id,
        permission: request.permission.clone(),
        patterns: request.patterns.clone(),
        metadata: request.metadata.clone(),
        always: request.always.clone(),
        tool: request.tool.as_ref().map(|tool| schema::PermissionRequestTool {
            message_id: tool.message_id.clone(),
            call_id: tool.call_id.clone(),
        }),
    })
}
