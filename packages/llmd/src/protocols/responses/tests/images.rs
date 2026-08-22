use super::*;

#[test]
fn standard_responses_encodes_base64_image_input() {
    let mut request = crate::protocols::tests_support::semantic_request();
    let ConversationItemKind::User { content } = &mut request.conversation.items[0].kind else {
        panic!("fixture starts with user input");
    };
    *content = piko_protocol::MessageContent::Blocks(vec![
        piko_protocol::ContentBlock::Text {
            text: "inspect".into(),
        },
        piko_protocol::ContentBlock::Image {
            data: "AA==".into(),
            mime_type: "image/png".into(),
        },
    ]);
    let target = target();
    target.validate(&request).unwrap();
    let plan = plan(&target, &request.conversation).unwrap();
    let body = ResponsesAdapter
        .encode(&request, &target, &plan, true)
        .unwrap();
    assert_eq!(body["input"][0]["content"][0]["type"], "input_text");
    assert_eq!(body["input"][0]["content"][1]["type"], "input_image");
    assert_eq!(
        body["input"][0]["content"][1]["image_url"],
        "data:image/png;base64,AA=="
    );
}
