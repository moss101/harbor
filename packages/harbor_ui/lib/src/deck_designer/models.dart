// Catalogue + request/response models for the deck designer.
//
// Pure data with tolerant `fromJson` (the core owns the catalogue; an
// unknown or missing optional field never throws). JSON shapes are the
// `deck.catalog`, `deck.sample`, `deck.starter` and `deck.restyle` arms of
// `core/harbor_ffi/src/lib.rs`.
import 'dart:ui' show Color;

import 'package:flutter/foundation.dart' show immutable, listEquals;

import '../slides/slide_scene.dart';

String _s(Object? v, [String fallback = '']) => v is String ? v : fallback;
bool _b(Object? v) => v is bool ? v : false;
List<String> _sl(Object? v) => v is List
    ? [
        for (final e in v)
          if (e is String) e
      ]
    : const [];

List<Map<String, dynamic>> _ml(Object? v) {
  if (v is! List) return const [];
  return [
    for (final e in v)
      if (e is Map) e.map((k, val) => MapEntry(k.toString(), val)),
  ];
}

/// Parse a `#RRGGBB` swatch; null for anything else.
Color? parseSwatch(String hex) {
  var h = hex.trim();
  if (h.startsWith('#')) h = h.substring(1);
  if (h.length != 6) return null;
  final v = int.tryParse(h, radix: 16);
  return v == null ? null : Color(0xFF000000 | v);
}

/// A design (composition / layout language), e.g. `aurora`.
@immutable
class DeckDesignInfo {
  const DeckDesignInfo({
    required this.id,
    required this.name,
    this.tagline = '',
    this.dark = false,
    this.swatches = const [],
    this.suits = const [],
    this.majorFont = '',
    this.minorFont = '',
  });

  final String id, name, tagline;

  /// True when the design's own colours are a dark page.
  final bool dark;

  /// Six accents, then page and ink, as `#RRGGBB`.
  final List<String> swatches;
  final List<String> suits;
  final String majorFont, minorFont;

  factory DeckDesignInfo.fromJson(Map<String, dynamic> j) => DeckDesignInfo(
        id: _s(j['id']),
        name: _s(j['name'], _s(j['id'])),
        tagline: _s(j['tagline']),
        dark: _b(j['dark']),
        swatches: _sl(j['swatches']),
        suits: _sl(j['suits']),
        majorFont: _s(j['major_font']),
        minorFont: _s(j['minor_font']),
      );

  @override
  bool operator ==(Object other) =>
      other is DeckDesignInfo &&
      other.id == id &&
      other.name == name &&
      other.tagline == tagline &&
      other.dark == dark;

  @override
  int get hashCode => Object.hash(id, name, tagline, dark);
}

/// A palette (colour only), e.g. `terracotta`.
@immutable
class DeckPaletteInfo {
  const DeckPaletteInfo({
    required this.id,
    required this.name,
    this.mood = '',
    this.dark = false,
    this.swatches = const [],
  });

  final String id, name, mood;
  final bool dark;

  /// Six accents, then page and ink, as `#RRGGBB`.
  final List<String> swatches;

  factory DeckPaletteInfo.fromJson(Map<String, dynamic> j) => DeckPaletteInfo(
        id: _s(j['id']),
        name: _s(j['name'], _s(j['id'])),
        mood: _s(j['mood']),
        dark: _b(j['dark']),
        swatches: _sl(j['swatches']),
      );

  @override
  bool operator ==(Object other) =>
      other is DeckPaletteInfo &&
      other.id == id &&
      other.name == name &&
      listEquals(other.swatches, swatches);

  @override
  int get hashCode => Object.hash(id, name, Object.hashAll(swatches));
}

/// One slide of a purpose's storyline.
@immutable
class DeckBeatInfo {
  const DeckBeatInfo({this.kicker = '', this.layout = '', this.says = ''});
  final String kicker, layout, says;

  factory DeckBeatInfo.fromJson(Map<String, dynamic> j) => DeckBeatInfo(
        kicker: _s(j['kicker']),
        layout: _s(j['layout']),
        says: _s(j['says']),
      );
}

/// A deck purpose (pitch deck, project report, …) with its storyline.
@immutable
class DeckPurposeInfo {
  const DeckPurposeInfo({
    required this.id,
    required this.name,
    this.summary = '',
    this.theme = '',
    this.altThemes = const [],
    this.motion = '',
    this.progressive = false,
    this.beats = const [],
  });

  final String id, name;

