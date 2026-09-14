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
            || text.contains("Command::new(\"faad")
            || text.contains("Command::new(\"faac")
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

#[test]
fn lab_fdk_pin_is_isolated_v2_0_3() {
    let pin = std::fs::read_to_string(root().join("lab/fdk/PIN.md")).unwrap();
    assert!(pin.contains("2.0.3"));
    assert!(pin.contains("e25671cd96b10bad896aa42ab91a695a9e573395262baed4e4a2ff178d6a3a78"));
    assert!(pin.contains("AFTERBURNER=0"));
    assert!(pin.contains("AACENC_TRANSMUX=2"));
    assert!(pin.contains("Fraunhofer"));
    let adapt = std::fs::read_to_string(root().join("lab/fdk/fdk_adapt.c")).unwrap();
    assert!(adapt.contains("aacDecoder_DecodeFrame"));
    assert!(adapt.contains("aacEncEncode"));
    assert!(adapt.contains("AACENC_AFTERBURNER"));
    let cargo = std::fs::read_to_string(root().join("Cargo.toml")).unwrap();
    assert!(!cargo.contains("fdk"));
    assert!(!cargo.contains("fdk-aac"));
}

#[test]
fn lab_faac_pin_is_isolated_1_31_1() {
    let pin = std::fs::read_to_string(root().join("lab/faac/PIN.md")).unwrap();
    assert!(pin.contains("1.31.1"));
    assert!(pin.contains("3191bf1b131f1213221ed86f65c2dfabf22d41f6b3771e7e65b6d29478433527"));
    assert!(pin.contains("LOW"));
    assert!(pin.contains("ADTS"));
    assert!(pin.contains("LGPL"));
    assert!(pin.contains("bitRate"));
    let adapt = std::fs::read_to_string(root().join("lab/faac/faac_adapt.c")).unwrap();
    assert!(adapt.contains("faacEncEncode"));
    assert!(adapt.contains("FAAC_INPUT_FLOAT"));
    assert!(adapt.contains("ADTS_STREAM"));
    assert!(adapt.contains("aacObjectType = LOW"));
    let mk = std::fs::read_to_string(root().join("lab/faac/Makefile")).unwrap();
    assert!(mk.contains("Not invoked by cargo test"));
    let report = std::fs::read_to_string(root().join("lab/faac/REPORT.md")).unwrap();
    assert!(report.contains("97280"));
    assert!(report.contains("no-go"));
    assert!(report.contains("syom"));
    let product = std::fs::read_to_string(root().join("Cargo.toml")).unwrap();
    assert!(!product.contains("faac"));
    assert!(!product.contains("libfaac"));
}

#[test]
fn lab_fdk_aac_rust_pin_is_isolated_0_2_3() {
    let pin = std::fs::read_to_string(root().join("lab/fdk-aac-rust/PIN.md")).unwrap();
    assert!(pin.contains("0.2.3"));
    assert!(pin.contains("607e6ba558b60e1219ccc8200bbf03fb96fad20c5d948ab25982ea964af13ae5"));
    assert!(pin.contains("d8e6b1a3aa606c450241632b64b703f21ea31ce3"));
    assert!(pin.contains("default = [\"ffi\"]"));
    assert!(pin.contains("--no-default-features"));
    assert!(pin.contains("Fraunhofer"));
    assert!(pin.contains("Not invoked by cargo test") || pin.contains("must not"));
    let cargo = std::fs::read_to_string(root().join("lab/fdk-aac-rust/Cargo.toml")).unwrap();
    assert!(cargo.contains("fdk-aac-rust"));
    assert!(cargo.contains("default-features = false"));
    let mk = std::fs::read_to_string(root().join("lab/fdk-aac-rust/Makefile")).unwrap();
    assert!(mk.contains("Not invoked by cargo test"));
    let report = std::fs::read_to_string(root().join("lab/fdk-aac-rust/REPORT.md")).unwrap();
    assert!(report.contains("no-go"));
    assert!(report.contains("shared") || report.contains("lineage"));
    assert!(report.contains("13312"));
    let product = std::fs::read_to_string(root().join("Cargo.toml")).unwrap();
    assert!(!product.contains("fdk-aac-rust"));
    assert!(!product.contains("lab/fdk-aac-rust"));
}

#[test]
fn lab_faad2_pin_is_isolated_2_11_3() {
    let pin = std::fs::read_to_string(root().join("lab/faad2/PIN.md")).unwrap();
    assert!(pin.contains("2.11.3"));
    assert!(pin.contains("860ab62087e336c1844a70e33196c1790b525fb9a9e7b6ac4fab1a1a4e4d5ce8"));
    assert!(pin.contains("FAAD_FMT_FLOAT"));
    assert!(pin.contains("GPL-2.0-or-later"));
    let adapt = std::fs::read_to_string(root().join("lab/faad2/faad_adapt.c")).unwrap();
    assert!(adapt.contains("NeAACDecDecode"));
    assert!(adapt.contains("FAAD_FMT_FLOAT"));
    assert!(adapt.contains("NeAACDecInit"));
    let mk = std::fs::read_to_string(root().join("lab/faad2/Makefile")).unwrap();
    assert!(mk.contains("FAAD2_PREFIX"));
    assert!(mk.contains("Not invoked by cargo test"));
    let disagree = std::fs::read_to_string(root().join("lab/faad2/DISAGREE.md")).unwrap();
    assert!(disagree.contains("alignment / priming"));
    assert!(disagree.contains("channel labels"));
    let cargo = std::fs::read_to_string(root().join("Cargo.toml")).unwrap();
    assert!(!cargo.contains("faad"));
    assert!(!cargo.contains("libfaad"));
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
