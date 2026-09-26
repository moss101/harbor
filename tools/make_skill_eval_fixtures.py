#!/usr/bin/env python3
"""Generate the Office fixtures used by the skill eval suites (decision 0006).

Dependency-free and deterministic (fixed zip timestamps), so the fixture
hashes recorded in eval reports are reproducible:

- fixtures/office/formula_errors.xlsx — a budget sheet with cached
  #DIV/0!, #REF! (dangling sheet reference), #NAME? (unqualified function)
  and #N/A errors next to healthy formulas; exercised by formula-audit.
- fixtures/office/letter_template.docx — a letter with {{name}}, [[ref]],
  $AMOUNT$ and <<sender>> placeholders; exercised by placeholder-fill.
- fixtures/office/board_deck.pptx — a six-slide deck with template text
  left behind, numeric claims without a source in the notes and two
  identically structured slides; exercised by deck-review.
- fixtures/office/dcf_model.xlsx — a projection with a hard-coded value
  inside a formula row (reproducible by the shifted neighbour formula), a
  constant that no formula reproduces, and a link to another workbook;
  exercised by financial-model-review.
- fixtures/office/report_styles.docx — headings that skip a level, direct
  font/size/colour overrides, an Arabic paragraph not marked RTL, a
  mixed-direction paragraph, cramped margins and heading sizes in
  styles.xml; exercised by document-style-review and doc-coauthoring.
- fixtures/office/messy_table.xlsx — a sales table with stray whitespace,
  numbers stored as text, a region spelled three ways, a blank row inside
  the table, a duplicate row and a revenue figure stored with its currency
  symbol; exercised by table-cleanup.
- fixtures/office/quarterly_report.pdf — a three-page report (plain
  Helvetica text, uncompressed) whose figures and sentences sit on known
  pages; exercised by report-to-slides' page citations.
"""
from __future__ import annotations

import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / "fixtures" / "office"
STAMP = (2026, 9, 18, 0, 0, 0)


def write_zip(path: pathlib.Path, parts: list[tuple[str, str]]) -> None:
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for name, text in parts:
            info = zipfile.ZipInfo(name, date_time=STAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, text.encode("utf-8"))


def xlsx() -> None:
    def c(ref: str, *, v: str | None = None, f: str | None = None, t: str | None = None) -> str:
        attrs = f' r="{ref}"' + (f' t="{t}"' if t else "")
        body = (f"<f>{f}</f>" if f else "") + (f"<v>{v}</v>" if v is not None else "")
        return f"<c{attrs}>{body}</c>"

    def s(ref: str, text: str) -> str:
        return f'<c r="{ref}" t="inlineStr"><is><t>{text}</t></is></c>'

    rows = [
        (1, [s("A1", "Line"), s("B1", "Amount")]),
        (2, [s("A2", "Revenue"), c("B2", v="100")]),
        (3, [s("A3", "Units"), c("B3", v="0")]),
        (4, [s("A4", "Per unit"), c("B4", f="B2/B3", v="#DIV/0!", t="e")]),
        (5, [s("A5", "Total"), c("B5", f="SUM(B2:B3)", v="100")]),
        (6, [s("A6", "External"), c("B6", f="Missing!A1", v="#REF!", t="e")]),
        (7, [s("A7", "Custom"), c("B7", f="FOO(B2)", v="#NAME?", t="e")]),
        (8, [s("A8", "Lookup"), c("B8", f='VLOOKUP("zz",A2:B3,2,FALSE)', v="#N/A", t="e")]),
        (9, [s("A9", "Rounded"), c("B9", f="ROUND(B2/3,2)", v="33.33")]),
    ]
    sheet_rows = "".join(f'<row r="{r}">{"".join(cells)}</row>' for r, cells in rows)
    sheet1 = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        f"<sheetData>{sheet_rows}</sheetData></worksheet>"
    )
    workbook = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        '<sheets><sheet name="Budget" sheetId="1" r:id="rId1"/></sheets></workbook>'
    )
    wb_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>'
        '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
        "</Relationships>"
    )
    styles = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        '<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>'
        '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>'
        '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
        '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
        '<cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>'
        '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>'
        "</styleSheet>"
    )
    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>'
        "</Types>"
    )
    rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>'
        "</Relationships>"
    )
    write_zip(
        OUT / "formula_errors.xlsx",
        [
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", rels),
            ("xl/workbook.xml", workbook),
            ("xl/_rels/workbook.xml.rels", wb_rels),
            ("xl/styles.xml", styles),
            ("xl/worksheets/sheet1.xml", sheet1),
        ],
    )