  /// One-line description (the core's `summary`).
  final String summary;

  /// Default design id and the designs that suit this purpose.
  final String theme;
  final List<String> altThemes;

  /// Default motion preset id.
  final String motion;
  final bool progressive;
  final List<DeckBeatInfo> beats;

  factory DeckPurposeInfo.fromJson(Map<String, dynamic> j) => DeckPurposeInfo(
        id: _s(j['id']),
        name: _s(j['name'], _s(j['id'])),
        summary: _s(j['summary']),
        theme: _s(j['theme']),
        altThemes: _sl(j['alt_themes']),
        motion: _s(j['motion']),
        progressive: _b(j['progressive']),
        beats: [for (final b in _ml(j['beats'])) DeckBeatInfo.fromJson(b)],
      );
}

/// A motion preset: `none`, `subtle`, `dynamic` or `cinematic`.
@immutable
class DeckMotionInfo {
  const DeckMotionInfo({required this.id, this.description = ''});
  final String id, description;

  factory DeckMotionInfo.fromJson(Map<String, dynamic> j) =>
      DeckMotionInfo(id: _s(j['id']), description: _s(j['description']));
}

/// The whole `deck.catalog` answer.
@immutable
class DeckCatalog {
  const DeckCatalog({
    this.designs = const [],
    this.palettes = const [],
    this.purposes = const [],
    this.motions = const [],
    this.layouts = const [],
  });

  final List<DeckDesignInfo> designs;
  final List<DeckPaletteInfo> palettes;
  final List<DeckPurposeInfo> purposes;
  final List<DeckMotionInfo> motions;
  final List<String> layouts;

  /// The core's JSON calls designs "themes".
  factory DeckCatalog.fromJson(Map<String, dynamic> j) => DeckCatalog(
        designs: [for (final t in _ml(j['themes'])) DeckDesignInfo.fromJson(t)],
        palettes: [
          for (final p in _ml(j['palettes'])) DeckPaletteInfo.fromJson(p)
        ],
        purposes: [
          for (final p in _ml(j['purposes'])) DeckPurposeInfo.fromJson(p)
        ],
        motions: [
          for (final m in _ml(j['motions'])) DeckMotionInfo.fromJson(m)
        ],
        layouts: _sl(j['layouts']),
      );

  DeckDesignInfo? design(String id) {
    for (final d in designs) {
      if (d.id == id) return d;
    }
    return null;
  }

  DeckPaletteInfo? palette(String id) {
    for (final p in palettes) {
      if (p.id == id) return p;
    }
    return null;
  }

  DeckPurposeInfo? purpose(String id) {
    for (final p in purposes) {
      if (p.id == id) return p;
    }
    return null;
  }
}

/// What the user picked in the designer. [palette] null/empty = the
/// design's own colours.
@immutable
class DeckStyleChoice {
  const DeckStyleChoice({
    required this.design,
    this.palette,
    this.motion = 'subtle',
    this.rtl = false,
  });

  final String design;
  final String? palette;
  final String motion;
  final bool rtl;

  DeckStyleChoice copyWith({
    String? design,
    String? palette,
    bool clearPalette = false,
    String? motion,
    bool? rtl,
  }) =>
      DeckStyleChoice(
        design: design ?? this.design,
        palette: clearPalette ? null : (palette ?? this.palette),
        motion: motion ?? this.motion,
        rtl: rtl ?? this.rtl,
      );

  /// Cache key for a rendered sample of this choice.
  String get cacheKey => '$design|${palette ?? ''}|$motion|$rtl';

  @override
  bool operator ==(Object other) =>
      other is DeckStyleChoice &&
      other.design == design &&
      (other.palette ?? '') == (palette ?? '') &&
      other.motion == motion &&
      other.rtl == rtl;

  @override
  int get hashCode => Object.hash(design, palette ?? '', motion, rtl);

  @override
  String toString() => 'DeckStyleChoice($cacheKey)';
}

/// `deck.starter` answer.
@immutable
class DeckStarterResult {
  const DeckStarterResult({
    required this.bytes,
    required this.fileName,
    required this.title,
    required this.slides,
    required this.theme,
    required this.motion,
    this.scene,
  });

  final List<int> bytes;
  final String fileName, title, theme, motion;
  final int slides;
  final DeckScene? scene;
}

/// `deck.restyle` answer.
@immutable
class DeckRestyleResult {
  const DeckRestyleResult({required this.bytes, this.scene});
  final List<int> bytes;
  final DeckScene? scene;
}
