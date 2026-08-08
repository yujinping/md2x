//! OOXML 打包：固定部件 + media 组装成 .docx（zip）。

use crate::error::MpeError;
use std::io::Write;
use std::path::Path;

use super::DocxContent;

const XML_DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

const NS_W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const NS_CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const NS_WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";

/// 将 body 包成完整的 word/document.xml。
pub fn document_wrapper(body: &str) -> String {
    format!(
        "{XML_DECL}<w:document xmlns:w=\"{NS_W}\" xmlns:r=\"{NS_R}\" \
         xmlns:wp=\"{NS_WP}\" xmlns:a=\"{NS_A}\" xmlns:pic=\"{NS_PIC}\">\
         <w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/>\
         <w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" \
         w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    )
}

/// 将内容打包为 .docx 文件。
pub fn write_package(content: &DocxContent, dst: &Path) -> Result<(), MpeError> {
    let file = std::fs::File::create(dst).map_err(MpeError::IoError)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    write_zip(&mut zip, &opts, "[Content_Types].xml", &content_types_xml(&content.media))?;
    write_zip(&mut zip, &opts, "_rels/.rels", &root_rels_xml())?;
    write_zip(&mut zip, &opts, "docProps/core.xml", &core_xml())?;
    write_zip(&mut zip, &opts, "docProps/app.xml", &app_xml())?;
    write_zip(&mut zip, &opts, "word/document.xml", &content.document_xml)?;
    write_zip(&mut zip, &opts, "word/styles.xml", &styles_xml())?;
    write_zip(&mut zip, &opts, "word/numbering.xml", &numbering_xml())?;
    write_zip(
        &mut zip,
        &opts,
        "word/_rels/document.xml.rels",
        &document_rels_xml(&content.media),
    )?;

    for (name, bytes, _ct) in &content.media {
        write_zip(&mut zip, &opts, &format!("word/media/{name}"), bytes)?;
    }

    zip.finish().map_err(zip_err)?;
    Ok(())
}

fn write_zip<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    opts: &zip::write::SimpleFileOptions,
    name: &str,
    data: impl AsRef<[u8]>,
) -> Result<(), MpeError> {
    zip.start_file(name, opts.clone()).map_err(zip_err)?;
    zip.write_all(data.as_ref()).map_err(MpeError::IoError)
}

fn zip_err(e: zip::result::ZipError) -> MpeError {
    MpeError::IoError(std::io::Error::other(e.to_string()))
}

fn content_types_xml(media: &[(String, Vec<u8>, String)]) -> String {
    let mut defaults = String::new();
    let mut seen = std::collections::BTreeSet::new();
    for (name, _bytes, ct) in media {
        let ext = name.rsplit('.').next().unwrap_or("bin").to_ascii_lowercase();
        if seen.insert(ext.clone()) {
            defaults.push_str(&format!(
                "<Default Extension=\"{ext}\" ContentType=\"{ct}\"/>"
            ));
        }
    }
    format!(
        "{XML_DECL}<Types xmlns=\"{NS_CT}\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         {defaults}\
         <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
         <Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
         <Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/>\
         <Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>\
         <Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>\
         </Types>"
    )
}

fn root_rels_xml() -> String {
    format!(
        "{XML_DECL}<Relationships xmlns=\"{NS_PKG_REL}\">\
         <Relationship Id=\"rId1\" Type=\"{NS_R}/officeDocument\" Target=\"word/document.xml\"/>\
         <Relationship Id=\"rId2\" Type=\"{NS_PKG_REL}/metadata/core-properties\" Target=\"docProps/core.xml\"/>\
         <Relationship Id=\"rId3\" Type=\"{NS_R}/extended-properties\" Target=\"docProps/app.xml\"/>\
         </Relationships>"
    )
}

fn document_rels_xml(media: &[(String, Vec<u8>, String)]) -> String {
    let mut rels = String::new();
    rels.push_str(&format!(
        "{XML_DECL}<Relationships xmlns=\"{NS_PKG_REL}\">\
         <Relationship Id=\"rId1\" Type=\"{NS_R}/styles\" Target=\"styles.xml\"/>\
         <Relationship Id=\"rId2\" Type=\"{NS_R}/numbering\" Target=\"numbering.xml\"/>"
    ));
    for (i, (name, _bytes, _ct)) in media.iter().enumerate() {
        let rid = i + 3;
        rels.push_str(&format!(
            "<Relationship Id=\"rId{rid}\" Type=\"{NS_R}/image\" Target=\"media/{name}\"/>"
        ));
    }
    rels.push_str("</Relationships>");
    rels
}

