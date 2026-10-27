import 'models.dart';
import '../slides/slide_scene.dart';

/// What the deck designer needs from the core. Each app's `HarborService`
/// implements it; widget tests pass a fake. Failures throw (the apps use
/// their `HarborCoreException`) so callers can show the core's reason.
abstract class DeckDesignerApi {
  /// Designs, palettes, purposes and motions. Cached by the implementation.
  Future<DeckCatalog> deckCatalog();

  /// A small showcase deck in [style] (slide 1 is the cover), as a scene.
  Future<DeckScene> deckSample(DeckStyleChoice style);

  /// A full starter deck following [purpose]'s storyline.
  /// [style] null = the purpose's own defaults.
  Future<DeckStarterResult> deckStarter({
    required String purpose,
    DeckStyleChoice? style,
    String? title,
  });

  /// Re-style the open deck's working copy and adopt the result as the new
  /// (unsaved) working copy, keeping undo history. Never writes the source
  /// file. [palette] empty = the design's own colours.
  Future<DeckRestyleResult> restyleOpenDeck(DeckStyleChoice style);
}
