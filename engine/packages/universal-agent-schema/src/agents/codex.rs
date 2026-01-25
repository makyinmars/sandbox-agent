use crate::{
    extract_message_from_value,
    text_only_from_parts,
    AttachmentSource,
    ConversionError,
    CrashInfo,
    EventConversion,
    Started,
    UniversalEventData,
    UniversalMessage,
    UniversalMessageParsed,
    UniversalMessagePart,
};
use crate::codex as schema;
use serde_json::{Map, Value};

pub fn event_to_universal(event: &schema::ThreadEvent) -> EventConversion {
    let schema::ThreadEvent {
        error,
        item,
        thread_id,
        type_,
    } = event;
    match type_ {
        schema::ThreadEventType::ThreadCreated | schema::ThreadEventType::ThreadUpdated => {
            let started = Started {
                message: Some(type_.to_string()),
                details: serde_json::to_value(event).ok(),
            };
            EventConversion::new(UniversalEventData::Started { started })
                .with_session(thread_id.clone())
        }
        schema::ThreadEventType::ItemCreated | schema::ThreadEventType::ItemUpdated => {
            if let Some(item) = item.as_ref() {
                let message = thread_item_to_message(item);
                EventConversion::new(UniversalEventData::Message { message })
                    .with_session(thread_id.clone())
            } else {
                EventConversion::new(UniversalEventData::Unknown {
                    raw: serde_json::to_value(event).unwrap_or(Value::Null),
                })
            }
        }
        schema::ThreadEventType::Error => {
            let message = extract_message_from_value(&Value::Object(error.clone()))
                .unwrap_or_else(|| "codex error".to_string());
            let crash = CrashInfo {
                message,
                kind: Some("error".to_string()),
                details: Some(Value::Object(error.clone())),
            };
            EventConversion::new(UniversalEventData::Error { error: crash })
                .with_session(thread_id.clone())
        }
    }
}

pub fn universal_event_to_codex(event: &UniversalEventData) -> Result<schema::ThreadEvent, ConversionError> {
    match event {
        UniversalEventData::Message { message } => {
            let parsed = match message {
                UniversalMessage::Parsed(parsed) => parsed,
                UniversalMessage::Unparsed { .. } => {
                    return Err(ConversionError::Unsupported("unparsed message"))
                }
            };
            let id = parsed.id.clone().ok_or(ConversionError::MissingField("message.id"))?;
            let content = text_only_from_parts(&parsed.parts)?;
            let role = match parsed.role.as_str() {
                "user" => Some(schema::ThreadItemRole::User),
                "assistant" => Some(schema::ThreadItemRole::Assistant),
                "system" => Some(schema::ThreadItemRole::System),
                _ => None,
            };
            let item = schema::ThreadItem {
                content: Some(schema::ThreadItemContent::Variant0(content)),
                id,
                role,
                status: None,
                type_: schema::ThreadItemType::Message,
            };
            Ok(schema::ThreadEvent {
                error: Map::new(),
                item: Some(item),
                thread_id: None,
                type_: schema::ThreadEventType::ItemCreated,
            })
        }
        _ => Err(ConversionError::Unsupported("codex event")),
    }
}

pub fn message_to_universal(message: &schema::Message) -> UniversalMessage {
    let schema::Message { role, content } = message;
    UniversalMessage::Parsed(UniversalMessageParsed {
        role: role.to_string(),
        id: None,
        metadata: Map::new(),
        parts: vec![UniversalMessagePart::Text {
            text: content.clone(),
        }],
    })
}

pub fn universal_message_to_message(
    message: &UniversalMessage,
) -> Result<schema::Message, ConversionError> {
    let parsed = match message {
        UniversalMessage::Parsed(parsed) => parsed,
        UniversalMessage::Unparsed { .. } => {
            return Err(ConversionError::Unsupported("unparsed message"))
        }
    };
    let content = text_only_from_parts(&parsed.parts)?;
    Ok(schema::Message {
        role: match parsed.role.as_str() {
            "user" => schema::MessageRole::User,
            "assistant" => schema::MessageRole::Assistant,
            "system" => schema::MessageRole::System,
            _ => schema::MessageRole::User,
        },
        content,
    })
}