def docx() -> None:
    def p(text: str, style: str | None = None) -> str:
        ppr = f'<w:pPr><w:pStyle w:val="{style}"/></w:pPr>' if style else ""
        escaped = text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
        return f'<w:p>{ppr}<w:r><w:t xml:space="preserve">{escaped}</w:t></w:r></w:p>'

    body = "".join(
        [
            p("Harbor Letter", "Title"),
            p("Dear {{name}},"),
            p("Your reference is [[ref]] and the amount due is $AMOUNT$."),
            p("This paragraph has no placeholders and must stay untouched."),
            p("Regards, <<sender>>"),
        ]
    )
    document = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
        f"<w:body>{body}<w:sectPr/></w:body></w:document>"
    )
    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        "</Types>"
    )
    rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
        "</Relationships>"
    )
    write_zip(
        OUT / "letter_template.docx",
        [("[Content_Types].xml", content_types), ("_rels/.rels", rels), ("word/document.xml", document)],
    )


XML_HEAD = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
OOXML_RELS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PKG_RELS = "http://schemas.openxmlformats.org/package/2006/relationships"


def esc(text: str) -> str:
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def pptx() -> None:
    """Six slides: cover, agenda, three content slides, summary."""
    slides = [
        ("Harbor board update — Q3", [], "Cover. Presenter: ops lead."),
        ("Agenda", ["Results", "Pipeline", "Risks", "Next steps"], None),
        (
            "Results: revenue up 12% to $540k",
            ["Gross margin improved to 61%", "Churn fell to 3.1%", "Two enterprise logos closed"],
            "Numbers pulled from the finance dashboard.",
        ),
        (
            "Pipeline",
            ["Lorem ipsum dolor sit amet", "Qualified pipeline $2.4m", "Win rate 28% per CRM report"],
            "Source: CRM export 2026-09-15.",
        ),
        (
            "Risks",
            ["Hiring plan slips one quarter", "Vendor contract renews at +9%", "Click to add text"],
            None,
        ),
        ("Summary", ["Ask: approve the hiring plan", "Next board: December"], "Source: this deck."),
    ]

    def slide_xml(title: str, bullets: list[str]) -> str:
        def sp(text: str, ph: str | None, sp_id: int) -> str:
            ph_xml = f'<p:nvPr><p:ph type="{ph}"/></p:nvPr>' if ph else "<p:nvPr/>"
            return (
                f'<p:sp><p:nvSpPr><p:cNvPr id="{sp_id}" name="TextBox {sp_id}"/><p:cNvSpPr/>{ph_xml}</p:nvSpPr>'
                f'<p:spPr/><p:txBody><a:bodyPr/><a:p><a:r><a:rPr lang="en-US"/><a:t>{esc(text)}</a:t></a:r></a:p></p:txBody></p:sp>'
            )

        shapes = sp(title, "title", 2) + "".join(sp(b, None, 3 + i) for i, b in enumerate(bullets))
        return (
            XML_HEAD
            + '<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
            + f'xmlns:r="{OOXML_RELS}" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">'
            + f'<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{shapes}</p:spTree></p:cSld></p:sld>'
        )

    def notes_xml(text: str) -> str:
        return (
            XML_HEAD
            + '<p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
            + f'xmlns:r="{OOXML_RELS}" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">'
            + '<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>'
            + '<p:sp><p:nvSpPr><p:cNvPr id="2" name="Notes"/><p:cNvSpPr/><p:nvPr><p:ph type="body"/></p:nvPr></p:nvSpPr>'
            + f'<p:spPr/><p:txBody><a:bodyPr/><a:p><a:r><a:t>{esc(text)}</a:t></a:r></a:p></p:txBody></p:sp>'
            + "</p:spTree></p:cSld></p:notes>"
        )

    parts: list[tuple[str, str]] = []
    overrides = []
    sld_ids = []
    pres_rels = []
    for n, (title, bullets, notes) in enumerate(slides, start=1):
        parts.append((f"ppt/slides/slide{n}.xml", slide_xml(title, bullets)))
        overrides.append(
            f'<Override PartName="/ppt/slides/slide{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>'
        )
        sld_ids.append(f'<p:sldId id="{255 + n}" r:id="rId{n}"/>')
        pres_rels.append(f'<Relationship Id="rId{n}" Type="{OOXML_RELS}/slide" Target="slides/slide{n}.xml"/>')
        if notes is not None:
            parts.append((f"ppt/notesSlides/notesSlide{n}.xml", notes_xml(notes)))
            overrides.append(
                f'<Override PartName="/ppt/notesSlides/notesSlide{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml"/>'
            )
            parts.append(
                (
                    f"ppt/slides/_rels/slide{n}.xml.rels",
                    XML_HEAD
                    + f'<Relationships xmlns="{PKG_RELS}"><Relationship Id="rId2" Type="{OOXML_RELS}/notesSlide" Target="../notesSlides/notesSlide{n}.xml"/></Relationships>',
                )
            )
    presentation = (
        XML_HEAD
        + f'<p:presentation xmlns:r="{OOXML_RELS}" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">'
        + f'<p:sldIdLst>{"".join(sld_ids)}</p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>'
    )
    content_types = (
        XML_HEAD
        + '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        + '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        + '<Default Extension="xml" ContentType="application/xml"/>'
        + '<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>'
        + '<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>'
        + "".join(overrides)
        + "</Types>"
    )
    rels = (
        XML_HEAD
        + f'<Relationships xmlns="{PKG_RELS}">'
        + f'<Relationship Id="rId1" Type="{OOXML_RELS}/officeDocument" Target="ppt/presentation.xml"/>'
        + '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>'
        + "</Relationships>"
    )
    core = (
        XML_HEAD
        + '<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/">'
        + "<dc:title>Harbor board update</dc:title></cp:coreProperties>"
    )
    write_zip(
        OUT / "board_deck.pptx",
        [
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", rels),
            ("docProps/core.xml", core),
            ("ppt/presentation.xml", presentation),
            ("ppt/_rels/presentation.xml.rels", XML_HEAD + f'<Relationships xmlns="{PKG_RELS}">{"".join(pres_rels)}</Relationships>'),
            *parts,
        ],
    )


