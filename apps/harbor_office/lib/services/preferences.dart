import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';

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
  });

  final Locale locale;
  final ThemeMode themeMode;
  final bool lensDocked;
  final String? chatModel;

  HarborPreferences copyWith({
    Locale? locale,
    ThemeMode? themeMode,
    bool? lensDocked,
    String? chatModel,
    bool clearChatModel = false,
  }) {
    return HarborPreferences(
      locale: locale ?? this.locale,
      themeMode: themeMode ?? this.themeMode,
      lensDocked: lensDocked ?? this.lensDocked,
      chatModel: clearChatModel ? null : (chatModel ?? this.chatModel),
    );
  }

  Map<String, dynamic> toJson() => {
        'locale': locale.languageCode,
        'theme': themeMode.name,
        'lens_docked': lensDocked,
        if (chatModel != null) 'chat_model': chatModel,
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
    );
  }

  @override
  bool operator ==(Object other) =>
      other is HarborPreferences &&
      other.locale == locale &&
      other.themeMode == themeMode &&
      other.lensDocked == lensDocked &&
      other.chatModel == chatModel;

  @override
  int get hashCode => Object.hash(locale, themeMode, lensDocked, chatModel);
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
