use super::inputs;
use pretty_assertions::assert_eq;

#[test]
fn changed_dependency_and_removed_input_invalidate_even_at_the_same_size() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir(&root)?;
    let file = root.join("dependency.mbt");
    std::fs::write(&file, "original")?;
    let mut memo = Default::default();
    let first = inputs(std::slice::from_ref(&root), &mut memo)?;
    assert_eq!(first, inputs(std::slice::from_ref(&root), &mut memo)?);
    std::fs::write(&file, "modified")?;
    let changed = inputs(std::slice::from_ref(&root), &mut memo)?;
    assert_ne!(first, changed);
    std::fs::remove_file(file)?;
    assert_ne!(changed, inputs(std::slice::from_ref(&root), &mut memo)?);
    std::fs::remove_dir(root)
}
