use std::path::{Path, PathBuf};

use printpdf::{
    Mm, Op, ParsedFont, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt, TextItem,
};

const PAGE_WIDTH_MM: f32 = 210.0;
const PAGE_HEIGHT_MM: f32 = 297.0;
const MARGIN_MM: f32 = 18.0;
const FONT_SIZE_PT: f32 = 10.0;
const LINE_HEIGHT_PT: f32 = 14.0;
const LINES_PER_PAGE: usize = 52;
const MAX_LINE_UNITS: usize = 96;

pub fn write_history_pdf(path: &Path, text: &str) -> Result<(), String> {
    let font_path = find_system_font().ok_or_else(|| {
        "No compatible Windows text font was found for PDF generation".to_string()
    })?;
    let font_bytes = std::fs::read(&font_path)
        .map_err(|e| format!("read PDF font {}: {e}", font_path.display()))?;
    let bytes = history_pdf_bytes(text, &font_bytes)?;
    std::fs::write(path, bytes).map_err(|e| format!("write PDF history {}: {e}", path.display()))
}

pub fn history_pdf_bytes(text: &str, font_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut font_warnings = Vec::new();
    let font = ParsedFont::from_bytes(font_bytes, 0, &mut font_warnings)
        .ok_or_else(|| "Windows text font could not be parsed for PDF generation".to_string())?;
    let mut document = PdfDocument::new("CopyAPP History");
    let font_id = document.add_font(&font);
    let lines = wrap_history_text(text);
    let mut pages = Vec::new();

    for page_lines in lines.chunks(LINES_PER_PAGE) {
        let mut operations = vec![
            Op::StartTextSection,
            Op::SetTextCursor {
                pos: Point::new(Mm(MARGIN_MM), Mm(PAGE_HEIGHT_MM - MARGIN_MM)),
            },
            Op::SetLineHeight {
                lh: Pt(LINE_HEIGHT_PT),
            },
            Op::SetFont {
                font: PdfFontHandle::External(font_id.clone()),
                size: Pt(FONT_SIZE_PT),
            },
        ];
        for line in page_lines {
            operations.push(Op::ShowText {
                items: vec![TextItem::Text(line.clone())],
            });
            operations.push(Op::AddLineBreak);
        }
        operations.push(Op::EndTextSection);
        pages.push(PdfPage::new(
            Mm(PAGE_WIDTH_MM),
            Mm(PAGE_HEIGHT_MM),
            operations,
        ));
    }

    let mut warnings = Vec::new();
    let options = PdfSaveOptions {
        subset_fonts: true,
        ..Default::default()
    };
    Ok(document.with_pages(pages).save(&options, &mut warnings))
}

fn wrap_history_text(text: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = Vec::new();
    for source_line in normalized.split('\n') {
        let source_line = source_line.replace('\t', "    ");
        if source_line.is_empty() {
            output.push(String::new());
            continue;
        }
        let mut remaining = source_line.as_str();
        while display_units(remaining) > MAX_LINE_UNITS {
            let split = wrap_position(remaining, MAX_LINE_UNITS);
            let (line, rest) = remaining.split_at(split);
            output.push(line.trim_end().to_string());
            remaining = rest.trim_start();
        }
        output.push(remaining.to_string());
    }
    if output.is_empty() {
        output.push(String::new());
    }
    output
}

fn wrap_position(value: &str, max_units: usize) -> usize {
    let mut units = 0usize;
    let mut last_whitespace = None;
    let mut boundary = value.len();
    for (index, character) in value.char_indices() {
        let next_units = units + character_units(character);
        if next_units > max_units {
            boundary = index;
            break;
        }
        units = next_units;
        if character.is_whitespace() {
            last_whitespace = Some(index);
        }
    }
    last_whitespace
        .filter(|index| *index > 0)
        .unwrap_or(boundary)
}

fn display_units(value: &str) -> usize {
    value.chars().map(character_units).sum()
}

fn character_units(character: char) -> usize {
    if character.is_ascii() {
        1
    } else {
        2
    }
}

fn find_system_font() -> Option<PathBuf> {
    [
        PathBuf::from(r"C:\Windows\Fonts\segoeui.ttf"),
        PathBuf::from(r"C:\Windows\Fonts\arial.ttf"),
        PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_preserves_blank_lines_between_copies() {
        let lines = wrap_history_text("one\r\n\r\ntwo\r\n");
        assert_eq!(lines, vec!["one", "", "two", ""]);
    }

    #[test]
    fn generated_history_is_a_multipage_capable_pdf() {
        let font_path = find_system_font().expect("system test font");
        let font = std::fs::read(font_path).expect("read system test font");
        let content = (1..=120)
            .map(|line| format!("Clipboard line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let bytes = history_pdf_bytes(&content, &font).expect("generate history PDF");
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.ends_with(b"%%EOF"));
        assert!(bytes.windows(8).any(|window| window == b"/Count 3"));
        let parsed = PdfDocument::parse(
            &bytes,
            &printpdf::PdfParseOptions::default(),
            &mut Vec::new(),
        )
        .expect("parse generated PDF");
        assert_eq!(parsed.pages.len(), 3);
        let extracted = parsed
            .extract_text()
            .into_iter()
            .flatten()
            .collect::<String>();
        assert!(extracted.contains("Clipboard line 1"));
        assert!(extracted.contains("Clipboard line 120"));

        if let Ok(output_dir) = std::env::var("COPYAPP_PDF_QA_DIR") {
            let output_dir = PathBuf::from(output_dir);
            std::fs::create_dir_all(&output_dir).expect("create PDF QA directory");
            std::fs::write(output_dir.join("CopyAPP-History-QA.pdf"), &bytes)
                .expect("write PDF QA artifact");
            for page_number in 1..=parsed.pages.len() {
                let svg = parsed
                    .page_to_svg(
                        page_number,
                        &printpdf::PdfToSvgOptions::default(),
                        &mut Vec::new(),
                    )
                    .expect("render generated PDF page to SVG");
                std::fs::write(output_dir.join(format!("page-{page_number}.svg")), svg)
                    .expect("write PDF QA page SVG");
            }
        }
    }
}
