use super::fit_page;
use codex_tools::output_archive::ResourcePage;
use pretty_assertions::assert_eq;

#[test]
fn pages_use_available_space_and_reassemble_escaped_unicode() {
    let original = "ordinary text 雪\0\"\\\n".repeat(120);
    let mut offset = 0;
    let mut recovered = String::new();
    while offset < original.len() {
        let page = fit_page(
            ResourcePage {
                resource_id: "reference:example".into(),
                offset: offset as u64,
                next_offset: original.len() as u64,
                total_bytes: original.len() as u64,
                eof: true,
                text: original[offset..].into(),
            },
            512,
        )
        .unwrap();
        assert!(serde_json::to_vec(&page).unwrap().len() <= 512);
        assert!(page.eof || page.text.len() > 200);
        assert_eq!(page.next_offset, page.offset + page.text.len() as u64);
        assert_eq!(page.eof, page.next_offset == page.total_bytes);
        offset = page.next_offset as usize;
        recovered.push_str(&page.text);
    }
    assert_eq!(recovered, original);
}

#[test]
fn exact_budget_preserves_full_page_and_small_budget_cannot_stall() {
    let make_page = || ResourcePage {
        resource_id: "r".into(),
        offset: 9,
        next_offset: 12,
        total_bytes: 12,
        eof: true,
        text: "雪".into(),
    };
    let budget = serde_json::to_vec(&make_page()).unwrap().len();
    assert_eq!(fit_page(make_page(), budget).unwrap(), make_page());
    assert!(fit_page(make_page(), budget - 1).is_err());
    let empty = ResourcePage {
        resource_id: "r".into(),
        offset: 12,
        next_offset: 12,
        total_bytes: 12,
        eof: true,
        text: String::new(),
    };
    assert_eq!(fit_page(empty, budget).unwrap().next_offset, 12);
}
