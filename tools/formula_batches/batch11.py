"""Batch 11 — bond/dated financial functions, text conversion, AGGREGATE (24)."""
from dsl import C

IRRV = [("A1", -10000), ("A2", 2750), ("A3", 4250), ("A4", 3250), ("A5", 2750)]
IRRD = [("B1", 39448), ("B2", 39508), ("B3", 39751), ("B4", 39859), ("B5", 39904)]
IRR = IRRV + IRRD
AGG = [("A1", 4), ("A2", "#DIV/0!"), ("A3", 7), ("A4", 10), ("A5", 2)]
GRID = [("A1", 1), ("B1", 2), ("A2", 3), ("B2", 4)]

BATCH = {
    "id": "11",
    "title": "Dated financial functions, text conversion and AGGREGATE",
    "functions": [
        ("ACCRINT", [C("=ACCRINT(DATE(2026,3,1),DATE(2026,8,31),DATE(2026,5,1),0.1,1000,2,0)"), C("=ACCRINT(DATE(2026,3,1),DATE(2026,8,31),DATE(2026,5,1),0.1,1000,4,1)"), C("=ACCRINT(DATE(2026,3,1),DATE(2026,8,31),DATE(2026,5,1),0,1000,2,0)", dim="errors")]),
        ("ACCRINTM", [C("=ACCRINTM(DATE(2026,4,1),DATE(2026,6,15),0.1,1000,3)"), C("=ACCRINTM(DATE(2026,4,1),DATE(2026,6,15),0.1,1000,0)"), C("=ACCRINTM(DATE(2026,6,15),DATE(2026,4,1),0.1,1000,3)", dim="errors")]),
        ("CUMIPMT", [C("=CUMIPMT(0.09/12,360,125000,1,12,0)"), C("=CUMIPMT(0.09/12,360,125000,13,24,1)"), C("=CUMIPMT(0.09/12,360,125000,12,1,0)", dim="errors")]),
        ("CUMPRINC", [C("=CUMPRINC(0.09/12,360,125000,1,12,0)"), C("=CUMPRINC(0.09/12,360,125000,13,24,1)"), C("=CUMPRINC(0.09/12,360,125000,12,1,0)", dim="errors")]),
        ("PRICE", [C("=PRICE(DATE(2026,2,15),DATE(2036,11,15),0.0575,0.065,100,2,0)"), C("=PRICE(DATE(2026,2,15),DATE(2036,11,15),0.0575,0.065,100,2,1)"), C("=PRICE(DATE(2036,11,15),DATE(2026,2,15),0.0575,0.065,100,2,0)", dim="errors")]),
        ("YIELD", [C("=YIELD(DATE(2026,2,15),DATE(2036,11,15),0.0575,95.04287,100,2,0)"), C("=YIELD(DATE(2026,2,15),DATE(2036,11,15),0.0575,100,100,2,0)", note="priced at par, yield equals the coupon"), C("=YIELD(DATE(2036,11,15),DATE(2026,2,15),0.0575,95,100,2,0)", dim="errors")]),
        ("TBILLEQ", [C("=TBILLEQ(DATE(2026,3,31),DATE(2026,6,1),0.0914)"), C("=TBILLEQ(DATE(2026,3,31),DATE(2026,6,1),0.05)"), C("=TBILLEQ(DATE(2026,3,31),DATE(2027,6,1),0.0914)", dim="errors", note="more than a year")]),
        ("TBILLPRICE", [C("=TBILLPRICE(DATE(2026,3,31),DATE(2026,6,1),0.09)"), C("=TBILLPRICE(DATE(2026,3,31),DATE(2026,6,1),0.05)"), C("=TBILLPRICE(DATE(2026,3,31),DATE(2026,6,1),0)", dim="errors")]),
        ("TBILLYIELD", [C("=TBILLYIELD(DATE(2026,3,31),DATE(2026,6,1),98.45)"), C("=TBILLYIELD(DATE(2026,3,31),DATE(2026,6,1),99)"), C("=TBILLYIELD(DATE(2026,3,31),DATE(2026,6,1),0)", dim="errors")]),
        ("XIRR", [C("=XIRR(A1:A5,B1:B5)", IRR), C("=XIRR(A1:A5,B1:B5,0.2)", IRR, note="with a guess"), C("=XIRR(A1:A5,B1:B4)", IRR, dim="errors")]),
        ("XNPV", [C("=XNPV(0.09,A1:A5,B1:B5)", IRR), C("=XNPV(0,A1:A5,B1:B5)", IRR, note="zero rate is the plain sum"), C("=XNPV(0.09,A1:A5,B1:B4)", IRR, dim="errors")]),
        ("CLEAN", [C('=CLEAN("a"&CHAR(7)&"b")'), C('=CLEAN("plain text")'), C("=CLEAN(A1)", dim="blanks")]),
        ("CODE", [C('=CODE("A")'), C('=CODE("abc")', note="only the first character"), C('=CODE("")', dim="errors")]),
        ("DOLLAR", [C("=DOLLAR(1234.567,2)"), C("=DOLLAR(-1234.567,0)"), C('=DOLLAR("x")', dim="errors")]),
        ("FIXED", [C("=FIXED(1234.567,1)"), C("=FIXED(1234.567,-1,TRUE)", note="no thousands separator"), C('=FIXED("x")', dim="errors")]),
        ("NUMBERVALUE", [C('=NUMBERVALUE("2.500,27",",",".")'), C('=NUMBERVALUE("3.5%")'), C('=NUMBERVALUE("abc")', dim="errors")]),
        ("TEXTAFTER", [C('=TEXTAFTER("Red riding hood\'s","ing ")'), C('=TEXTAFTER("a-b-c-d","-",2)', note="second occurrence"), C('=TEXTAFTER("abc","x")', dim="errors")]),
        ("TEXTBEFORE", [C('=TEXTBEFORE("Red riding hood\'s","ing")'), C('=TEXTBEFORE("a-b-c-d","-",2)'), C('=TEXTBEFORE("abc","x")', dim="errors")]),
        ("UNICHAR", [C("=UNICHAR(65)"), C("=UNICHAR(9731)"), C("=UNICHAR(0)", dim="errors")]),
        ("UNICODE", [C('=UNICODE("A")'), C('=UNICODE("☃")'), C('=UNICODE("")', dim="errors")]),
        ("VALUETOTEXT", [C("=VALUETOTEXT(123)"), C('=VALUETOTEXT("hi")'), C('=VALUETOTEXT("hi",1)', note="strict format quotes text")]),
        ("ARRAYTOTEXT", [C("=ARRAYTOTEXT(A1:B2)", GRID), C("=ARRAYTOTEXT(A1:B2,1)", GRID, note="strict format uses array braces")]),
        ("ASC", [C('=ASC("abc")'), C('=ASC("ＡＢＣ")', note="full-width to half-width")]),
        ("AGGREGATE", [C("=AGGREGATE(9,6,A1:A5)", AGG, note="SUM, ignoring the error"), C("=AGGREGATE(4,6,A1:A5)", AGG, note="MAX, ignoring the error"), C("=AGGREGATE(14,6,A1:A5,2)", AGG, note="second largest"), C("=AGGREGATE(9,0,A1:A5)", AGG, dim="errors", note="errors are not ignored"), C("=AGGREGATE(99,6,A1:A5)", AGG, dim="errors")]),
    ],
}
