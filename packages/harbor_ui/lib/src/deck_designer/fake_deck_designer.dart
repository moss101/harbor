import '../slides/slide_scene.dart';
import 'api.dart';
import 'models.dart';

// A scriptable [DeckDesignerApi] for widget tests (this package's and the
// apps'). Test support only: nothing in a shipping build imports it.

const fakeDesigns = [
  DeckDesignInfo(
      id: 'aurora', name: 'Aurora', tagline: 'Glowing gradients', dark: true),
  DeckDesignInfo(id: 'ivory', name: 'Ivory', tagline: 'Quiet editorial'),
  DeckDesignInfo(id: 'slate', name: 'Slate', tagline: 'Calm and corporate'),
  DeckDesignInfo(id: 'ocean', name: 'Ocean', tagline: 'Deep blues'),
];

const fakePalettes = [
  DeckPaletteInfo(id: 'terracotta', name: 'Terracotta', mood: 'warm'),
  DeckPaletteInfo(id: 'mint', name: 'Mint', mood: 'fresh'),
];

const fakeMotions = [
  DeckMotionInfo(id: 'none', description: 'No motion'),
  DeckMotionInfo(id: 'subtle', description: 'Gentle fades'),
  DeckMotionInfo(id: 'dynamic', description: 'Pushes'),
  DeckMotionInfo(id: 'cinematic', description: 'Ripples'),
];

const fakePurposes = [
  DeckPurposeInfo(
      id: 'pitch_deck',
      name: 'Pitch deck',
      summary: 'Raise a round.',
      theme: 'aurora',
      altThemes: ['ocean'],
      motion: 'cinematic'),
  DeckPurposeInfo(
      id: 'project_report',
      name: 'Project report',
      summary: 'Status for stakeholders.',
      theme: 'slate',
      motion: 'subtle'),
];

/// Background colour a fake sample uses for a palette (so a test can see a
/// recolour).
String fakeBackground(String? palette) => switch (palette) {
      'terracotta' => '#C2410C',
      'mint' => '#10B981',
      _ => '#101010',
    };

DeckScene fakeScene(String? palette) => DeckScene(
      width: 12192000,
      height: 6858000,
      slides: [
        for (var i = 1; i <= 3; i++)
          SlideScene(
              index: i,
              background: ScenePaint(
                  kind: PaintKind.solid, c: fakeBackground(palette), a: 1)),
      ],
    );

/// A [DeckDesignerApi] that records every call and can be told to fail.
class FakeDeckDesigner implements DeckDesignerApi {
  FakeDeckDesigner();

  final List<DeckStyleChoice> samples = [];
  final List<({String purpose, DeckStyleChoice? style})> starters = [];
  final List<DeckStyleChoice> restyles = [];
  int catalogCalls = 0;

  /// Fail the next N catalogue / starter / restyle / sample calls.
  int failCatalog = 0, failStarter = 0, failRestyle = 0, failSample = 0;

  @override
  Future<DeckCatalog> deckCatalog() async {
    catalogCalls++;
    if (failCatalog > 0) {
      failCatalog--;
      throw Exception('catalogue offline');
    }
    return const DeckCatalog(
      designs: fakeDesigns,
      palettes: fakePalettes,
      purposes: fakePurposes,
      motions: fakeMotions,
    );
  }

  @override
  Future<DeckScene> deckSample(DeckStyleChoice style) async {
    samples.add(style);
    if (failSample > 0) {
      failSample--;
      throw Exception('sample failed');
    }
    return fakeScene(style.palette);
  }

  @override
  Future<DeckStarterResult> deckStarter({
    required String purpose,
    DeckStyleChoice? style,
    String? title,
  }) async {
    starters.add((purpose: purpose, style: style));
    if (failStarter > 0) {
      failStarter--;
      throw Exception('the core said no');
    }
    return DeckStarterResult(
      bytes: const [1, 2, 3],
      fileName: 'Starter.pptx',
      title: 'Starter',
      slides: 3,
      theme: style?.design ?? '',
      motion: style?.motion ?? '',
    );
  }

  @override
  Future<DeckRestyleResult> restyleOpenDeck(DeckStyleChoice style) async {
    restyles.add(style);
    if (failRestyle > 0) {
      failRestyle--;
      throw Exception('restyle refused');
    }
    return DeckRestyleResult(bytes: const [9], scene: fakeScene(style.palette));
  }
}
