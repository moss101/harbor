import 'dart:convert';
import 'dart:io';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';

/// A recently opened or created file: identity for the Work surface's
/// recents list (the path is what the picker granted; reopening verifies
/// the bytes honestly rather than trusting the path blindly).
@immutable
class RecentFile {
  const RecentFile({
    required this.name,
    required this.path,
    required this.kind,
    required this.at,
  });

  final String name;
  final String path;
  final String kind;
  final String at;

  Map<String, dynamic> toJson() => {
        'name': name,
        'path': path,
        'kind': kind,
        'at': at,
      };

  static RecentFile? fromJson(Map<String, dynamic> json) {
    final name = json['name'];
    final path = json['path'];
    final kind = json['kind'];
    final at = json['at'];
    if (name is! String || path is! String || kind is! String) return null;
    return RecentFile(
        name: name,
        path: path,
        kind: kind,
        at: at is String ? at : '');
  }
}

/// Presentation preferences the shell owns (never policy): locale, theme,
/// and the recents list for the Work surface.
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
    this.recentFiles = const [],
  });

  final Locale locale;
  final ThemeMode themeMode;
  final bool lensDocked;
  final String? chatModel;
  final List<RecentFile> recentFiles;

  /// [file] moved to the front (deduped by path), capped at 8.
  HarborPreferences withRecentFile(RecentFile file) {
    final rest = recentFiles.where((r) => r.path != file.path).toList();
    return copyWith(recentFiles: [file, ...rest].take(8).toList());
  }

  HarborPreferences copyWith({
    Locale? locale,
    ThemeMode? themeMode,
    bool? lensDocked,
    String? chatModel,
    bool clearChatModel = false,
    List<RecentFile>? recentFiles,
  }) {
    return HarborPreferences(
      locale: locale ?? this.locale,
      themeMode: themeMode ?? this.themeMode,
      lensDocked: lensDocked ?? this.lensDocked,
      chatModel: clearChatModel ? null : (chatModel ?? this.chatModel),
      recentFiles: recentFiles ?? this.recentFiles,
    );
  }

  Map<String, dynamic> toJson() => {
        'locale': locale.languageCode,
        'theme': themeMode.name,
        'lens_docked': lensDocked,
        if (chatModel != null) 'chat_model': chatModel,
        'recent_files': [for (final r in recentFiles) r.toJson()],
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
      recentFiles: json['recent_files'] is List
          ? (json['recent_files'] as List)
              .map((e) =>
                  e is Map ? RecentFile.fromJson(e.cast<String, dynamic>()) : null)
              .whereType<RecentFile>()
              .toList()
          : const [],
    );
  }

  @override
  bool operator ==(Object other) =>
      other is HarborPreferences &&
      other.locale == locale &&
      other.themeMode == themeMode &&
      other.lensDocked == lensDocked &&
      other.chatModel == chatModel &&
      const ListEquality().equals(other.recentFiles, recentFiles);

  @override
  int get hashCode =>
      Object.hash(locale, themeMode, lensDocked, chatModel, Object.hashAll(
          recentFiles.map((r) => Object.hash(r.name, r.path, r.kind))));
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
