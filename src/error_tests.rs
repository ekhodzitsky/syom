use super::SyomError;

#[test]
fn test_display_names_media_and_io() {
    assert!(SyomError::Media("nope".into()).to_string().contains("nope"));
    let io = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
    assert!(!SyomError::from(io).to_string().is_empty());
}
