//! Lab adapters stay off the product path (TASK-6).

#![allow(clippy::unwrap_used, clippy::expect_used)]

fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn product_cargo_does_not_link_ffmpeg_lab() {
    let cargo = std::fs::read_to_string(root().join("Cargo.toml")).unwrap();
    assert!(
        !cargo.contains("lab/"),
        "lab must not be a workspace/path dep"
    );
    assert!(
        cargo.contains("# Intentionally empty"),
        "product [dependencies] stays empty"
    );
    assert!(!cargo.contains("libavcodec"));
    assert!(!cargo.contains("ffmpeg-sys"));
}

#[test]
fn src_does_not_spawn_ffmpeg_or_fdk() {
    let mut hits = Vec::new();
    for ent in walk(root().join("src")) {
        let text = std::fs::read_to_string(&ent).unwrap();
        if text.contains("Command::new(\"ffmpeg\")")
            || text.contains("Command::new(\"ffprobe\")")
            || text.contains("Command::new(\"fdk")
        {
            hits.push(ent);
        }
    }
    assert!(hits.is_empty(), "src spawned oracle: {hits:?}");
}

#[test]
fn lab_pin_and_adapter_are_native_aac_in_process() {
    let pin = std::fs::read_to_string(root().join("lab/PIN.md")).unwrap();
    assert!(pin.contains("9.0.1"));
    assert!(pin.contains("cf38e0e28c7e5605942c4a77755349b0145804a397af37eb1fb4c77cb237f635"));
    assert!(pin.contains("threads=1"));
    assert!(pin.contains("in_process_au"));
    assert!(pin.contains("in_process_container"));
    assert!(pin.contains("process_launch"));
    assert!(pin.contains("--enable-decoder=aac"));
    assert!(pin.contains("--enable-encoder=aac"));
    let adapt = std::fs::read_to_string(root().join("lab/libavcodec/avc_adapt.c")).unwrap();
    assert!(adapt.contains("avcodec_send_packet"));
    assert!(adapt.contains("avcodec_send_frame"));
    assert!(adapt.contains("avcodec_find_decoder(AV_CODEC_ID_AAC)"));
    assert!(adapt.contains("avcodec_find_encoder(AV_CODEC_ID_AAC)"));
    assert!(adapt.contains("\"threads\""));
    let mk = std::fs::read_to_string(root().join("lab/libavcodec/Makefile")).unwrap();
    assert!(mk.contains("FFMPEG_PREFIX"));
    assert!(mk.contains("Not invoked by cargo test"));
}

fn walk(dir: std::path::PathBuf) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(p));
        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(p);
        }
    }
    out
}
