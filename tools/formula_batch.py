#!/usr/bin/env python3
"""Formula-expansion batches: Excel is the oracle, the pinned engine is the subject.

    tools/formula_batch.py oracle  01   # run the batch's cases in real Excel
    tools/formula_batch.py probe   01   # run the same cases on the pinned engine
    tools/formula_batch.py compare 01   # show agreements / deviations
    tools/formula_batch.py build   01   # write agreeing functions into the
                                        # corpus + 22_Formula_Coverage.json
    tools/formula_batch.py all     01   # oracle + probe + compare + build

A function enters the corpus only when EVERY one of its cases gives the same
answer in Excel and in the engine ("probed-then-fixtured"); the rest are
listed as deferred in the group's notes. Expected values come from Excel, never
from the engine. Needs Microsoft Excel and an unlocked screen for `oracle`.
"""
import importlib.util
import json
import math
import os
import pathlib
import re
import subprocess
import sys
import time
import zipfile

REPO = pathlib.Path(__file__).resolve().parent.parent
CORPUS = REPO / "core/harbor_formula/fixtures/formula_corpus.json"
COVERAGE = REPO / "22_Formula_Coverage.json"
WORK = pathlib.Path(os.environ.get("FORMULA_WORK", "/tmp/formula_batch"))
ERRORS = {"#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A", "#NULL!", "#SPILL!", "#CALC!"}
# Functions Excel rejects (corrupt-workbook dialog) unless the file formula carries a prefix.
PREFIX_HINT = {"FILTER": "_xlfn._xlws.", "SORT": "_xlfn._xlws."}
ARRAY_MARK = "\x00ARRAY\x00"  # prefix on an oracle formula that must be written as an array formula
FORMULA_CELL = (1000, 1000)  # where the engine's qualifier puts the formula
FORMULA_REF = "ALL1000"  # the same cell in A1 notation (col 1000)


# --------------------------------------------------------------------- batch
def load_batch(bid):
    path = REPO / "tools/formula_batches" / f"batch{bid}.py"
    spec = importlib.util.spec_from_file_location(f"batch{bid}", path)
    mod = importlib.util.module_from_spec(spec)
    sys.path.insert(0, str(path.parent))
    spec.loader.exec_module(mod)
    return mod.BATCH


def col_num(letters):
    n = 0
    for ch in letters:
        n = n * 26 + ord(ch) - 64
    return n


def col_name(n):
    s = ""
    while n:
        n, r = divmod(n - 1, 26)
        s = chr(65 + r) + s
    return s


def parse_ref(ref):
    sheet = "Sheet1"
    if "!" in ref:
        sheet, ref = ref.split("!")
    m = re.fullmatch(r"([A-Z]+)(\d+)", ref)
    return sheet, int(m.group(2)), col_num(m.group(1))


def enc(v):
    if v is None:
        return {}
    if isinstance(v, bool):
        return {"b": v}
    if isinstance(v, (int, float)):
        return {"n": v}
    if isinstance(v, str) and v in ERRORS:
        return {"e": v}
    return {"t": v}


def cells_json(cells):
    out = []
    for ref, v in cells:
        sheet, r, c = parse_ref(ref)
        out.append([sheet, r, c, enc(v)])
    return out


def expand(batch):
    """Flat list of corpus-shaped cases (no `expect`)."""
    group = f"engine_expansion_b{batch['id']}"
    cases = []
    merged = {}
    for fn, fcases in batch["functions"]:  # the same function may be listed twice
        merged.setdefault(fn, []).extend(fcases)
    for fn, fcases in merged.items():
        slug = fn.lower().replace(".", "")
        for k, c in enumerate(fcases, 1):
            case = {
                "id": f"b{batch['id']}-{slug}-{k}",
                "group": group,
                "target": fn,
                "dimension": "recalc_edit" if c["edits"] else c["dim"],
                "cells": cells_json(c["cells"]),
                "formula": c["formula"],
                "note": c["note"],
                "tol": c["tol"],
                "edit_cells": [cells_json(e) for e in c["edits"]],
                "xl": c.get("xl"),
                "array": c.get("array", False),
            }
            cases.append(case)
    return cases


