use crate::{
    message_from_parts,
    message_from_text,
    text_only_from_parts,
    ConversionError,
    CrashInfo,
    EventConversion,
    UniversalEventData,
    UniversalMessage,
    UniversalMessageParsed,
    UniversalMessagePart,
};
use crate::amp as schema;
use serde_json::{Map, Value};

pub fn event_to_universal(event: &schema::StreamJsonMessage) -> EventConversion {
    let schema::StreamJsonMessage {
        content,
        error,
        id,
        tool_call,
        type_,
    } = event;
    match type_ {
        schema::StreamJsonMessageType::Message => {
            let text = content.clone().unwrap_or_default();
            let mut message = message_from_text("assistant", text);
            if let UniversalMessage::Parsed(parsed) = &mut message {
                parsed.id = id.clone();
            }
            EventConversion::new(UniversalEventData::Message { message })
        }
        schema::StreamJsonMessageType::ToolCall => {
            let tool_call = tool_call.as_ref();
            let part = if let Some(tool_call) = tool_call {
                let schema::ToolCall { arguments, id, name } = tool_call;
                let input = match arguments {
                    schema::ToolCallArguments::Variant0(text) => Value::String(text.clone()),
                    schema::ToolCallArguments::Variant1(map) => Value::Object(map.clone()),
                };
                UniversalMessagePart::ToolCall {
                    id: Some(id.clone()),
                    name: name.clone(),
                    input,
                }
            } else {
                UniversalMessagePart::Unknown { raw: Value::Null }
            };
            let mut message = message_from_parts("assistant", vec![part]);
            if let UniversalMessage::Parsed(parsed) = &mut message {
                parsed.id = id.clone();
            }
            EventConversion::new(UniversalEventData::Message { message })
        }
        schema::StreamJsonMessageType::ToolResult => {
            let output = content
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null);
            let part = UniversalMessagePart::ToolResult {
                id: id.clone(),
                name: None,
                output,
                is_error: None,
            };
            let message = message_from_parts("tool", vec![part]);
            EventConversion::new(UniversalEventData::Message { message })
        }
        schema::StreamJsonMessageType::Error => {
            let message = error.clone().unwrap_or_else(|| "amp error".to_string());
            let crash = CrashInfo {
                message,
                kind: Some("amp".to_string()),
                details: serde_json::to_value(event).ok(),
            };
            EventConversion::new(UniversalEventData::Error { error: crash })
        }
        schema::StreamJsonMessageType::Done => EventConversion::new(UniversalEventData::Unknown {
            raw: serde_json::to_value(event).unwrap_or(Value::Null),
        }),
    }
}

pub fn universal_event_to_amp(event: &UniversalEventData) -> Result<schema::StreamJsonMessage, ConversionError> {
    match event {
        UniversalEventData::Message { message } => {
            let parsed = match message {
                UniversalMessage::Parsed(parsed) => parsed,
                UniversalMessage::Unparsed { .. } => {
                    return Err(ConversionError::Unsupported("unparsed message"))
                }
            };
            let content = text_only_from_parts(&parsed.parts)?;
            Ok(schema::StreamJsonMessage {
                content: Some(content),
                error: None,
                id: parsed.id.clone(),
                tool_call: None,
                type_: schema::StreamJsonMessageType::Message,
            })
        }
        _ => Err(ConversionError::Unsupported("amp event")),
    }
}

pub fn message_to_universal(message: &schema::Message) -> UniversalMessage {
    let schema::Message {
        role,
        content,
        tool_calls,
    } = message;
    let mut parts = vec![UniversalMessagePart::Text {
        text: content.clone(),
    }];
    for call in tool_calls {
        let schema::ToolCall { arguments, id, name } = call;
        let input = match arguments {
            schema::ToolCallArguments::Variant0(text) => Value::String(text.clone()),
            schema::ToolCallArguments::Variant1(map) => Value::Object(map.clone()),
        };
        parts.push(UniversalMessagePart::ToolCall {
            id: Some(id.clone()),
            name: name.clone(),
            input,
        });
    }
    UniversalMessage::Parsed(UniversalMessageParsed {
        role: role.to_string(),
        id: None,
        metadata: Map::new(),
        parts,
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
        tool_calls: vec![],
    })
}
