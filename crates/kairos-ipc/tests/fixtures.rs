use std::path::PathBuf;

use kairos_ipc::*;
use serde_json::Value;

fn fixtures() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../protocol/fixtures");
    let mut out: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(&p).unwrap(),
            )
        })
        .collect();
    out.sort();
    assert!(out.len() >= 18);
    out
}

#[test]
fn fixtures_round_trip() {
    for (name, text) in fixtures() {
        let original: Value = serde_json::from_str(&text).unwrap();
        let reencoded = if name.starts_with("request_") {
            encode(&decode_client(&text).unwrap_or_else(|e| panic!("{name}: {e}")))
        } else {
            encode(&decode_server(&text).unwrap_or_else(|e| panic!("{name}: {e}")))
        };
        assert!(reencoded.ends_with('\n'));
        let again: Value = serde_json::from_str(&reencoded).unwrap();
        assert_eq!(original, again, "{name}");
    }
}

#[test]
fn rejects_unknown_version() {
    assert!(matches!(
        decode_client(r#"{"v":2,"type":"GetState"}"#),
        Err(DecodeError::Version(2))
    ));
}

#[test]
fn request_without_id_is_accepted() {
    let message = decode_client(r#"{"v":1,"type":"Resume"}"#).unwrap();
    assert_eq!(message.id, None);
    assert_eq!(message.request, Request::Resume);
}
