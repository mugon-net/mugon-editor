#[test]
fn war_mat() {
    let t = std::fs::read_to_string("tests/war.mat").unwrap();
    let m = mugon_editor_import::unity::material(&t);
    assert_eq!(m.texture_guid.as_deref(), Some("a0e6e61468834da4ea64e5e8c2b405ce"), "name {}", m.name);
}