# ------------------------------------------------------------------- oracle
def xml_esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def cell_xml(row, col, v):
    ref = f"{col_name(col)}{row}"
    if "n" in v:
        return f'<c r="{ref}"><v>{repr(float(v["n"])) if not float(v["n"]).is_integer() else int(v["n"])}</v></c>'
    if "t" in v:
        return f'<c r="{ref}" t="inlineStr"><is><t xml:space="preserve">{xml_esc(v["t"])}</t></is></c>'
    if "b" in v:
        return f'<c r="{ref}" t="b"><v>{1 if v["b"] else 0}</v></c>'
    if "e" in v:
        return f'<c r="{ref}" t="e"><v>{v["e"]}</v></c>'
    return ""


def sheet_xml(cells, formula):
    rows = {}
    for r, c, v in cells:
        rows.setdefault(r, []).append((c, v))
    fr, fc = FORMULA_CELL
    rows.setdefault(fr, []).append((fc, {"f": formula}))
    body = ""
    for r in sorted(rows):
        cs = ""
        for c, v in sorted(rows[r], key=lambda x: x[0]):
            if "f" in v:
                ftxt = v["f"]
                if ftxt.startswith(ARRAY_MARK):
                    cs += f'<c r="{col_name(c)}{r}"><f t="array" ref="{col_name(c)}{r}">{xml_esc(ftxt[len(ARRAY_MARK):])}</f></c>'
                else:
                    cs += f'<c r="{col_name(c)}{r}"><f>{xml_esc(ftxt)}</f></c>'
            else:
                cs += cell_xml(r, c, v)
        body += f'<row r="{r}">{cs}</row>'
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>'
        + body
        + "</sheetData></worksheet>"
    )


def build_xlsx(path, sheets):
    """sheets: [(name, [(row,col,value)], formula_without_equals)]"""
    n = len(sheets)
    ct = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        + "".join(
            f'<Override PartName="/xl/worksheets/sheet{i + 1}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
            for i in range(n)
        )
        + "</Types>"
    )
    rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>'
    )
    wb = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>'
        + "".join(f'<sheet name="{nm}" sheetId="{i + 1}" r:id="rId{i + 1}"/>' for i, (nm, _, _) in enumerate(sheets))
        + '</sheets><calcPr fullCalcOnLoad="1"/></workbook>'
    )
    wbr = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        + "".join(
            f'<Relationship Id="rId{i + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{i + 1}.xml"/>'
            for i in range(n)
        )
        + "</Relationships>"
    )
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", ct)
        z.writestr("_rels/.rels", rels)
        z.writestr("xl/workbook.xml", wb)
        z.writestr("xl/_rels/workbook.xml.rels", wbr)
        for i, (_, cells, f) in enumerate(sheets):
            z.writestr(f"xl/worksheets/sheet{i + 1}.xml", sheet_xml(cells, f))


def sheet_names(idx):
    return {"Sheet1": f"s{idx}", "Sheet2": f"s{idx}b", "Sheet3": f"s{idx}c"}


def to_excel_formula(formula, idx, target, prefix):
    f = formula[1:] if formula.startswith("=") else formula
    for src, dst in sheet_names(idx).items():
        f = f.replace(f"'{src}'!", f"{dst}!").replace(f"{src}!", f"{dst}!")
    if prefix:
        f = re.sub(rf"(?<![A-Za-z0-9_.]){re.escape(target)}\(", f"{prefix}{target}(", f)
    return f


READ_SCRIPT = f'''
tell application "Microsoft Excel"
  set wb to active workbook
  set out to ""
  repeat with i from 1 to (count of worksheets of wb)
    set ws to worksheet i of wb
    set r to range "{FORMULA_REF}" of ws
    set v to value of r
    set cls to (class of v) as text
    if cls is "real" or cls is "integer" or cls is "number" then set number format of r to "0.000000000000000E+00"
    set sv to string value of r
    set vt to ""
    if cls is "boolean" or cls is "date" then set vt to (v as text)
    set out to out & (name of ws) & "§§" & cls & "§§" & sv & "§§" & vt & linefeed
  end repeat
  return out
end tell
'''

CLOSE_SCRIPT = 'tell application "Microsoft Excel" to close active workbook saving no'


