import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/deck_designer.dart';

import '../../l10n/app_localizations.dart';

/// The reason behind a failure, as the core stated it.
String deckErrorText(Object e) =>
    e is ffi.HarborCoreException ? e.message : e.toString();

/// The shared deck designer's strings, from this app's localisations.
DeckDesignerLabels deckDesignerLabels(AppLocalizations l10n) {
  return DeckDesignerLabels(
    newTitle: l10n.deckNewTitle,
    catalogLoading: l10n.deckCatalogLoading,
    catalogFailed: l10n.deckCatalogFailed,
    retry: l10n.deckRetry,
    stepPurpose: l10n.deckStepPurpose,
    stepDesign: l10n.deckStepDesign,
    stepStyle: l10n.deckStepStyle,
    stepOf: l10n.deckStepOf,
    purposeHeading: l10n.deckPurposeHeading,
    purposeHelp: l10n.deckPurposeHelp,
    purposeLabel: l10n.deckPurposeLabel,
    createFailed: l10n.deckCreateFailed,
    back: l10n.deckBack,
    next: l10n.deckNext,
    creating: l10n.deckCreating,
    create: l10n.deckCreate,
    designHeading: l10n.deckDesignHeading,
    paletteOwn: l10n.deckPaletteOwn,
    paletteLabel: l10n.deckPaletteLabel,
    darkLabel: l10n.deckDarkLabel,
    lightLabel: l10n.deckLightLabel,
    suggested: l10n.deckSuggested,
    thumbUnavailable: l10n.deckThumbUnavailable,
    galleryLabel: l10n.deckGalleryLabel,
    styleHeading: l10n.deckStyleHeading,
    motionHeading: l10n.deckMotionHeading,
    motionLabel: l10n.deckMotionLabel,
    motionNames: {
      'none': l10n.deckMotionNone,
      'subtle': l10n.deckMotionSubtle,
      'dynamic': l10n.deckMotionDynamic,
      'cinematic': l10n.deckMotionCinematic,
    },
    motionDescriptions: {
      'none': l10n.deckMotionNoneDesc,
      'subtle': l10n.deckMotionSubtleDesc,
      'dynamic': l10n.deckMotionDynamicDesc,
      'cinematic': l10n.deckMotionCinematicDesc,
    },
    languageHeading: l10n.deckLanguageHeading,
    languageLtr: l10n.deckLanguageLtr,
    languageRtl: l10n.deckLanguageRtl,
    previewHeading: l10n.deckPreviewHeading,
    previewUnavailable: l10n.deckPreviewUnavailable,
    previewOf: l10n.deckPreviewOf,
    previousSlide: l10n.presentPrevious,
    nextSlide: l10n.presentNext,
    slideOf: l10n.presentSlideOf,
    changeDesignTitle: l10n.deckChangeDesignTitle,
    currentDesign: l10n.deckCurrentDesign,
    applyDesign: l10n.deckApplyDesign,
    errorText: deckErrorText,
  );
}
