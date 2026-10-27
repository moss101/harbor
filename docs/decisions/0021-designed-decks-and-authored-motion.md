# Decision 0021 — Designed decks, real slide previews and authored motion

Date: 2026-10-03

Renumbered from 0011 on integration (2026-10-28): main already used 0011 for EmbeddingGemma 2 and personal agents.
Status: Accepted (implemented; matrix rows 24-25 added as REQUIRED_UNQUALIFIED).

## Problem
Harbor wrote one slide design — a fixed two-layout master, one palette, one font — and
the apps previewed decks as a list of titles and bullets. Users asking for a pitch deck,
an investor update or a product launch got a plain outline in a plain file, could not see
what they were making, and could not choose how it looked or moved. The feature matrix
also said animations and transitions are never emitted (PRESERVE_ONLY), which ruled out
motion entirely.

## Decision
1. **Slides are previewed from the file, not from a Harbor model.** `harbor_render::slides`
   resolves any PPTX (Harbor's, PowerPoint's, anyone's) into a scene — layout and master
   inheritance, theme colours and transforms, geometry, text styles, pictures, tables,
   charts, authored transitions and animations — and the Flutter `SlideView` paints it.
   What is previewed is what the file contains. What it cannot draw is listed per slide
   (`unsupported`), never dropped silently. Every part read is size-bounded and XML nesting
   is capped (roxmltree recurses per level; a few hundred nested elements overflow the
   stack), and the same cap now guards the shared package probe used by every reader.
2. **Design is separate from content.** A `DesignedDeck` is content (titles, bullets,
   blocks, chart data) plus a `Design` (theme, motion, purpose, right-to-left). Fourteen
   themes (palette, Office-shipped font pair, cover style, ornament, card style), fifteen
   layouts (cover, section, bullets, two-column, comparison, stats, cards, timeline, quote,
   image split, chart, agenda, team, statement, closing) and twenty purposes (pitch deck,
   investor update, project report, product launch, ...) each with a storyline. Layout is
   inferred from content by documented rules and steered — never forced — by the purpose's
   storyline: a beat only overrides when the slide's content can honestly fill the layout.
   Text sizes are fitted by estimate, not trusted to a viewer's autofit.
3. **The batch records the design.** `deck.build` takes an optional `design`; the
   creation batch carries theme, motion and the RESOLVED layout and kicker of every slide,
   so the approved output is re-derivable from the batch alone, as for every creation
   batch. Without a `design` the plain deck is produced exactly as before (approved
   batches stay valid). The presentation-builder and report-to-slides skills expose
   purpose, theme, motion and right-to-left as inputs; presentation-builder briefs the
   outline writer with the purpose's storyline (`deck.brief`).
4. **Motion is authored, bounded and optional.** Presets none / subtle / dynamic /
   cinematic choose transitions and entrance animations. Content appears by itself in a
   fixed order (nothing waits for a click unless the author asks for progressive bullets),
   total motion per slide stays under about three seconds, nothing loops, nothing makes a
   sound. Transitions that need a newer PowerPoint ride in `mc:AlternateContent` with a
   plain `fade` fallback. The preview honours the platform reduce-motion setting by
   skipping motion entirely. Motion Harbor did not author stays PRESERVE_ONLY: it is
   previewed but never rewritten.
5. **Harbor-made decks carry their source** (`harbor/deck.json`, linked by a Harbor
   relationship) so the design can be changed losslessly later. A deck any other app has
   re-saved loses it; changing its design is then refused with the reason, never guessed.
6. **Generated charts are schema-valid.** The previous chart writer interpolated the plot
   element's NAME as bare text (`<c:layout/>barChart<c:grouping/>`), so every chart it
   wrote was invalid. All chart writing now goes through one writer with an element-order
   lint, theme-referenced colours, and a real embedded workbook behind "Edit Data".

## Verification
Deterministic bytes; every theme x layout x motion builds and re-reads; contrast gates on
every theme (ink 7:1, muted ink 4.5:1, accent 3:1, text on accent fills 3:1); XML
well-formedness of every part and of the timing tree; PowerPoint opens a generated deck
without repair and reports the transitions and animation counts the planner emitted (checked
by script on the qualification Mac); the scene reader is exercised against Harbor decks,
hand-built packages (groups, rotation, gradients, dark colour-map overrides, tables) and
hostile input; the Dart painter loads Rust-produced scene fixtures.

## Not done / still open
Rows 24-25 stay REQUIRED_UNQUALIFIED: a Keynote and Google Slides open check, and the
cross-platform fixture set, are not yet run. Table style definitions are approximated in
the preview; SmartArt, OLE objects and vertical text are listed as unsupported, not drawn.
