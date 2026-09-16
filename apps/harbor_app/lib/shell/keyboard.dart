import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

/// Desktop keyboard contract (UX-038): ⌘/Ctrl+1…9 switch surfaces,
/// ⌘/Ctrl+K opens the command palette, ⌘/Ctrl+L toggles the Lens,
/// ⌘/Ctrl+O opens a file on the Work Canvas, ⌘/Ctrl+, opens Settings.
class GoToSurfaceIntent extends Intent {
  const GoToSurfaceIntent(this.index);
  final int index;
}

class CommandPaletteIntent extends Intent {
  const CommandPaletteIntent();
}

class ToggleLensIntent extends Intent {
  const ToggleLensIntent();
}

class OpenFileIntent extends Intent {
  const OpenFileIntent();
}

class OpenSettingsIntent extends Intent {
  const OpenSettingsIntent();
}

/// Whether shortcuts should be advertised (desktop platforms only).
bool get harborHasKeyboardShortcuts {
  if (kIsWeb) return false;
  return Platform.isMacOS || Platform.isWindows || Platform.isLinux;
}

/// The modifier glyph for hints: ⌘ on macOS, Ctrl elsewhere.
String get harborModifierGlyph {
  if (!kIsWeb && Platform.isMacOS) return '⌘';
  return 'Ctrl+';
}

String harborShortcutHint(String key) => '$harborModifierGlyph$key';

/// Shortcut map merged into the app's defaults. Both modifiers are bound
/// so the map is platform-neutral; the hint text picks the right glyph.
Map<ShortcutActivator, Intent> harborShortcuts() {
  final map = <ShortcutActivator, Intent>{};
  const digits = [
    LogicalKeyboardKey.digit1,
    LogicalKeyboardKey.digit2,
    LogicalKeyboardKey.digit3,
    LogicalKeyboardKey.digit4,
    LogicalKeyboardKey.digit5,
    LogicalKeyboardKey.digit6,
    LogicalKeyboardKey.digit7,
    LogicalKeyboardKey.digit8,
    LogicalKeyboardKey.digit9,
  ];
  for (final (i, key) in digits.indexed) {
    map[SingleActivator(key, meta: true)] = GoToSurfaceIntent(i);
    map[SingleActivator(key, control: true)] = GoToSurfaceIntent(i);
  }
  map[const SingleActivator(LogicalKeyboardKey.keyK, meta: true)] =
      const CommandPaletteIntent();
  map[const SingleActivator(LogicalKeyboardKey.keyK, control: true)] =
      const CommandPaletteIntent();
  map[const SingleActivator(LogicalKeyboardKey.keyL, meta: true)] =
      const ToggleLensIntent();
  map[const SingleActivator(LogicalKeyboardKey.keyL, control: true)] =
      const ToggleLensIntent();
  map[const SingleActivator(LogicalKeyboardKey.keyO, meta: true)] =
      const OpenFileIntent();
  map[const SingleActivator(LogicalKeyboardKey.keyO, control: true)] =
      const OpenFileIntent();
  map[const SingleActivator(LogicalKeyboardKey.comma, meta: true)] =
      const OpenSettingsIntent();
  map[const SingleActivator(LogicalKeyboardKey.comma, control: true)] =
      const OpenSettingsIntent();
  return map;
}
