/// Deck designer widgets: pick a purpose, a design (with live thumbnails), a
/// palette and a motion preset. Data models mirror the core's `deck.catalog`;
/// the apps wire them to the core through [DeckDesignerApi].
library harbor_ui_deck_designer;

export 'src/deck_designer/api.dart';
export 'src/deck_designer/design_gallery.dart';
export 'src/deck_designer/models.dart';
export 'src/deck_designer/motion_picker.dart';
export 'src/deck_designer/palette_strip.dart';
export 'src/deck_designer/purpose_picker.dart';