pub fn inputs_to_universal_message(inputs: &[schema::Input], role: &str) -> UniversalMessage {
    let parts = inputs.iter().map(input_to_universal_part).collect();
    UniversalMessage::Parsed(UniversalMessageParsed {
        role: role.to_string(),
        id: None,
        metadata: Map::new(),
        parts,
    })
}

pub fn input_to_universal_part(input: &schema::Input) -> UniversalMessagePart {
    let schema::Input {
        content,
        mime_type,
        path,
        type_,
    } = input;
    let raw = serde_json::to_value(input).unwrap_or(Value::Null);
    match type_ {
        schema::InputType::Text => match content {
            Some(content) => UniversalMessagePart::Text {
                text: content.clone(),
            },
            None => UniversalMessagePart::Unknown { raw },
        },
        schema::InputType::File => {
            let source = if let Some(path) = path {
                AttachmentSource::Path { path: path.clone() }
            } else if let Some(content) = content {
                AttachmentSource::Data {
                    data: content.clone(),
                    encoding: None,
                }
            } else {
                return UniversalMessagePart::Unknown { raw };
            };
            UniversalMessagePart::File {
                source,
                mime_type: mime_type.clone(),
                filename: None,
                raw: Some(raw),
            }
        }
        schema::InputType::Image => {
            let source = if let Some(path) = path {
                AttachmentSource::Path { path: path.clone() }
            } else if let Some(content) = content {
                AttachmentSource::Data {
                    data: content.clone(),
                    encoding: None,
                }
            } else {
                return UniversalMessagePart::Unknown { raw };
            };
            UniversalMessagePart::Image {
                source,
                mime_type: mime_type.clone(),
                alt: None,
                raw: Some(raw),
            }
        }
    }
}

pub fn universal_message_to_inputs(
    message: &UniversalMessage,
) -> Result<Vec<schema::Input>, ConversionError> {
    let parsed = match message {
        UniversalMessage::Parsed(parsed) => parsed,
        UniversalMessage::Unparsed { .. } => {
            return Err(ConversionError::Unsupported("unparsed message"))
        }
    };
    universal_parts_to_inputs(&parsed.parts)
}

pub fn universal_parts_to_inputs(
    parts: &[UniversalMessagePart],
) -> Result<Vec<schema::Input>, ConversionError> {
    let mut inputs = Vec::new();
    for part in parts {
        match part {
            UniversalMessagePart::Text { text } => inputs.push(schema::Input {
                content: Some(text.clone()),
                mime_type: None,
                path: None,
                type_: schema::InputType::Text,
            }),
            UniversalMessagePart::File {
                source,
                mime_type,
                ..
            } => inputs.push(input_from_attachment(source, mime_type.as_ref(), schema::InputType::File)?),
            UniversalMessagePart::Image {
                source, mime_type, ..
            } => inputs.push(input_from_attachment(
                source,
                mime_type.as_ref(),
                schema::InputType::Image,
            )?),
            UniversalMessagePart::ToolCall { .. }
            | UniversalMessagePart::ToolResult { .. }
            | UniversalMessagePart::FunctionCall { .. }
            | UniversalMessagePart::FunctionResult { .. }
            | UniversalMessagePart::Error { .. }
            | UniversalMessagePart::Unknown { .. } => {
                return Err(ConversionError::Unsupported("unsupported part"))
            }
        }
    }
    if inputs.is_empty() {
        return Err(ConversionError::MissingField("parts"));
    }
    Ok(inputs)
}

