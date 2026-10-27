"""Batch 10 — Bessel functions and core financial functions (25)."""
from dsl import C

FLOWS = [("A1", -70000), ("A2", 12000), ("A3", 15000), ("A4", 18000), ("A5", 21000)]
NPVF = [("A1", -10000), ("A2", 3000), ("A3", 4200), ("A4", 6800)]
POS = [("A1", 100), ("A2", 200), ("A3", 300)]

BATCH = {
    "id": "10",
    "title": "Bessel functions and core financial functions",
    "functions": [
        ("BESSELI", [C("=BESSELI(1.5,1)"), C("=BESSELI(0,0)"), C("=BESSELI(1.5,-1)", dim="errors")]),
        ("BESSELJ", [C("=BESSELJ(1.9,2)"), C("=BESSELJ(0,0)"), C("=BESSELJ(1.9,-1)", dim="errors")]),
        ("BESSELK", [C("=BESSELK(1.5,1)"), C("=BESSELK(2,0)"), C("=BESSELK(0,1)", dim="errors")]),
        ("BESSELY", [C("=BESSELY(2.5,1)"), C("=BESSELY(1,0)"), C("=BESSELY(0,1)", dim="errors")]),
        ("FV", [C("=FV(0.06/12,10,-200,-500,1)"), C("=FV(0.1,5,-100)"), C("=FV(0,10,-100)", note="zero rate is a plain sum")]),
        ("PV", [C("=PV(0.08/12,240,500)"), C("=PV(0,10,-100)"), C("=PV(0.05,10,-100,0,1)", note="payments at the start of the period")]),
        ("PMT", [C("=PMT(0.08/12,10,10000)"), C("=PMT(0,10,1000)"), C("=PMT(0.08/12,10,10000,0,1)", note="payments at the start of the period")]),
        ("NPER", [C("=NPER(0.12/12,-100,-1000,10000,1)"), C("=NPER(0.1,-100,1000)"), C("=NPER(0,0,100)", dim="errors")]),
        ("RATE", [C("=RATE(48,-200,8000)"), C("=RATE(10,-100,800,0,1,0.05)", note="with a starting guess"), C("=RATE(10,100,800)", dim="errors", note="payments and loan the same sign never converge")]),
        ("IPMT", [C("=IPMT(0.1/12,1,36,8000)"), C("=IPMT(0.1/12,36,36,8000)"), C("=IPMT(0.1/12,2,36,8000,0,1)", note="payments at the start of the period"), C("=IPMT(0.1/12,1,36,8000,0,1)", note="no interest accrues before the first start-of-period payment"), C("=IPMT(0.1/12,40,36,8000)", dim="errors")]),
        ("PPMT", [C("=PPMT(0.1/12,1,36,8000)"), C("=PPMT(0.1/12,36,36,8000)"), C("=PPMT(0.1/12,2,36,8000,0,1)", note="payments at the start of the period"), C("=PPMT(0.1/12,40,36,8000)", dim="errors")]),
        ("ISPMT", [C("=ISPMT(0.1,1,4,4000000)"), C("=ISPMT(0.1,0,4,4000000)"), C("=ISPMT(0.1,1,0,4000000)", dim="errors")]),
        ("NPV", [C("=NPV(0.1,A1:A4)", NPVF), C("=NPV(0.1,-10000,3000,4200,6800)"), C("=NPV(0.1,A1:A4)", [("A1", -10000), ("A2", "x"), ("A3", 4200), ("A4", 6800)], note="text in a range is skipped")]),
        ("IRR", [C("=IRR(A1:A5)", FLOWS), C("=IRR(A1:A5,0.1)", FLOWS, note="with a guess"), C("=IRR(A1:A3)", POS, dim="errors", note="all positive flows")]),
        ("MIRR", [C("=MIRR(A1:A5,0.1,0.12)", FLOWS), C("=MIRR(A1:A5,0.08,0.08)", FLOWS), C("=MIRR(A1:A3,0.1,0.12)", POS, dim="errors")]),
        ("SLN", [C("=SLN(30000,7500,10)"), C("=SLN(1000,0,5)"), C("=SLN(1000,100,0)", dim="errors")]),
        ("SYD", [C("=SYD(30000,7500,10,1)"), C("=SYD(30000,7500,10,10)"), C("=SYD(30000,7500,10,11)", dim="errors")]),
        ("DB", [C("=DB(1000000,100000,6,1,7)"), C("=DB(1000000,100000,6,2,7)"), C("=DB(1000000,100000,6,8,7)", dim="errors")]),
        ("DDB", [C("=DDB(2400,300,10,1)"), C("=DDB(2400,300,10,2,1.5)", note="factor 1.5"), C("=DDB(2400,300,10,11)", dim="errors")]),
        ("EFFECT", [C("=EFFECT(0.0525,4)"), C("=EFFECT(0.1,1)"), C("=EFFECT(0.1,0)", dim="errors")]),
        ("NOMINAL", [C("=NOMINAL(0.053543,4)"), C("=NOMINAL(0.1,1)"), C("=NOMINAL(0.1,0)", dim="errors")]),
        ("PDURATION", [C("=PDURATION(0.025,2000,2200)"), C("=PDURATION(0.1,100,200)"), C("=PDURATION(0,100,200)", dim="errors")]),
        ("RRI", [C("=RRI(96,10000,11000)"), C("=RRI(5,100,200)"), C("=RRI(0,100,200)", dim="errors")]),
        ("DOLLARDE", [C("=DOLLARDE(1.02,16)"), C("=DOLLARDE(1.1,32)"), C("=DOLLARDE(1.02,0)", dim="errors")]),
        ("DOLLARFR", [C("=DOLLARFR(1.125,16)"), C("=DOLLARFR(1.5,32)"), C("=DOLLARFR(1.125,0)", dim="errors")]),
    ],
}
