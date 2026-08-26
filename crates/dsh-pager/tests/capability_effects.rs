use dsh_pager::{fetch_attachment, list_file_references, RpcTransport};
use dsh_pager_test_support::NodeStdioMock;

fn scripted_transport(response: &str) -> (RpcTransport, NodeStdioMock) {
    let mock = NodeStdioMock::echo_line(response).expect("write node protocol mock");
    let transport = RpcTransport::spawn(mock.program(), &[mock.script_arg()])
        .expect("spawn node protocol mock");
    (transport, mock)
}

#[test]
fn file_reference_results_keep_path_and_kind() {
    let (mut transport, _mock) = scripted_transport(
        r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true,"value":{"items":[{"path":"src/main.rs","kind":"file"},{"path":"src","kind":"directory"}]}}}"#,
    );
    let value =
        list_file_references(&mut transport, "session-1", "src").expect("file reference response");
    assert_eq!(value.items.len(), 2);
    assert_eq!(value.items[0].path, "src/main.rs");
    assert_eq!(value.items[1].kind, "directory");
}

#[test]
fn attachment_preview_parses_authoritative_metadata_and_data() {
    let (mut transport, _mock) = scripted_transport(
        r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true,"value":{"attachment":{"attachmentId":"img-1","mediaType":"image/png","bytes":5,"width":4,"height":3,"name":"plot"},"data":"aGVsbG8="}}}"#,
    );
    let preview =
        fetch_attachment(&mut transport, "session-1", "img-1").expect("attachment response");
    assert_eq!(preview.attachment_id, "img-1");
    assert_eq!(preview.media_type, "image/png");
    assert_eq!(preview.bytes, Some(5));
    assert_eq!(preview.width, Some(4));
    assert_eq!(preview.height, Some(3));
    assert_eq!(preview.data, "aGVsbG8=");
}