fn input_from_attachment(
    source: &AttachmentSource,
    mime_type: Option<&String>,
    input_type: schema::InputType,
) -> Result<schema::Input, ConversionError> {
    match source {
        AttachmentSource::Path { path } => Ok(schema::Input {
            content: None,
            mime_type: mime_type.cloned(),
            path: Some(path.clone()),
            type_: input_type,
        }),
        AttachmentSource::Data { data, encoding } => {
            if let Some(encoding) = encoding.as_deref() {
                if encoding != "base64" {
                    return Err(ConversionError::Unsupported("codex data encoding"));
                }
            }
            Ok(schema::Input {
                content: Some(data.clone()),
                mime_type: mime_type.cloned(),
                path: None,
                type_: input_type,
            })
        }
        AttachmentSource::Url { .. } => Err(ConversionError::Unsupported("codex input url")),
    }
}

fn thread_item_to_message(item: &schema::ThreadItem) -> UniversalMessage {
    let schema::ThreadItem {
        content,
        id,
        role,
        status,
        type_,
    } = item;
    let mut metadata = Map::new();
    metadata.insert("itemType".to_string(), Value::String(type_.to_string()));
    if let Some(status) = status {
        metadata.insert("status".to_string(), Value::String(status.to_string()));
    }
    let role = role
        .as_ref()
        .map(|role| role.to_string())
        .unwrap_or_else(|| "assistant".to_string());
    let parts = match type_ {
        schema::ThreadItemType::Message => message_parts_from_codex_content(content),
        schema::ThreadItemType::FunctionCall => vec![function_call_part_from_codex(id, content)],
        schema::ThreadItemType::FunctionResult => vec![function_result_part_from_codex(id, content)],
    };
    UniversalMessage::Parsed(UniversalMessageParsed {
        role,
        id: Some(id.clone()),
        metadata,
        parts,
    })
}

fn message_parts_from_codex_content(
    content: &Option<schema::ThreadItemContent>,
) -> Vec<UniversalMessagePart> {
    match content {
        Some(schema::ThreadItemContent::Variant0(text)) => {
            vec![UniversalMessagePart::Text { text: text.clone() }]
        }
        Some(schema::ThreadItemContent::Variant1(raw)) => {
            vec![UniversalMessagePart::Unknown {
                raw: serde_json::to_value(raw).unwrap_or(Value::Null),
            }]
        }
        None => Vec::new(),
    }
}

fn function_call_part_from_codex(
    item_id: &str,
    content: &Option<schema::ThreadItemContent>,
) -> UniversalMessagePart {
    let raw = thread_item_content_to_value(content);
    let name = extract_object_field(&raw, "name");
    let arguments = extract_object_value(&raw, "arguments").unwrap_or_else(|| raw.clone());
    UniversalMessagePart::FunctionCall {
        id: Some(item_id.to_string()),
        name,
        arguments,
        raw: Some(raw),
    }
}

fn function_result_part_from_codex(
    item_id: &str,
    content: &Option<schema::ThreadItemContent>,
) -> UniversalMessagePart {
    let raw = thread_item_content_to_value(content);
    let name = extract_object_field(&raw, "name");
    let result = extract_object_value(&raw, "result")
        .or_else(|| extract_object_value(&raw, "output"))
        .or_else(|| extract_object_value(&raw, "content"))
        .unwrap_or_else(|| raw.clone());
    UniversalMessagePart::FunctionResult {
        id: Some(item_id.to_string()),
        name,
        result,
        is_error: None,
        raw: Some(raw),
    }
}

fn thread_item_content_to_value(content: &Option<schema::ThreadItemContent>) -> Value {
    match content {
        Some(schema::ThreadItemContent::Variant0(text)) => Value::String(text.clone()),
        Some(schema::ThreadItemContent::Variant1(raw)) => {
            Value::Array(raw.iter().cloned().map(Value::Object).collect())
        }
        None => Value::Null,
    }
}

fn extract_object_field(raw: &Value, field: &str) -> Option<String> {
    extract_object_value(raw, field)
        .and_then(|value| value.as_str().map(|s| s.to_string()))
}

fn extract_object_value(raw: &Value, field: &str) -> Option<Value> {
    match raw {
        Value::Object(map) => map.get(field).cloned(),
        Value::Array(values) => values
            .first()
            .and_then(|value| value.as_object())
            .and_then(|map| map.get(field).cloned()),
        _ => None,
    }
}