def dcf_xlsx() -> None:
    """A projection sheet with formula rows, one hard-coded value and a link."""

    def c(ref: str, *, v: str | None = None, f: str | None = None, t: str | None = None) -> str:
        attrs = f' r="{ref}"' + (f' t="{t}"' if t else "")
        body = (f"<f>{f}</f>" if f else "") + (f"<v>{v}</v>" if v is not None else "")
        return f"<c{attrs}>{body}</c>"

    def s(ref: str, text: str) -> str:
        return f'<c r="{ref}" t="inlineStr"><is><t>{esc(text)}</t></is></c>'

    rows = [
        (1, [s("A1", "Line"), c("B1", v="2024"), c("C1", v="2025"), c("D1", v="2026"), c("E1", v="2027")]),
        # Revenue: D2 is typed as 1210 where =C2*1.1 belongs (reproducible).
        (2, [s("A2", "Revenue"), c("B2", v="1000"), c("C2", f="B2*1.1", v="1100"), c("D2", v="1210"), c("E2", f="D2*1.1", v="1331")]),
        (3, [s("A3", "Costs"), c("B3", v="600"), c("C3", f="B3*1.05", v="630"), c("D3", f="C3*1.05", v="661.5"), c("E3", f="D3*1.05", v="694.575")]),
        # EBIT: D4 is a constant no neighbouring formula reproduces (548.5 expected).
        (4, [s("A4", "EBIT"), c("B4", f="B2-B3", v="400"), c("C4", f="C2-C3", v="470"), c("D4", v="500"), c("E4", f="E2-E3", v="636.425")]),
        (5, [s("A5", "Discount rate"), c("B5", f="[Assumptions.xlsx]Inputs!A1", v="0.08")]),
        (6, [s("A6", "Growth check"), c("B6", f="C2/B2-1", v="0.1"), c("C6", f="D2/C2-1", v="0.1"), c("D6", f="E2/D2-1", v="0.1")]),
    ]
    sheet_rows = "".join(f'<row r="{r}">{"".join(cells)}</row>' for r, cells in rows)
    sheet1 = (
        XML_HEAD
        + '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        + f"<sheetData>{sheet_rows}</sheetData></worksheet>"
    )
    workbook = (
        XML_HEAD
        + '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
        + f'xmlns:r="{OOXML_RELS}">'
        + '<sheets><sheet name="DCF" sheetId="1" r:id="rId1"/></sheets></workbook>'
    )
    wb_rels = (
        XML_HEAD
        + f'<Relationships xmlns="{PKG_RELS}">'
        + f'<Relationship Id="rId1" Type="{OOXML_RELS}/worksheet" Target="worksheets/sheet1.xml"/>'
        + f'<Relationship Id="rId2" Type="{OOXML_RELS}/styles" Target="styles.xml"/>'
        + "</Relationships>"
    )
    styles = (
        XML_HEAD
        + '<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        + '<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>'
        + '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>'
        + '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
        + '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
        + '<cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>'
        + '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>'
        + "</styleSheet>"
    )
    content_types = (
        XML_HEAD
        + '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        + '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        + '<Default Extension="xml" ContentType="application/xml"/>'
        + '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        + '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        + '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>'
        + "</Types>"
    )
    rels = (
        XML_HEAD
        + f'<Relationships xmlns="{PKG_RELS}">'
        + f'<Relationship Id="rId1" Type="{OOXML_RELS}/officeDocument" Target="xl/workbook.xml"/>'
        + "</Relationships>"
    )
    write_zip(
        OUT / "dcf_model.xlsx",
        [
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", rels),
            ("xl/workbook.xml", workbook),
            ("xl/_rels/workbook.xml.rels", wb_rels),
            ("xl/styles.xml", styles),
            ("xl/worksheets/sheet1.xml", sheet1),
        ],
    )


