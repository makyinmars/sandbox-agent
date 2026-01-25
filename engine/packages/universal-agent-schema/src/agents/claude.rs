use crate::{
    message_from_parts,
    message_from_text,
    text_only_from_parts,
    ConversionError,
    EventConversion,
    QuestionInfo,
    QuestionOption,
    QuestionRequest,
    UniversalEventData,
    UniversalMessage,
    UniversalMessageParsed,
    UniversalMessagePart,
};
use serde_json::{Map, Value};

pub fn event_to_universal_with_session(
    event: &Value,
    session_id: String,
) -> EventConversion {
    let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");
    match event_type {
        "assistant" => assistant_event_to_universal(event),
        "tool_use" => tool_use_event_to_universal(event, session_id),
        "tool_result" => tool_result_event_to_universal(event),
        "result" => result_event_to_universal(event),
        _ => EventConversion::new(UniversalEventData::Unknown { raw: event.clone() }),
    }
}

pub fn universal_event_to_claude(event: &UniversalEventData) -> Result<Value, ConversionError> {
    match event {
        UniversalEventData::Message { message } => {
            let parsed = match message {
                UniversalMessage::Parsed(parsed) => parsed,
                UniversalMessage::Unparsed { .. } => {
                    return Err(ConversionError::Unsupported("unparsed message"))
                }
            };
            let text = text_only_from_parts(&parsed.parts)?;
            Ok(Value::Object(Map::from_iter([
                ("type".to_string(), Value::String("assistant".to_string())),
                (
                    "message".to_string(),
                    Value::Object(Map::from_iter([(
                        "content".to_string(),
                        Value::Array(vec![Value::Object(Map::from_iter([(
                            "type".to_string(),
                            Value::String("text".to_string()),
                        ), (
                            "text".to_string(),
                            Value::String(text),
                        )]))]),
                    )])),
                ),
            ])))
        }
        _ => Err(ConversionError::Unsupported("claude event")),
    }
}

pub fn prompt_to_universal(prompt: &str) -> UniversalMessage {
    message_from_text("user", prompt.to_string())
}

pub fn universal_message_to_prompt(message: &UniversalMessage) -> Result<String, ConversionError> {
    let parsed = match message {
        UniversalMessage::Parsed(parsed) => parsed,
        UniversalMessage::Unparsed { .. } => {
            return Err(ConversionError::Unsupported("unparsed message"))
        }
    };
    text_only_from_parts(&parsed.parts)
}

fn assistant_event_to_universal(event: &Value) -> EventConversion {
    let content = event
        .get("message")
        .and_then(|msg| msg.get("content"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut parts = Vec::new();
    for block in content {
        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
        match block_type {
            "text" => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    parts.push(UniversalMessagePart::Text {
                        text: text.to_string(),
                    });
                }
            }
            "tool_use" => {
                if let Some(name) = block.get("name").and_then(Value::as_str) {
                    let input = block.get("input").cloned().unwrap_or(Value::Null);
                    let id = block.get("id").and_then(Value::as_str).map(|s| s.to_string());
                    parts.push(UniversalMessagePart::ToolCall {
                        id,
                        name: name.to_string(),
                        input,
                    });
                }
            }
            _ => parts.push(UniversalMessagePart::Unknown { raw: block }),
        }
    }
    let message = UniversalMessage::Parsed(UniversalMessageParsed {
        role: "assistant".to_string(),
        id: None,
        metadata: Map::new(),
        parts,
    });
    EventConversion::new(UniversalEventData::Message { message })
}

fn tool_use_event_to_universal(event: &Value, session_id: String) -> EventConversion {
    let tool_use = event.get("tool_use");
    let name = tool_use
        .and_then(|tool| tool.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let input = tool_use
        .and_then(|tool| tool.get("input"))
        .cloned()
        .unwrap_or(Value::Null);
    let id = tool_use
        .and_then(|tool| tool.get("id"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    if name == "AskUserQuestion" {
        if let Some(question) =
            question_from_claude_input(&input, id.clone(), session_id.clone())
        {
            return EventConversion::new(UniversalEventData::QuestionAsked {
                question_asked: question,
            });
        }
    }

    let message = message_from_parts(
        "assistant",
        vec![UniversalMessagePart::ToolCall {
            id,
            name: name.to_string(),
            input,
        }],
    );
    EventConversion::new(UniversalEventData::Message { message })
}

fn tool_result_event_to_universal(event: &Value) -> EventConversion {
    let tool_result = event.get("tool_result");
    let output = tool_result
        .and_then(|tool| tool.get("content"))
        .cloned()
        .unwrap_or(Value::Null);
    let is_error = tool_result
        .and_then(|tool| tool.get("is_error"))
        .and_then(Value::as_bool);
    let id = tool_result
        .and_then(|tool| tool.get("id"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    let message = message_from_parts(
        "tool",
        vec![UniversalMessagePart::ToolResult {
            id,
            name: None,
            output,
            is_error,
        }],
    );
    EventConversion::new(UniversalEventData::Message { message })
}

fn result_event_to_universal(event: &Value) -> EventConversion {
    let result_text = event
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let session_id = event
        .get("session_id")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let message = message_from_text("assistant", result_text);
    EventConversion::new(UniversalEventData::Message { message }).with_session(session_id)
}

fn question_from_claude_input(
    input: &Value,
    tool_id: Option<String>,
    session_id: String,
) -> Option<QuestionRequest> {
    let questions = input.get("questions").and_then(Value::as_array)?;
    let mut parsed_questions = Vec::new();
    for question in questions {
        let question_text = question.get("question")?.as_str()?.to_string();
        let header = question
            .get("header")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let multi_select = question
            .get("multiSelect")
            .and_then(Value::as_bool);
        let options = question
            .get("options")
            .and_then(Value::as_array)
            .map(|options| {
                options
                    .iter()
                    .filter_map(|option| {
                        let label = option.get("label")?.as_str()?.to_string();
                        let description = option
                            .get("description")
                            .and_then(Value::as_str)
                            .map(|s| s.to_string());
                        Some(QuestionOption { label, description })
                    })
                    .collect::<Vec<_>>()
            })?;
        parsed_questions.push(QuestionInfo {
            question: question_text,
            header,
            options,
            multi_select,
            custom: None,
        });
    }
    Some(QuestionRequest {
        id: tool_id.unwrap_or_else(|| "claude-question".to_string()),
        session_id,
        questions: parsed_questions,
        tool: None,
    })
}
