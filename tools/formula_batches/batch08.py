"""Batch 08 — engineering part 1: base conversion, bits, step functions, erf, COMPLEX, CONVERT (25)."""
from dsl import C

BATCH = {
    "id": "08",
    "title": "Engineering: base conversion, bit operations, error functions, COMPLEX and CONVERT",
    "functions": [
        ("BIN2DEC", [C('=BIN2DEC("1010")'), C('=BIN2DEC("1111111111")', note="ten-bit two's complement is -1"), C('=BIN2DEC("102")', dim="errors")]),
        ("BIN2HEX", [C('=BIN2HEX("1111")'), C('=BIN2HEX("11111111",4)', note="padded to four places"), C('=BIN2HEX("102")', dim="errors")]),
        ("BIN2OCT", [C('=BIN2OCT("1001")'), C('=BIN2OCT("1111111111")'), C('=BIN2OCT("102")', dim="errors")]),
        ("DEC2BIN", [C("=DEC2BIN(10)"), C("=DEC2BIN(10,8)"), C("=DEC2BIN(-1)", note="negative numbers use ten-bit two's complement"), C("=DEC2BIN(512)", dim="errors")]),
        ("DEC2HEX", [C("=DEC2HEX(255)"), C("=DEC2HEX(255,4)"), C("=DEC2HEX(-1)"), C("=DEC2HEX(549755813888)", dim="errors")]),
        ("DEC2OCT", [C("=DEC2OCT(58)"), C("=DEC2OCT(58,5)"), C("=DEC2OCT(-1)"), C("=DEC2OCT(1073741824)", dim="errors")]),
        ("HEX2BIN", [C('=HEX2BIN("F")'), C('=HEX2BIN("FFFFFFFE00")'), C('=HEX2BIN("G")', dim="errors")]),
        ("HEX2DEC", [C('=HEX2DEC("FF")'), C('=HEX2DEC("FFFFFFFFFF")'), C('=HEX2DEC("G")', dim="errors")]),
        ("HEX2OCT", [C('=HEX2OCT("F")'), C('=HEX2OCT("FFFFFFFFFF")'), C('=HEX2OCT("G")', dim="errors")]),
        ("OCT2BIN", [C('=OCT2BIN("7")'), C('=OCT2BIN("7777777000")'), C('=OCT2BIN("8")', dim="errors")]),
        ("OCT2DEC", [C('=OCT2DEC("17")'), C('=OCT2DEC("7777777777")'), C('=OCT2DEC("8")', dim="errors")]),
        ("OCT2HEX", [C('=OCT2HEX("17")'), C('=OCT2HEX("7777777777")'), C('=OCT2HEX("8")', dim="errors")]),
        ("BITAND", [C("=BITAND(13,25)"), C("=BITAND(A1,A2)", [("A1", 12), ("A2", 10)]), C("=BITAND(-1,2)", dim="errors")]),
        ("BITLSHIFT", [C("=BITLSHIFT(4,2)"), C("=BITLSHIFT(1,10)"), C("=BITLSHIFT(4,54)", dim="errors")]),
        ("BITOR", [C("=BITOR(23,10)"), C("=BITOR(0,0)"), C("=BITOR(-1,2)", dim="errors")]),
        ("BITRSHIFT", [C("=BITRSHIFT(13,2)"), C("=BITRSHIFT(1024,10)"), C("=BITRSHIFT(4,54)", dim="errors")]),
        ("BITXOR", [C("=BITXOR(5,3)"), C("=BITXOR(7,7)"), C("=BITXOR(-1,2)", dim="errors")]),
        ("DELTA", [C("=DELTA(5,5)"), C("=DELTA(5,4)"), C("=DELTA(A1)", dim="blanks", note="a blank cell compares as 0 against the default 0")]),
        ("GESTEP", [C("=GESTEP(5,4)"), C("=GESTEP(3,4)"), C("=GESTEP(4,4)", note="equal counts as at least the step")]),
        ("ERF", [C("=ERF(1)"), C("=ERF(0.5,1)", note="two limits integrate between them"), C('=ERF("x")', dim="errors")]),
        ("ERF.PRECISE", [C("=ERF.PRECISE(1)"), C("=ERF.PRECISE(-0.745)"), C('=ERF.PRECISE("x")', dim="errors")]),
        ("ERFC", [C("=ERFC(1)"), C("=ERFC(0)"), C('=ERFC("x")', dim="errors")]),
        ("ERFC.PRECISE", [C("=ERFC.PRECISE(1)"), C("=ERFC.PRECISE(-0.5)"), C('=ERFC.PRECISE("x")', dim="errors")]),
        ("COMPLEX", [C("=COMPLEX(3,4)"), C('=COMPLEX(3,4,"j")'), C("=COMPLEX(0,1)"), C('=COMPLEX(3,4,"k")', dim="errors")]),
        ("CONVERT", [C('=CONVERT(1,"lbm","kg")'), C('=CONVERT(100,"C","F")', note="temperature scales"), C('=CONVERT(1,"mi","km")'), C('=CONVERT(1,"kg","m")', dim="errors", note="incompatible units")]),
    ],
}