def osascript(src, name, wait=90):
    WORK.mkdir(parents=True, exist_ok=True)
    scr = WORK / f"{name}.applescript"
    out = WORK / f"{name}.out"
    scr.write_text(src)
    if out.exists():
        out.unlink()
    proc = subprocess.Popen(
        f'osascript "{scr}" > "{out}" 2>&1', shell=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    )
    t0 = time.time()
    while time.time() - t0 < wait:
        time.sleep(1.5)
        if proc.poll() is not None:
            break
    else:
        proc.kill()
        subprocess.run(["pkill", "osascript"], check=False)
        raise SystemExit(f"osascript {name} timed out (is the screen locked / Excel stuck?)")
    return out.read_text()


def excel_run(path):
    subprocess.run(["open", "-a", "Microsoft Excel", str(path)], check=True)
    for _ in range(60):
        time.sleep(2)
        probe = osascript('tell application "Microsoft Excel" to return (count of workbooks) as text', "probe", 20).strip()
        if probe.isdigit() and int(probe) >= 1:
            break
    time.sleep(2)
    if osascript('tell application "Microsoft Excel" to return (count of workbooks) as text', "probe", 20).strip() == "0":
        # "We found a problem with some content": dismiss it and fail loudly.
        osascript('tell application "System Events" to tell process "Microsoft Excel" to click button "No" of window 1', "dismiss", 20)
        raise SystemExit(f"Excel rejected {path.name}: a formula in it is invalid in file form (needs an _xlfn. prefix?)")
    raw = osascript(READ_SCRIPT, "read", 120)
    osascript(CLOSE_SCRIPT, "close", 30)
    return raw


def parse_excel(raw):
    res = {}
    for line in raw.splitlines():
        parts = line.split("§§")
        if len(parts) < 4:
            continue
        name, cls, sv, vt = parts[0], parts[1], parts[2], parts[3]
        if sv in ERRORS:
            res[name] = {"e": sv}
        elif cls == "boolean":
            res[name] = {"b": vt.strip().lower() == "true"}
        elif cls in ("real", "integer", "number"):
            res[name] = {"n": float(sv.replace(",", "."))}
        elif cls == "date":
            res[name] = {"date": sv}
        elif cls == "missing value" and sv == "":
            res[name] = {}
        else:
            res[name] = {"t": sv}
    return res


def round_sheet_map(idx, j):
    suffix = f"e{j}" if j else ""
    return {k: v + suffix for k, v in sheet_names(idx).items()}


def workbook_sheets(cases, ids, prefixed):
    """Sheets for the given case ids (one set per evaluation round)."""
    sheets, index = [], []
    for i, c in enumerate(cases):
        if c["id"] not in ids:
            continue
        acc = []
        for j, add in enumerate([c["cells"]] + c["edit_cells"]):
            acc = acc + add
            names = round_sheet_map(i, j)
            f = to_excel_formula(c.get("xl") or c["formula"], i, c["target"], "" if c.get("xl") else prefixed.get(c["id"], ""))
            if c.get("array"):
                f = ARRAY_MARK + f
            for src, dst in sheet_names(i).items():
                f = f.replace(f"{dst}!", f"{names[src]}!")
            by_sheet = {}
            for sh, r, cc, val in acc:
                by_sheet.setdefault(names.get(sh, sh), []).append((r, cc, val))
            primary = names["Sheet1"]
            sheets.append((primary, by_sheet.pop(primary, []), f))
            index.append((c["id"], j))
            for other, oc in by_sheet.items():
                sheets.append((other, oc, "0"))
                index.append((None, None))
    return sheets, index


