// Source-level guards run on the host because the firmware requires Xtensa.
const FIRMWARE: &str = include_str!("../../nockster-fw/src/bin/main.rs");

#[test]
fn draft_dispatch_cannot_sign_without_confirmation() {
    let handler = FIRMWARE
        .split("fn handle_frame_v1(")
        .nth(1)
        .unwrap()
        .split("fn handle_request_v1(")
        .next()
        .unwrap();
    assert!(
        !handler.contains("sign_draft_v1("),
        "Draft dispatch must queue review, never sign directly"
    );

    let draft = handler.split("FragKind::SignDraft => {").nth(1).unwrap();
    let before_key = draft
        .split("signing::active_root_signer_config()")
        .next()
        .unwrap();
    assert!(
        before_key.contains("let Some(ui) = ui.as_mut() else {"),
        "A GUI is required before accessing the signing key"
    );
    let missing_ui = before_key
        .split("let Some(ui) = ui.as_mut() else {")
        .nth(1)
        .unwrap();
    assert!(missing_ui.contains("return Response::Err"));
    assert!(missing_ui.contains("code: ERR_UNSUPPORTED_VERSION"));
}
