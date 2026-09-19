#!/usr/bin/env python3
"""Generate the Office fixtures used by the skill eval suites (decision 0006).

Dependency-free and deterministic (fixed zip timestamps), so the fixture
hashes recorded in eval reports are reproducible:

- fixtures/office/formula_errors.xlsx — a budget sheet with cached
  #DIV/0!, #REF! (dangling sheet reference), #NAME? (unqualified function)
  and #N/A errors next to healthy formulas; exercised by formula-audit.
- fixtures/office/letter_template.docx — a letter with {{name}}, [[ref]],
  $AMOUNT$ and <<sender>> placeholders; exercised by placeholder-fill.
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


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    xlsx()
    docx()
    for name in ("formula_errors.xlsx", "letter_template.docx"):
        path = OUT / name
        import hashlib

        print(f"{name}: {path.stat().st_size} bytes sha256={hashlib.sha256(path.read_bytes()).hexdigest()}")