def cmd_oracle(bid):
    batch = load_batch(bid)
    cases = expand(batch)
    results = {}
    prefixed = {c["id"]: PREFIX_HINT[c["target"]] for c in cases if c["target"] in PREFIX_HINT}
    todo = {c["id"] for c in cases}
    prefixes = ["", "_xlfn.", "_xlfn._xlws."]
    for attempt in range(len(prefixes)):
        sheets, index = workbook_sheets(cases, todo, prefixed)
        WORK.mkdir(parents=True, exist_ok=True)
        path = WORK / f"oracle{bid}_{attempt}.xlsx"
        build_xlsx(path, sheets)
        got = parse_excel(excel_run(path))
        for (cid, j), (name, _, _) in zip(index, sheets):
            if cid is not None:
                results.setdefault(cid, {})[str(j)] = got.get(name)
        # newer functions need an _xlfn. (or _xlfn._xlws.) prefix in file formulas
        todo = {cid for cid in todo if results[cid].get("0") == {"e": "#NAME?"}}
        if not todo or attempt == len(prefixes) - 1:
            break
        for cid in todo:
            prefixed[cid] = prefixes[attempt + 1]
    out = WORK / f"oracle{bid}.json"
    out.write_text(json.dumps({"results": results, "prefixed": prefixed}, indent=1))
    print(f"oracle: {len(results)} cases -> {out}")


# -------------------------------------------------------------------- probe
def cmd_probe(bid):
    batch = load_batch(bid)
    cases = expand(batch)
    WORK.mkdir(parents=True, exist_ok=True)
    rounds = []
    for c in cases:
        base = {"id": c["id"] + "#0", "cells": c["cells"], "formula": c["formula"]}
        rounds.append(base)
        acc = list(c["cells"])
        for j, add in enumerate(c["edit_cells"], 1):
            acc = acc + add
            rounds.append({"id": f'{c["id"]}#{j}', "cells": acc, "formula": c["formula"]})
    p = WORK / f"probe{bid}_in.json"
    p.write_text(json.dumps({"cases": rounds}))
    r = subprocess.run(
        ["cargo", "run", "-q", "-p", "harbor_formula", "--example", "probe_cases", "--", str(p)],
        cwd=REPO / "core",
        capture_output=True,
        text=True,
        env={**os.environ, "PATH": f"{pathlib.Path.home()}/.cargo/bin:" + os.environ["PATH"]},
    )
    if r.returncode != 0:
        print(r.stderr[-2000:])
        raise SystemExit("probe failed")
    res = {}
    for line in r.stdout.splitlines():
        if line.startswith("{"):
            o = json.loads(line)
            cid, j = o["id"].split("#")
            res.setdefault(cid, {})[int(j)] = o["engine"]
    (WORK / f"probe{bid}.json").write_text(json.dumps(res, indent=1))
    print(f"probe: {len(res)} cases")


# ------------------------------------------------------------------ compare
def same(excel, engine):
    if excel is None or engine is None:
        return False, None
    if "panic" in engine:
        return False, None
    if "n" in excel and "n" in engine:
        a, b = excel["n"], engine["n"]
        d = abs(a - b)
        if d <= 1e-9 * max(1.0, abs(a)):
            return True, None
        if d <= 1e-6 * max(1.0, abs(a)):
            return True, 1e-6
        return False, None
    return excel == engine, None


def load_results(bid):
    o = json.loads((WORK / f"oracle{bid}.json").read_text())["results"]
    e = json.loads((WORK / f"probe{bid}.json").read_text())
    return o, e


def analyse(bid):
    batch = load_batch(bid)
    cases = expand(batch)
    o, e = load_results(bid)
    by_fn = {}
    for c in cases:
        rounds = o.get(c["id"], {})
        eng = e.get(c["id"], {})
        ok, tol, detail = True, None, []
        n_rounds = 1 + len(c["edit_cells"])
        for j in range(n_rounds):
            xl = rounds.get(str(j)) if str(j) in rounds else rounds.get(j)
            en = eng.get(str(j), eng.get(j))
            if xl is None or "date" in (xl or {}):
                ok = False
                detail.append(f"round {j}: excel unusable {xl}")
                continue
            s, t = same(xl, en)
            if not s:
                ok = False
                detail.append(f"round {j}: excel {xl} engine {en}")
            if t:
                tol = max(tol or 0, t)
        by_fn.setdefault(c["target"], []).append((c, ok, tol, detail, rounds))
    return batch, by_fn


