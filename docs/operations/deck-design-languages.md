# Deck design languages

A **design** (theme id) decides layout, shape and typography; a **palette**
decides colour and whether the deck is dark or light. Any palette goes on any
design, so a design must read correctly in every palette (dark and light), and
two designs must never be one composition in two colours. Code:
`core/harbor_artifacts/src/deck/` — `look.rs` (the choices), `theme.rs` (which
theme uses which), `layouts/*.rs` and `cover.rs` (what each choice draws),
`header.rs` + `parts.rs` (master furniture).

## What makes a design (the axes)

| axis | values |
|---|---|
| header (`Header`) | TopLeft, Banner, Sidebar{right}, Rail, Centered, Poster — owned by the integrator (frame + master furniture) |
| cover (`CoverStyle`) | Aurora, Split, Editorial, Centered, Poster, Band, Bento |
| section (`SectionStyle`) | Numeral, Split, Band |
| chrome (`CardStyle`) | Filled, Outlined, Raised, Glass, Sticker, Rule, Bento — `layouts/kit.rs::chrome` |
| stats (`StatsStyle`) | Tiles, Rows, Hero, Columns |
| cards (`CardsStyle`) | Grid, Rows, Columns, Stickers, Bento |
| timeline (`TimelineStyle`) | Spine, Chevrons, Vertical |
| compare (`CompareStyle`) | Panels, Rows, Fields |
| agenda (`AgendaStyle`) | Rows, Grid |
| team (`TeamStyle`) | Cards, Inline |

Every layout builder works inside `cx.frame.body` (the area the header leaves
for content: from ~6.8 M EMU wide beside a side panel to ~10.8 M wide full
width; 4.0–5.5 M tall). **A variant must adapt to the area it is given**: a
three-column grid that is fine at 10.8 M wide must restack at 6.8 M, not
squeeze words to one per line.

## The fourteen designs

1. **aurora** — *Glass.* Luminous dark. Glass cards (hairline + 7% fill), neon
   accents, orbs. Tiles / Grid / Spine / Panels / Rows / Cards. Cover Aurora.
2. **harbor** — *Corporate banner.* Navy band carries the title. Process
   **chevrons** for timelines, boxed tiles with an accent top edge, solid
   panels. Cover Band: a top colour band with the title, a wide art field below.
3. **ivory** — *Editorial rail.* Serif, hairlines, no boxes. Stats as **ruled
   rows** (a huge serif numeral on the left, its label right, a rule between);
   cards as **numbered rows**; **vertical** timeline with dates in a left
   column; comparison as a **table** with hairline rules; inline team. Cover
   Editorial on paper.
4. **slate** — *Status dashboard.* **Hero** stat (one giant KPI on the left, the
   rest stacked as a list on the right), cards as **columns** divided by thin
   vertical rules, comparison as **two colour fields**, agenda as a numeral grid.
   Cover Split.
5. **sunrise** — *Friendly stickers.* Thick outlines and **hard offset shadows**
   (chrome Sticker), cards as slightly **rotated stickers** (−1.5° … +1.5°),
   chevron timeline, warm gradient cover (Aurora style on a light theme).
6. **obsidian** — *Bento.* Black, rounded tiles. **Hero** stats laid out as a
   bento (one large tile + smaller), cards as a **bento grid** (1 large + 2
   small, asymmetric), chevrons. Cover Bento: a grid of tiles, one carries the title.
7. **ocean** — *Sidebar, left.* Title lives in a colour panel at the left; the
   content area is narrow. Stats as rows, cards as numbered rows, vertical
   timeline, comparison rows, inline team. Cover Split.
8. **forest** — *Centred.* Everything on a centre axis under a small ornament.
   Stats as centred **columns** with thin vertical rules; cards as centred
   columns; spine timeline; panels; inline team. Cover Centered (rings).
9. **midnight_gold** — *Rail, dark.* Left rail with a large gold slide number,
   hairline gold rules, serif. Columns stats/cards, spine timeline, fields
   comparison. Cover Centered with halo rings.
10. **playground** — *Centred stickers.* Bright, rounded, thick outlines and
    hard shadows, stickers, chevrons, fields comparison. Cover Aurora in a
    vivid light gradient.
11. **mono** — *Poster.* Huge flush-left serif title over a heavy rule; one red
    accent. Hero stats (a giant numeral), columns cards, vertical timeline,
    fields comparison, inline team. Cover Poster: type fills the slide.
12. **terracotta** — *Sidebar, right.* Clay panel on the right carries the title.
    Tiles stats, cards that stack in the narrow area, spine timeline, fields
    comparison. Cover Editorial.
13. **royal** — *Poster × bento, dark.* Heavy rule header; tiles stats; bento
    cards; spine; panels. Cover Bento in violet/pink.
14. **blueprint** — *Technical banner + grid.* Banner header over a faint
    drafting grid; **cards must be opaque** (grid must not run through text);
    columns stats and cards with bracketed labels like `[01]`, vertical
    timeline, comparison rows, inline team. Cover Split.

## Rules every variant follows

* **Colours are theme references** (`Clr::accent(n)`, `Clr::tx1()`, `Clr::bg2()`
  …), never literals: every palette must work. Text on an accent fill uses
  `on_accent(t, n)`. A number or label in an accent colour as *text* goes through
  `accent_text(t, n, min_contrast)` (falls back to ink when the accent would not read).
* **Fit text by estimate** (`fit_pt`, `stack_height`, `line_count` in `grid.rs`);
  never trust autofit. The generic gate `no_variant_overflows_its_text_boxes_or_the_page`
  runs every variant under every header with dense content; it must stay green.
* **Motion roles.** Everything that belongs to the n-th card/stat/milestone/person
  is tagged `Role::Item(n)` (use `b.card(...)` for chrome), so entrance
  animations reveal items together, in order.
* **Right-to-left.** Build geometry left-to-right; shapes are mirrored for RTL
  decks automatically. Do not hard-code left/right in text alignment (use
  `Align`, which mirrors). Check one Arabic render (`deck_review.sh … rtl`).
* **Accessibility.** Purely decorative shapes are `Shape::deco(..)` (skipped by
  screen readers); real content is text in a shape or placeholder. Reading
  order = emission order: title, then items in order.
* **Restraint.** SOTA decks are quiet: generous margins (use the frame), at most
  two type sizes in a card, one accent per item, alignment to a grid. If a
  variant looks busy, remove something.
* **Distinctness is the point.** If your variant could be mistaken for another
  design with a different colour, it has failed. Change composition (where the
  big element sits, what is boxed and what is ruled, direction of reading),
  not just fills.