def styles_docx() -> None:
    """A short report whose styling breaks the six principles in known ways."""

    def p(text: str, style: str | None = None, *, rpr: str = "", ppr_extra: str = "") -> str:
        ppr = ""
        if style or ppr_extra:
            ppr = "<w:pPr>" + (f'<w:pStyle w:val="{style}"/>' if style else "") + ppr_extra + "</w:pPr>"
        run_pr = f"<w:rPr>{rpr}</w:rPr>" if rpr else ""
        return f'<w:p>{ppr}<w:r>{run_pr}<w:t xml:space="preserve">{esc(text)}</w:t></w:r></w:p>'

    body = "".join(
        [
            p("Archive migration decision", "Title"),
            p("Summary", "Heading1"),
            p("We recommend moving the archive to local storage in Q4; the cost model assumes 2 engineers for 6 weeks."),
            p("Options considered", "Heading3"),  # skips Heading2
            p("Keep the current provider.", rpr='<w:rFonts w:ascii="Comic Sans MS"/><w:sz w:val="28"/><w:color w:val="FF0000"/>'),
            p("Move to local storage.", rpr="<w:b/>"),  # emphasis only: not contamination
            p("Risks", "Heading3"),
            p("Timeline", "Heading3"),  # heading directly after heading
            p("Migration starts 2026-11-02 and ends 2026-12-12.", ppr_extra='<w:spacing w:before="0" w:after="480"/>'),
            p("ملخص القرار", "Heading2"),  # Arabic heading, not marked RTL
            p("سيتم نقل الأرشيف إلى التخزين المحلي خلال 6 أسابيع بتكلفة 42,000 دولار", ppr_extra="<w:bidi/>"),  # mixed with digits, RTL
        ]
    )
    document = (
        XML_HEAD
        + '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
        + f'<w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="400" w:right="400" w:bottom="720" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>'
    )

    def style(sid: str, name: str, sz: int) -> str:
        return (
            f'<w:style w:type="paragraph" w:styleId="{sid}"><w:name w:val="{name}"/><w:basedOn w:val="Normal"/>'
            f'<w:rPr><w:sz w:val="{sz}"/></w:rPr></w:style>'
        )

    styles = (
        XML_HEAD
        + '<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
        + '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:sz w:val="22"/></w:rPr></w:style>'
        + style("Title", "Title", 52)
        + style("Heading1", "heading 1", 32)  # 16pt
        + style("Heading2", "heading 2", 26)  # 13pt → ratio 1.23 fine
        + style("Heading3", "heading 3", 24)  # 12pt → ratio 1.08 flagged
        + "</w:styles>"
    )
    content_types = (
        XML_HEAD
        + '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        + '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        + '<Default Extension="xml" ContentType="application/xml"/>'
        + '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        + '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
        + "</Types>"
    )
    rels = (
        XML_HEAD
        + f'<Relationships xmlns="{PKG_RELS}">'
        + f'<Relationship Id="rId1" Type="{OOXML_RELS}/officeDocument" Target="word/document.xml"/>'
        + "</Relationships>"
    )
    doc_rels = (
        XML_HEAD
        + f'<Relationships xmlns="{PKG_RELS}">'
        + f'<Relationship Id="rId1" Type="{OOXML_RELS}/styles" Target="styles.xml"/>'
        + "</Relationships>"
    )
    write_zip(
        OUT / "report_styles.docx",
        [
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", rels),
            ("word/_rels/document.xml.rels", doc_rels),
            ("word/document.xml", document),
            ("word/styles.xml", styles),
        ],
    )