def cmd_compare(bid):
    batch, by_fn = analyse(bid)
    good, deferred = [], []
    for fn, rows in by_fn.items():
        if all(r[1] for r in rows):
            good.append(fn)
        else:
            deferred.append(fn)
            for c, ok, _, detail, _ in rows:
                if not ok:
                    print(f"  DEVIATION {c['id']}  {c['formula']}\n      " + "\n      ".join(detail))
    print(f"agree: {len(good)}  deferred: {len(deferred)}")
    print("deferred:", " ".join(deferred))


# -------------------------------------------------------------------- build
def case_json(c, rounds, tol):
    def val(r):
        return dict(r)

    first = rounds.get("0") or rounds.get(0)
    expect = val(first)
    if tol:
        expect["tol"] = tol
    out = {
        "id": c["id"],
        "group": c["group"],
        "target": c["target"],
        "dimension": c["dimension"],
        "cells": c["cells"],
        "formula": c["formula"],
        "expect": expect,
    }
    if c["edit_cells"]:
        out["edits"] = []
        for j, add in enumerate(c["edit_cells"], 1):
            ex = val(rounds.get(str(j)) or rounds.get(j))
            if tol:
                ex["tol"] = tol
            out["edits"].append({"cells": add, "expect": ex})
    if c["note"]:
        out["notes"] = c["note"]
    return out


def cmd_build(bid):
    batch, by_fn = analyse(bid)
    unusable = [r[0]["id"] for rows in by_fn.values() for r in rows if any("excel unusable" in d for d in r[3])]
    if unusable:
        raise SystemExit(f"build refused: Excel gave no usable answer for {len(unusable)} cases (e.g. {unusable[:3]}); rerun oracle")
    group = f"engine_expansion_b{bid}"
    good = [fn for fn, rows in by_fn.items() if all(r[1] for r in rows)]
    deferred = [fn for fn in by_fn if fn not in good]
    corpus = json.loads(CORPUS.read_text())
    new = []
    for fn in good:
        for c, ok, tol, _, rounds in by_fn[fn]:
            new.append(case_json(c, rounds, tol))
    at = next((i for i, c in enumerate(corpus["cases"]) if c.get("group") == group), len(corpus["cases"]))
    rest = [c for c in corpus["cases"] if c.get("group") != group]
    corpus["cases"] = rest[:at] + new + rest[at:]
    text = json.dumps(corpus, indent=1)
    CORPUS.write_text(text + ("\n" if CORPUS.read_text().endswith("\n") else ""))
    cov = json.loads(COVERAGE.read_text())
    note = (
        f"Formula expansion batch {bid}: {batch['title']}. Expected values taken from Microsoft Excel "
        f"(not from the engine); a function is included only when every one of its cases agrees between Excel and the pinned engine."
    )
    # Functions already targeted by another group get extra cases here without being re-listed.
    known = {f["name"] for g in cov["target_groups"] if g.get("group") != group for f in g["functions"]}
    entry = {
        "group": group,
        "qualification_status": "REQUIRED_UNQUALIFIED",
        "notes": note,
        "functions": [{"name": fn, "status": "REQUIRED_UNQUALIFIED"} for fn in good if fn not in known],
    }
    if deferred:
        entry["deferred_engine_deviations"] = [
            {
                "name": fn,
                "case_formulas": [r[0]["formula"] for r in by_fn[fn] if not r[1]],
                "detail": next(d for r in by_fn[fn] if not r[1] for d in r[3]),
            }
            for fn in sorted(deferred)
        ]
    slot = next((i for i, g in enumerate(cov["target_groups"]) if g.get("group") == group), None)
    if slot is None:
        cov["target_groups"].append(entry)
    else:
        cov["target_groups"][slot] = entry
    ctext = json.dumps(cov, indent=2)
    COVERAGE.write_text(ctext + ("\n" if COVERAGE.read_text().endswith("\n") else ""))
    print(f"build: +{len(good)} functions, +{len(new)} cases, deferred {len(deferred)}")


COMMANDS = {"oracle": cmd_oracle, "probe": cmd_probe, "compare": cmd_compare, "build": cmd_build}

if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] not in (*COMMANDS, "all"):
        raise SystemExit(__doc__)
    cmd, bid = sys.argv[1], sys.argv[2]
    if cmd == "all":
        for name in ("oracle", "probe", "compare", "build"):
            COMMANDS[name](bid)
    else:
        COMMANDS[cmd](bid)
