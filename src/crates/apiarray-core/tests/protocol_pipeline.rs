use apiarray_core::adapter::{
    AdapterKind, HeaderValue, StreamProtocol, build_transport_plan, parse_response,
};
use apiarray_core::canonical::{
    CanonicalRequest, ContentPart, FinishReason, Message, ResponseFormat, Role, ToolChoice,
};
use apiarray_core::provider::ProviderManifest;
use apiarray_core::secret::SecretRef;
use apiarray_core::stream::{StreamEvent, decode_stream_chunks};
use apiarray_core::templates::{TemplateContext, generate_templates};
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn provider_to_plan_to_response_to_templates_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    let provider = ProviderManifest::from_yaml(include_str!("../../../providers/openai.yaml"))?;
    let request = CanonicalRequest {
        schema_version: 1,
        model: "smart".to_owned(),
        messages: vec![Message::text(Role::User, "Hello")],
        max_output_tokens: 256,
        temperature: Some(0.2),
        stream: true,
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
        metadata: BTreeMap::new(),
    };
    let refs = BTreeMap::from([(
        "api_key".to_owned(),
        SecretRef::parse("secret://workspace/openai/api-key")?,
    )]);

    let plan = build_transport_plan(&provider, &refs, &request)?;
    assert_eq!(plan.adapter, AdapterKind::OpenaiCompatible);
    assert_eq!(plan.stream_protocol, StreamProtocol::OpenaiSse);
    assert!(plan.headers.iter().any(|header| matches!(
        &header.value,
        HeaderValue::SecretRef { reference, .. }
            if reference.as_str() == "secret://workspace/openai/api-key"
    )));

    let response = parse_response(
        AdapterKind::OpenaiCompatible,
        &json!({
            "id": "response-1",
            "model": "smart",
            "choices": [{"message": {"content": "Hello"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        }),
    )?;
    assert_eq!(response.finish_reason, FinishReason::Stop);
    assert!(matches!(&response.content[0], ContentPart::Text { text } if text == "Hello"));

    let stream = decode_stream_chunks(
        AdapterKind::OpenaiCompatible,
        &["data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".to_owned()],
    )?;
    assert!(
        stream
            .iter()
            .any(|event| matches!(event, StreamEvent::TextDelta { text } if text == "Hello"))
    );

    let templates = generate_templates(&TemplateContext {
        base_url: "http://127.0.0.1:6188/v1".to_owned(),
        model: "smart".to_owned(),
        stream: false,
        token_placeholder: "<API_ARRAY_TOKEN>".to_owned(),
    })?;
    assert_eq!(templates.len(), 8);
    Ok(())
}
