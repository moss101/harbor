"""Authoring DSL for formula-expansion batches (see tools/formula_batch.py).

A batch module defines::

    BATCH = {
        "id": "01",
        "title": "Trigonometry and hyperbolics",
        "functions": [
            ("ACOS", [C("=ACOS(0.5)"), C("=ACOS(2)", dim="errors")]),
            ...
        ],
    }

Expected values are NOT written by hand: the tool obtains them from real
Microsoft Excel, runs the same cases on the pinned engine, and only functions
whose every case agrees are added to the corpus.
"""


def C(formula, cells=(), dim="values", note="", tol=None, edits=None, xl=None):
    """One case. `cells` is [(ref, value)] with ref "A1" or "Sheet2!B3";
    value is a number, str (text; "#N/A"-style strings are errors), bool,
    or None (left blank). `edits` is [([(ref, value)...])] applied after the
    first evaluation (recalc_edit dimension). `xl` overrides the formula
    written to the Excel oracle workbook."""
    return {
        "formula": formula,
        "cells": list(cells),
        "dim": dim,
        "note": note,
        "tol": tol,
        "edits": edits or [],
        # Alternative formula text for the Excel oracle only (e.g. LET/LAMBDA
        # parameter names need an _xlpm. prefix inside the xlsx file).
        "xl": xl,
    }
