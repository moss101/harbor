"""Batch 07 — remaining statistics (regression, ranks) and date/time functions (22)."""
from dsl import C

X5 = [(f"B{i + 1}", i + 1) for i in range(5)]
Y5 = [(f"A{i + 1}", v) for i, v in enumerate([2, 4, 5, 4, 5])]
XY = X5 + Y5
GROW = [(f"A{i + 1}", v) for i, v in enumerate([2, 4, 8, 16, 32])] + X5
D10 = [(f"A{i + 1}", v) for i, v in enumerate([3, 4, 5, 2, 3, 4, 5, 6, 4, 7])]
MODES = [(f"A{i + 1}", v) for i, v in enumerate([1, 2, 2, 3, 3, 4])]
FIVE = [(f"A{i + 1}", v) for i, v in enumerate([1, 2, 3, 4, 5])]
TIES = [("A1", 3), ("A2", 3), ("A3", 5)]
BINS = [("B1", 2), ("B2", 4)]

BATCH = {
    "id": "07",
    "title": "Regression and ranking statistics; date and time functions",
    "functions": [
        ("FORECAST.LINEAR", [C("=FORECAST.LINEAR(6,A1:A5,B1:B5)", XY), C("=FORECAST.LINEAR(0,A1:A5,B1:B5)", XY), C("=FORECAST.LINEAR(6,A1:A5,B1:B4)", XY, dim="errors")]),
        ("QUARTILE.INC", [C("=QUARTILE.INC(A1:A10,1)", D10), C("=QUARTILE.INC(A1:A10,3)", D10), C("=QUARTILE.INC(A1:A10,0)", D10, note="the minimum"), C("=QUARTILE.INC(A1:A10,5)", D10, dim="errors")]),
        ("RANK.AVG", [C("=RANK.AVG(3,A1:A3,1)", TIES, note="tied values share the average rank"), C("=RANK.AVG(5,A1:A3)", TIES, note="descending by default"), C("=RANK.AVG(9,A1:A3)", TIES, dim="errors")]),
        ("FREQUENCY", [C("=INDEX(FREQUENCY(A1:A5,B1:B2),1)", FIVE + BINS, note="values up to the first bin"), C("=INDEX(FREQUENCY(A1:A5,B1:B2),2)", FIVE + BINS), C("=INDEX(FREQUENCY(A1:A5,B1:B2),3)", FIVE + BINS, note="values above the last bin")]),
        ("GROWTH", [C("=GROWTH(A1:A5,B1:B5,E1)", GROW + [("E1", 6)]), C("=INDEX(GROWTH(A1:A5,B1:B5),3)", GROW), C("=GROWTH(A1:A5,B1:B4,E1)", GROW + [("E1", 6)], dim="errors")]),
        ("LINEST", [C("=INDEX(LINEST(A1:A5,B1:B5),1,1)", XY, note="slope"), C("=INDEX(LINEST(A1:A5,B1:B5),1,2)", XY, note="intercept"), C("=INDEX(LINEST(A1:A5,B1:B5,TRUE,TRUE),3,1)", XY, note="r squared")]),
        ("LOGEST", [C("=INDEX(LOGEST(A1:A5,B1:B5),1,1)", GROW, note="growth base"), C("=INDEX(LOGEST(A1:A5,B1:B5),1,2)", GROW, note="scale")]),
        ("MODE.MULT", [C("=INDEX(MODE.MULT(A1:A6),1)", MODES), C("=INDEX(MODE.MULT(A1:A6),2)", MODES), C("=INDEX(MODE.MULT(A1:A6),3)", MODES, dim="errors", note="only two modes")]),
        ("TREND", [C("=TREND(A1:A5,B1:B5,E1)", XY + [("E1", 6)]), C("=INDEX(TREND(A1:A5,B1:B5),1)", XY), C("=TREND(A1:A5,B1:B4,E1)", XY + [("E1", 6)], dim="errors")]),
        ("DATEDIF", [C('=DATEDIF(DATE(2020,1,15),DATE(2026,9,12),"Y")'), C('=DATEDIF(DATE(2020,1,15),DATE(2026,9,12),"M")'), C('=DATEDIF(DATE(2020,1,15),DATE(2026,9,12),"D")'), C('=DATEDIF(DATE(2020,1,15),DATE(2026,9,12),"YM")'), C('=DATEDIF(DATE(2020,1,15),DATE(2026,9,12),"MD")'), C('=DATEDIF(DATE(2026,9,12),DATE(2020,1,15),"Y")', dim="errors", note="start after end")]),
        ("DATEVALUE", [C('=DATEVALUE("2026-09-12")'), C('=DATEVALUE("2024-02-29")'), C('=DATEVALUE("abc")', dim="errors")]),
        ("DAYS360", [C("=DAYS360(DATE(2026,1,30),DATE(2026,2,28))"), C("=DAYS360(DATE(2026,1,31),DATE(2026,3,31),TRUE)", note="European method"), C("=DAYS360(DATE(2026,1,1),DATE(2026,12,31))")]),
        ("HOUR", [C("=HOUR(0.75)"), C("=HOUR(TIME(13,45,30))"), C("=HOUR(-1)", dim="errors")]),
        ("MINUTE", [C("=MINUTE(TIME(13,45,30))"), C("=MINUTE(0.75625)"), C("=MINUTE(-1)", dim="errors")]),
        ("SECOND", [C("=SECOND(TIME(13,45,30))"), C("=SECOND(0.5)"), C("=SECOND(-1)", dim="errors")]),
        ("ISOWEEKNUM", [C("=ISOWEEKNUM(DATE(2026,1,1))"), C("=ISOWEEKNUM(DATE(2027,1,1))", note="belongs to week 53 of 2026"), C("=ISOWEEKNUM(DATE(2026,9,12))")]),
        ("NETWORKDAYS.INTL", [C("=NETWORKDAYS.INTL(DATE(2026,9,1),DATE(2026,9,30))"), C('=NETWORKDAYS.INTL(DATE(2026,9,1),DATE(2026,9,30),"0000011")', note="Saturday and Sunday off"), C("=NETWORKDAYS.INTL(DATE(2026,9,1),DATE(2026,9,30),7)", note="weekend code 7: Friday and Saturday"), C("=NETWORKDAYS.INTL(DATE(2026,9,1),DATE(2026,9,30),1,A1:A2)", [("A1", 46266), ("A2", 46267)], note="two holidays")]),
        ("TIME", [C("=TIME(13,45,30)"), C("=TIME(25,0,0)", note="hours wrap past 24"), C("=TIME(0,90,0)"), C("=TIME(-1,0,0)", dim="errors")]),
        ("TIMEVALUE", [C('=TIMEVALUE("6:30 PM")'), C('=TIMEVALUE("13:45:30")'), C('=TIMEVALUE("abc")', dim="errors")]),
        ("WEEKNUM", [C("=WEEKNUM(DATE(2026,9,12))"), C("=WEEKNUM(DATE(2026,9,12),2)", note="weeks start on Monday"), C("=WEEKNUM(DATE(2026,1,1))")]),
        ("WORKDAY.INTL", [C("=WORKDAY.INTL(DATE(2026,9,11),5)"), C("=WORKDAY.INTL(DATE(2026,9,11),5,11)", note="weekend code 11: Sunday only"), C("=WORKDAY.INTL(DATE(2026,9,11),-3)"), C("=WORKDAY.INTL(DATE(2026,9,11),5,1,A1)", [("A1", 46278)], note="one holiday")]),
        ("YEARFRAC", [C("=YEARFRAC(DATE(2026,1,1),DATE(2026,7,1))"), C("=YEARFRAC(DATE(2026,1,1),DATE(2026,7,1),1)", note="actual/actual"), C("=YEARFRAC(DATE(2026,1,1),DATE(2026,7,1),3)", note="actual/365"), C("=YEARFRAC(DATE(2026,1,1),DATE(2026,7,1),5)", dim="errors", note="invalid basis")]),
    ],
}
