"""Batch 05 — probability distributions, part 1 (25)."""
from dsl import C

BATCH = {
    "id": "05",
    "title": "Probability distributions: normal, lognormal, exponential, discrete and gamma",
    "functions": [
        ("FISHER", [C("=FISHER(0.75)"), C("=FISHER(-0.5)"), C("=FISHER(1)", dim="errors")]),
        ("FISHERINV", [C("=FISHERINV(0.972955)"), C("=FISHERINV(0)"), C('=FISHERINV("x")', dim="errors")]),
        ("STANDARDIZE", [C("=STANDARDIZE(42,40,1.5)"), C("=STANDARDIZE(A1,A2,A3)", [("A1", 7), ("A2", 5), ("A3", 2)]), C("=STANDARDIZE(42,40,0)", dim="errors")]),
        ("GAUSS", [C("=GAUSS(2)"), C("=GAUSS(0)"), C('=GAUSS("x")', dim="errors")]),
        ("PHI", [C("=PHI(0.75)"), C("=PHI(0)"), C('=PHI("x")', dim="errors")]),
        ("NORM.DIST", [C("=NORM.DIST(42,40,1.5,TRUE)"), C("=NORM.DIST(42,40,1.5,FALSE)"), C("=NORM.DIST(42,40,0,TRUE)", dim="errors")]),
        ("NORM.INV", [C("=NORM.INV(0.908789,40,1.5)"), C("=NORM.INV(0.5,10,3)"), C("=NORM.INV(1.5,40,1.5)", dim="errors")]),
        ("NORM.S.DIST", [C("=NORM.S.DIST(1.333333,TRUE)"), C("=NORM.S.DIST(0,FALSE)"), C('=NORM.S.DIST("x",TRUE)', dim="errors")]),
        ("NORM.S.INV", [C("=NORM.S.INV(0.9088)"), C("=NORM.S.INV(0.5)"), C("=NORM.S.INV(0)", dim="errors")]),
        ("LOGNORM.DIST", [C("=LOGNORM.DIST(4,3.5,1.2,TRUE)"), C("=LOGNORM.DIST(4,3.5,1.2,FALSE)"), C("=LOGNORM.DIST(0,3.5,1.2,TRUE)", dim="errors")]),
        ("LOGNORM.INV", [C("=LOGNORM.INV(0.0390836,3.5,1.2)"), C("=LOGNORM.INV(0.5,0,1)"), C("=LOGNORM.INV(0,3.5,1.2)", dim="errors")]),
        ("EXPON.DIST", [C("=EXPON.DIST(0.2,10,TRUE)"), C("=EXPON.DIST(0.2,10,FALSE)"), C("=EXPON.DIST(0.2,0,TRUE)", dim="errors")]),
        ("POISSON.DIST", [C("=POISSON.DIST(2,5,FALSE)"), C("=POISSON.DIST(2,5,TRUE)"), C("=POISSON.DIST(-1,5,TRUE)", dim="errors")]),
        ("BINOM.DIST", [C("=BINOM.DIST(6,10,0.5,FALSE)"), C("=BINOM.DIST(6,10,0.5,TRUE)"), C("=BINOM.DIST(6,10,1.5,FALSE)", dim="errors")]),
        ("BINOM.INV", [C("=BINOM.INV(6,0.5,0.75)"), C("=BINOM.INV(100,0.3,0.5)"), C("=BINOM.INV(6,0.5,1.5)", dim="errors")]),
        ("BINOM.DIST.RANGE", [C("=BINOM.DIST.RANGE(60,0.75,48)"), C("=BINOM.DIST.RANGE(60,0.75,45,50)"), C("=BINOM.DIST.RANGE(60,1.5,48)", dim="errors")]),
        ("NEGBINOM.DIST", [C("=NEGBINOM.DIST(10,5,0.25,FALSE)"), C("=NEGBINOM.DIST(10,5,0.25,TRUE)"), C("=NEGBINOM.DIST(10,5,1.5,TRUE)", dim="errors")]),
        ("HYPGEOM.DIST", [C("=HYPGEOM.DIST(1,4,8,20,FALSE)"), C("=HYPGEOM.DIST(1,4,8,20,TRUE)"), C("=HYPGEOM.DIST(1,4,30,20,FALSE)", dim="errors")]),
        ("WEIBULL.DIST", [C("=WEIBULL.DIST(105,20,100,TRUE)"), C("=WEIBULL.DIST(105,20,100,FALSE)"), C("=WEIBULL.DIST(105,0,100,TRUE)", dim="errors")]),
        ("GAMMA", [C("=GAMMA(2.5)"), C("=GAMMA(-1.5)"), C("=GAMMA(0)", dim="errors")]),
        ("GAMMALN", [C("=GAMMALN(4)"), C("=GAMMALN(0.5)"), C("=GAMMALN(0)", dim="errors")]),
        ("GAMMALN.PRECISE", [C("=GAMMALN.PRECISE(4)"), C("=GAMMALN.PRECISE(0.5)"), C("=GAMMALN.PRECISE(0)", dim="errors")]),
        ("GAMMA.DIST", [C("=GAMMA.DIST(10.00001131,9,2,FALSE)"), C("=GAMMA.DIST(10.00001131,9,2,TRUE)"), C("=GAMMA.DIST(-1,9,2,TRUE)", dim="errors")]),
        ("GAMMA.INV", [C("=GAMMA.INV(0.068094,9,2)"), C("=GAMMA.INV(0.5,3,1)"), C("=GAMMA.INV(1.5,9,2)", dim="errors")]),
    ],
}
