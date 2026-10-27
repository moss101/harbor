"""Batch 14 — LET and LAMBDA (2). Excel's file format needs _xlpm. on parameter names."""
from dsl import C

BATCH = {
    "id": "14",
    "title": "LET and LAMBDA",
    "functions": [
        ("LET", [
            C("=LET(x,5,x*2)", xl="=_xlfn.LET(_xlpm.x,5,_xlpm.x*2)"),
            C("=LET(a,A1,b,A2,a+b)", [("A1", 3), ("A2", 4)], xl="=_xlfn.LET(_xlpm.a,A1,_xlpm.b,A2,_xlpm.a+_xlpm.b)", note="two names, the second computed from cells"),
            C("=LET(x,2,y,x*3,y+x)", xl="=_xlfn.LET(_xlpm.x,2,_xlpm.y,_xlpm.x*3,_xlpm.y+_xlpm.x)", note="a later name uses an earlier one"),
            C("=LET(x,1/0,x)", xl="=_xlfn.LET(_xlpm.x,1/0,_xlpm.x)", dim="errors"),
        ]),
        ("LAMBDA", [
            C("=LAMBDA(x,x+1)(3)", xl="=_xlfn.LAMBDA(_xlpm.x,_xlpm.x+1)(3)"),
            C("=LAMBDA(a,b,a*b)(6,7)", xl="=_xlfn.LAMBDA(_xlpm.a,_xlpm.b,_xlpm.a*_xlpm.b)(6,7)"),
            C("=LAMBDA(x,x/0)(1)", xl="=_xlfn.LAMBDA(_xlpm.x,_xlpm.x/0)(1)", dim="errors"),
        ]),
    ],
}