def messy_xlsx() -> None:
    def s(ref: str, text: str) -> str:
        return f'<c r="{ref}" t="inlineStr"><is><t xml:space="preserve">{esc(text)}</t></is></c>'

    def n(ref: str, v: str) -> str:
        return f'<c r="{ref}"><v>{v}</v></c>'

    rows = [
        (1, [s("A1", "Region"), s("B1", "Rep"), s("C1", "Units"), s("D1", "Revenue")]),
        (2, [s("A2", "North"), s("B2", "Amina"), n("C2", "12"), n("D2", "1200")]),
        (3, [s("A3", "north "), s("B3", "Omar"), s("C3", "15"), n("D3", "1500")]),
        (4, [s("A4", "South"), s("B4", "  Lina"), n("C4", "9"), s("D4", "1,200")]),
        (6, [s("A6", "NORTH"), s("B6", "Sara"), n("C6", "11"), s("D6", "$950")]),
        (7, [s("A7", "South"), s("B7", "Karim"), n("C7", "7"), n("D7", "700")]),
        (8, [s("A8", "North"), s("B8", "Amina"), n("C8", "12"), n("D8", "1200")]),
    ]
    sheet_rows = "".join(f'<row r="{r}">{"".join(cells)}</row>' for r, cells in rows)
    sheet1 = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        f"<sheetData>{sheet_rows}</sheetData></worksheet>"
    )
    workbook = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        '<sheets><sheet name="Sales" sheetId="1" r:id="rId1"/></sheets></workbook>'
    )
    wb_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>'
        '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
        "</Relationships>"
    )
    styles = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        '<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>'
        '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>'
        '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
        '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
        '<cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>'
        '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>'
        "</styleSheet>"
    )
    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>'
        "</Types>"
    )
    rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>'
        "</Relationships>"
    )
    write_zip(
        OUT / "messy_table.xlsx",
        [
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", rels),
            ("xl/workbook.xml", workbook),
            ("xl/_rels/workbook.xml.rels", wb_rels),
            ("xl/styles.xml", styles),
            ("xl/worksheets/sheet1.xml", sheet1),
        ],
    )