fn core_xml() -> String {
    format!(
        "{XML_DECL}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" \
         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\
         <dc:creator>md2x</dc:creator><cp:lastModifiedBy>md2x</cp:lastModifiedBy></cp:coreProperties>"
    )
}

fn app_xml() -> String {
    format!(
        "{XML_DECL}<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\" \
         xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\">\
         <Application>md2x</Application></Properties>"
    )
}

fn styles_xml() -> String {
    let mut headings = String::new();
    let defs = [
        ("Heading1", "heading 1", "0", "45", "450", "150"),
        ("Heading2", "heading 2", "1", "36", "450", "150"),
        ("Heading3", "heading 3", "2", "30", "420", "120"),
        ("Heading4", "heading 4", "3", "26", "390", "90"),
        ("Heading5", "heading 5", "4", "22", "360", "90"),
        ("Heading6", "heading 6", "5", "22", "360", "90"),
    ];
    for (id, name, lvl, sz, before, after) in defs {
        headings.push_str(&format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"{id}\">\
             <w:name w:val=\"{name}\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/>\
             <w:qFormat/><w:uiPriority w:val=\"9\"/>\
             <w:pPr><w:keepNext/><w:keepLines/><w:outlineLvl w:val=\"{lvl}\"/>\
             <w:spacing w:before=\"{before}\" w:after=\"{after}\"/>\
             <w:pBdr><w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"1\" w:color=\"eaecef\"/></w:pBdr>\
             </w:pPr>\
             <w:rPr><w:b/><w:sz w:val=\"{sz}\"/><w:szCs w:val=\"{sz}\"/></w:rPr>\
             </w:style>"
        ));
    }
    format!(
        "{XML_DECL}<w:styles xmlns:w=\"{NS_W}\">\
         <w:docDefaults>\
         <w:rPrDefault><w:rPr>\
         <w:rFonts w:ascii=\"Segoe UI\" w:eastAsia=\"Microsoft YaHei\" w:hAnsi=\"Segoe UI\" w:cs=\"Segoe UI\"/>\
         <w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/></w:rPr></w:rPrDefault>\
         <w:pPrDefault><w:pPr><w:spacing w:line=\"350\" w:lineRule=\"auto\" w:after=\"240\"/></w:pPr></w:pPrDefault>\
         </w:docDefaults>\
         <w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\">\
         <w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
         {headings}\
         </w:styles>"
    )
}

fn numbering_xml() -> String {
    let lvl = |ilvl: u32, fmt: &str, text: &str, left: u32| {
        format!(
            "<w:lvl w:ilvl=\"{ilvl}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{fmt}\"/>\
             <w:lvlText w:val=\"{text}\"/><w:lvlJc w:val=\"left\"/>\
             <w:pPr><w:ind w:left=\"{left}\" w:hanging=\"240\"/></w:pPr></w:lvl>"
        )
    };
    let bullet = format!(
        "{}{}{}",
        lvl(0, "bullet", "•", 480),
        lvl(1, "bullet", "◦", 960),
        lvl(2, "bullet", "▪", 1440),
    );
    let decimal = format!(
        "{}{}{}",
        lvl(0, "decimal", "%1.", 480),
        lvl(1, "decimal", "%2.", 960),
        lvl(2, "decimal", "%3.", 1440),
    );
    format!(
        "{XML_DECL}<w:numbering xmlns:w=\"{NS_W}\">\
         <w:abstractNum w:abstractNumId=\"0\"><w:multiLevelType w:val=\"hybridMultilevel\"/>{bullet}</w:abstractNum>\
         <w:abstractNum w:abstractNumId=\"1\"><w:multiLevelType w:val=\"hybridMultilevel\"/>{decimal}</w:abstractNum>\
         <w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>\
         <w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num>\
         </w:numbering>"
    )
}
