use std::path::Path;

use iris_reader::{
    image::ImageManager,
    options::IconMode,
    renderer::{RenderedDocument, markdown},
    theme::Theme,
};

fn render_source(source: &str, width: u16, base_dir: Option<&Path>) -> RenderedDocument {
    let document = markdown::parse(source);
    let theme = Theme::ember(IconMode::NerdFont).expect("Ember theme should load");
    let mut images = ImageManager::default();

    markdown::render(&document, width, &theme, true, 4, &mut images, base_dir)
}

fn rendered_fixture(name: &str, width: u16) -> String {
    let source = std::fs::read_to_string(Path::new("tests/fixtures").join(name))
        .expect("fixture should be readable");

    let rendered = render_source(&source, width, Some(Path::new("tests/fixtures")));

    rendered
        .lines
        .into_iter()
        .map(|line| line.plain)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn renders_core_markdown_features() {
    let output = rendered_fixture("everything.md", 100);

    assert!(output.contains("IRIS Markdown Fixture"));
    assert!(output.contains("IMPORTANT"));
    assert!(output.contains("Finished task"));
    assert!(output.contains("┌"));
    assert!(output.contains("Hello from IRIS"));
    assert!(output.contains("IRIS logo"));
    assert!(output.contains("HTML strong text"));
    assert!(output.contains("highlighting"));
}

#[test]
fn renders_unicode_without_losing_content() {
    let output = rendered_fixture("unicode.md", 40);

    assert!(output.contains("Café"));
    assert!(output.contains("日本語"));
    assert!(output.contains("🔥"));
    assert!(output.contains("中文"));
    assert!(output.contains("مرحبًا"));
    assert!(output.contains("नमस्ते"));
}

#[test]
fn records_standalone_markdown_and_html_images() {
    let source = std::fs::read_to_string("tests/fixtures/everything.md")
        .expect("fixture should be readable");

    let rendered = render_source(&source, 100, Some(Path::new("tests/fixtures")));

    assert!(
        rendered
            .images
            .iter()
            .any(|image| image.source == "assets/iris.png")
    );

    assert!(
        rendered
            .images
            .iter()
            .any(|image| image.alt == "Centered IRIS image")
    );
}

#[test]
fn prose_reflows_at_smaller_widths() {
    let wide = rendered_fixture("wrapping.md", 100).lines().count();
    let narrow = rendered_fixture("wrapping.md", 35).lines().count();

    assert!(narrow > wide);
}

#[test]
fn renders_tight_list_text_and_tasks() {
    let source = "- first item\n- [x] finished task\n- [ ] pending task\n";
    let rendered = render_source(source, 80, None);

    let plain = rendered
        .lines
        .iter()
        .map(|line| line.plain.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("first item"));
    assert!(plain.contains("finished task"));
    assert!(plain.contains("pending task"));
}

#[test]
fn promotes_markdown_images_from_paragraph_flow_to_image_blocks() {
    let source = "Before ![IRIS](assets/iris.png) after";

    let rendered = render_source(source, 80, Some(Path::new("tests/fixtures")));

    assert_eq!(rendered.images.len(), 1);
    assert_eq!(rendered.images[0].source, "assets/iris.png");

    let plain = rendered
        .lines
        .iter()
        .map(|line| line.plain.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("Before"));
    assert!(plain.contains("after"));
}
