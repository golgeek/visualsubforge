use subtitle_formats::Format;
use vsf_application::subtitle_file;

const ASS: &str = "[Script Info]\nScriptType: v4.00+\n[V4+ Styles]\nFormat: Name, Fontname\nStyle: Default,Arial\n[Events]\nFormat: Layer, Start, End, Style, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,{\\pos(20,30)}日本語\\N😀\n[Private]\nretained=yes\n";
#[test]
fn shared_acceptance_counts_opaque_records_and_rejects_wrong_identity_or_encoding() {
    let parsed = subtitle_file::parse_document(ASS.as_bytes(), Some("ass")).unwrap();
    assert_eq!(parsed.source.format(), Format::Ass);
    assert_eq!(parsed.source.serialize(&parsed.document).unwrap(), ASS);
    assert!(subtitle_file::parse_document(ASS.as_bytes(), Some("ssa")).is_err());
    assert!(subtitle_file::parse_document(ASS.as_bytes(), Some("srt")).is_err());
    assert!(
        subtitle_file::parse_document(b"\xff\xfe", Some("ass"))
            .unwrap_err()
            .contains("UTF-8")
    );
    let oversized = format!(
        "{ASS}{}",
        "x".repeat(vsf_application::srt_limits::MAX_BYTES)
    );
    assert!(
        subtitle_file::parse_document(oversized.as_bytes(), Some("ass"))
            .unwrap_err()
            .contains("16 MiB")
    );
    let srt = "1\n00:00:01,001 --> 00:00:02,999\n{\\a6\\move(0,0,100,100)}<i>Mixed</i>\\Nline\n\n";
    let parsed = subtitle_file::parse_document(srt.as_bytes(), Some("srt")).unwrap();
    assert_eq!(parsed.source.format(), Format::Srt);
    assert_eq!(parsed.source.serialize(&parsed.document).unwrap(), srt);
}
