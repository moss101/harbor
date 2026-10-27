import 'package:flutter/foundation.dart' show immutable;

/// Every user-visible string of the deck designer flows (the New
/// presentation sheet and the Change design dialog).
///
/// harbor_ui has no l10n of its own: each app builds one of these from its
/// own localisations (see the apps' `deckDesignerLabels`), so translations
/// stay with the app's .arb files. Parameterised strings are closures so the
/// app's plural/placeholder handling is used as is.
@immutable
class DeckDesignerLabels {
  const DeckDesignerLabels({
    required this.newTitle,
    required this.catalogLoading,
    required this.catalogFailed,
    required this.retry,
    required this.stepPurpose,
    required this.stepDesign,
    required this.stepStyle,
    required this.stepOf,
    required this.purposeHeading,
    required this.purposeHelp,
    required this.purposeLabel,
    required this.createFailed,
    required this.back,
    required this.next,
    required this.creating,
    required this.create,
    required this.designHeading,
    required this.paletteOwn,
    required this.paletteLabel,
    required this.darkLabel,
    required this.lightLabel,
    required this.suggested,
    required this.thumbUnavailable,
    required this.galleryLabel,
    required this.styleHeading,
    required this.motionHeading,
    required this.motionLabel,
    required this.motionNames,
    required this.motionDescriptions,
    required this.languageHeading,
    required this.languageLtr,
    required this.languageRtl,
    required this.previewHeading,
    required this.previewUnavailable,
    required this.previewOf,
    required this.previousSlide,
    required this.nextSlide,
    required this.slideOf,
    required this.changeDesignTitle,
    required this.currentDesign,
    required this.applyDesign,
    required this.errorText,
  });

  /// Title of the full-screen New presentation sheet.
  final String newTitle;

  /// Shown while the catalogue loads.
  final String catalogLoading;

  /// Title and message when the catalogue cannot be loaded.
  final String catalogFailed;

  /// "Try again" on every retryable error.
  final String retry;

  // Step names and the step chip's "Step i of n" (1-based i).
  final String stepPurpose, stepDesign, stepStyle;
  final String Function(int step, int total) stepOf;

  final String purposeHeading, purposeHelp, purposeLabel;

  /// Heading of the banner shown when creating the deck failed.
  final String createFailed;

  final String back, next, creating, create;

  final String designHeading;

  /// The "keep the design's own colours" choice and the palette strip's
  /// semantic label.
  final String paletteOwn, paletteLabel;
  final String darkLabel, lightLabel, suggested;
  final String thumbUnavailable, galleryLabel;

  final String styleHeading, motionHeading, motionLabel;

  /// Names and one-line descriptions keyed by the core's motion id
  /// (`none`, `subtle`, `dynamic`, `cinematic`).
  final Map<String, String> motionNames, motionDescriptions;

  final String languageHeading, languageLtr, languageRtl;

  final String previewHeading, previewUnavailable;

  /// "Preview of {design}".
  final String Function(String design) previewOf;

  /// Tooltips of the preview's slide stepper, and "Slide i of n".
  final String previousSlide, nextSlide;
  final String Function(int index, int total) slideOf;

  final String changeDesignTitle;

  /// "Current design: {name}".
  final String Function(String name) currentDesign;
  final String applyDesign;

  /// The reason behind a failure, as the app's core bridge states it (the
  /// apps unwrap their `HarborCoreException`).
  final String Function(Object error) errorText;
}
