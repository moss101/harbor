import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';

/// The last choices made in the New presentation sheet, so the next one
/// starts where the user left off. Ids are the core's catalogue ids; an id
/// the catalogue no longer knows is ignored by the sheet.
@immutable
class DeckDesignPrefs {
  const DeckDesignPrefs({
    this.purpose,
    this.design,
    this.palette,
    this.motion,
    this.rtl = false,
  });

  final String? purpose;
  final String? design;

  /// Null = the design's own colours.
  final String? palette;
  final String? motion;
  final bool rtl;

  Map<String, dynamic> toJson() => {
        if (purpose != null) 'purpose': purpose,
        if (design != null) 'design': design,
        if (palette != null) 'palette': palette,
        if (motion != null) 'motion': motion,
        'rtl': rtl,
      };

  static DeckDesignPrefs fromJson(Map<String, dynamic> json) {
    String? str(String k) => json[k] is String ? json[k] as String : null;
    return DeckDesignPrefs(
      purpose: str('purpose'),
      design: str('design'),
      palette: str('palette'),
      motion: str('motion'),
      rtl: json['rtl'] == true,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is DeckDesignPrefs &&
      other.purpose == purpose &&
      other.design == design &&
      other.palette == palette &&
      other.motion == motion &&
      other.rtl == rtl;

  @override
  int get hashCode => Object.hash(purpose, design, palette, motion, rtl);
}

/// Presentation preferences the shell owns (never policy): locale, theme,
/// whether the Lens is docked on wide windows, the preferred chat model.
///
/// Persisted as a tiny JSON document under the app-support data root so
/// choices survive restarts. The core is never involved — these are UI
/// facts only.
@immutable
class HarborPreferences {
  const HarborPreferences({
    this.locale = const Locale('en'),
    this.themeMode = ThemeMode.system,
    this.lensDocked = true,
    this.chatModel,
    this.deckDesign,
  });

  final Locale locale;
  final ThemeMode themeMode;
  final bool lensDocked;
  final String? chatModel;

  /// Last New presentation choices (null until the sheet was used).
  final DeckDesignPrefs? deckDesign;

  HarborPreferences copyWith({
    Locale? locale,
    ThemeMode? themeMode,
    bool? lensDocked,
    String? chatModel,
    bool clearChatModel = false,
    DeckDesignPrefs? deckDesign,
  }) {
    return HarborPreferences(
      locale: locale ?? this.locale,
      themeMode: themeMode ?? this.themeMode,
      lensDocked: lensDocked ?? this.lensDocked,
      chatModel: clearChatModel ? null : (chatModel ?? this.chatModel),
      deckDesign: deckDesign ?? this.deckDesign,
    );
  }

  Map<String, dynamic> toJson() => {
        'locale': locale.languageCode,
        'theme': themeMode.name,
        'lens_docked': lensDocked,
        if (chatModel != null) 'chat_model': chatModel,
        if (deckDesign != null) 'deck_design': deckDesign!.toJson(),
      };

  static HarborPreferences fromJson(Map<String, dynamic> json) {
    final code = json['locale'];
    final theme = json['theme'];
    return HarborPreferences(
      locale: code is String && (code == 'en' || code == 'ar')
          ? Locale(code)
          : const Locale('en'),
      themeMode: ThemeMode.values.firstWhere(
        (m) => m.name == theme,
        orElse: () => ThemeMode.system,
      ),
      lensDocked:
          json['lens_docked'] is bool ? json['lens_docked'] as bool : true,
      chatModel:
          json['chat_model'] is String ? json['chat_model'] as String : null,
      deckDesign: json['deck_design'] is Map
          ? DeckDesignPrefs.fromJson(
              (json['deck_design'] as Map).cast<String, dynamic>())
          : null,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is HarborPreferences &&
      other.locale == locale &&
      other.themeMode == themeMode &&
      other.lensDocked == lensDocked &&
      other.chatModel == chatModel &&
      other.deckDesign == deckDesign;

  @override
  int get hashCode =>
      Object.hash(locale, themeMode, lensDocked, chatModel, deckDesign);
}

/// Storage for [HarborPreferences]. The file store is used by the real
/// app; tests inject [MemoryPreferencesStore].
abstract class PreferencesStore {
  Future<HarborPreferences> load();
  Future<void> save(HarborPreferences prefs);
}

class MemoryPreferencesStore implements PreferencesStore {
  MemoryPreferencesStore([this._current = const HarborPreferences()]);
  HarborPreferences _current;

  HarborPreferences get current => _current;

  @override
  Future<HarborPreferences> load() async => _current;

  @override
  Future<void> save(HarborPreferences prefs) async => _current = prefs;
}

class FilePreferencesStore implements PreferencesStore {
  FilePreferencesStore(this.path);
  final String path;

  @override
  Future<HarborPreferences> load() async {
    try {
      final file = File(path);
      if (!await file.exists()) return const HarborPreferences();
      final decoded = jsonDecode(await file.readAsString());
      if (decoded is! Map<String, dynamic>) return const HarborPreferences();
      return HarborPreferences.fromJson(decoded);
    } catch (_) {
      // A corrupt preferences file must never block startup.
      return const HarborPreferences();
    }
  }

  @override
  Future<void> save(HarborPreferences prefs) async {
    try {
      final file = File(path);
      await file.parent.create(recursive: true);
      // Write-then-rename so a crash mid-write cannot leave a torn file.
      final tmp = File('$path.tmp');
      await tmp.writeAsString(jsonEncode(prefs.toJson()), flush: true);
      await tmp.rename(path);
    } catch (_) {
      // Persistence is best-effort; the in-memory choice still applies.
    }
  }
}