REPORT_PAGES = [
    [
        "Northwind Clinics - Quarterly Operations Report, Q3 2026",
        "",
        "Summary",
        "Patient visits grew 14% to 48,200 across the six clinics.",
        "Average wait time fell from 31 to 22 minutes after the new triage desk opened.",
        "The Dubai Marina clinic reached full staffing in August.",
    ],
    [
        "Finances",
        "",
        "Operating revenue was AED 6.4m, 9% above plan.",
        "Staff costs rose to AED 3.1m because of the two new night shifts.",
        "Equipment spending was held at AED 0.4m, the same as Q2.",
    ],
    [
        "Risks and next quarter",
        "",
        "Two clinics still run on the old booking system; migration is planned for November.",
        "Nurse turnover reached 11% and is the main staffing risk.",
        "Next quarter the board is asked to approve a fourth triage desk.",
    ],
]


def pdf_report() -> None:
    def pdf_text(t: str) -> str:
        return t.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")

    objects: list[bytes] = []
    n_pages = len(REPORT_PAGES)
    font_id = 3 + 2 * n_pages
    page_ids = [3 + 2 * i for i in range(n_pages)]
    objects.append(b"<< /Type /Catalog /Pages 2 0 R >>")
    kids = " ".join(f"{i} 0 R" for i in page_ids)
    objects.append(f"<< /Type /Pages /Kids [{kids}] /Count {n_pages} >>".encode())
    for i, lines in enumerate(REPORT_PAGES):
        content = ["BT", "/F1 12 Tf", "16 TL", "72 720 Td"]
        for j, line in enumerate(lines):
            if j:
                content.append("T*")
            if line:
                content.append(f"({pdf_text(line)}) Tj")
        content.append("ET")
        stream = "\n".join(content).encode("latin-1")
        objects.append(
            f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 {font_id} 0 R >> >> /Contents {page_ids[i] + 1} 0 R >>".encode()
        )
        objects.append(b"<< /Length %d >>\nstream\n" % len(stream) + stream + b"\nendstream")
    objects.append(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for i, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += f"{i} 0 obj\n".encode() + body + b"\nendobj\n"
    xref = len(out)
    out += f"xref\n0 {len(objects) + 1}\n".encode()
    out += b"0000000000 65535 f \n"
    for off in offsets:
        out += f"{off:010d} 00000 n \n".encode()
    out += f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    (OUT / "quarterly_report.pdf").write_bytes(bytes(out))


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    xlsx()
    docx()
    pptx()
    dcf_xlsx()
    styles_docx()
    messy_xlsx()
    pdf_report()
    for name in ("formula_errors.xlsx", "letter_template.docx", "board_deck.pptx", "dcf_model.xlsx", "report_styles.docx", "messy_table.xlsx", "quarterly_report.pdf"):
        path = OUT / name
        import hashlib

        print(f"{name}: {path.stat().st_size} bytes sha256={hashlib.sha256(path.read_bytes()).hexdigest()}")
