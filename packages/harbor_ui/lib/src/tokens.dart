import 'package:flutter/material.dart';

/// Harbor Current 2 semantic color roles (06_Design_Tokens.json).
@immutable
class HarborColors {
  const HarborColors({
    required this.canvas,
    required this.surface,
    required this.surfaceRaised,
    required this.ink,
    required this.inkMuted,
    required this.border,
    required this.brand,
    required this.brandSoft,
    required this.accent,
    required this.local,
    required this.hybrid,
    required this.remote,
    required this.danger,
    required this.warning,
    required this.success,
    required this.onBrand,
    required this.statusLocalText,
    required this.statusLocalFill,
    required this.statusLocalBorder,
    required this.statusHybridText,
    required this.statusHybridFill,
    required this.statusHybridBorder,
    required this.statusRemoteText,
    required this.statusRemoteFill,
    required this.statusRemoteBorder,
    required this.statusDangerText,
    required this.statusDangerFill,
    required this.statusDangerBorder,
    required this.primaryActionText,
    required this.primaryActionFill,
    required this.focusRing,
  });

  final Color canvas;
  final Color surface;
  final Color surfaceRaised;
  final Color ink;
  final Color inkMuted;
  final Color border;
  final Color brand;
  final Color brandSoft;
  final Color accent;
  final Color local;
  final Color hybrid;
  final Color remote;
  final Color danger;
  final Color warning;
  final Color success;
  final Color onBrand;
  final Color statusLocalText, statusLocalFill, statusLocalBorder;
  final Color statusHybridText, statusHybridFill, statusHybridBorder;
  final Color statusRemoteText, statusRemoteFill, statusRemoteBorder;
  final Color statusDangerText, statusDangerFill, statusDangerBorder;
  final Color primaryActionText, primaryActionFill, focusRing;

  static const light = HarborColors(
    canvas: Color(0xFFF7F9FC),
    surface: Color(0xFFFFFFFF),
    surfaceRaised: Color(0xFFFBFCFE),
    ink: Color(0xFF12263A),
    inkMuted: Color(0xFF5B6F84),
    border: Color(0xFFD6E0EA),
    brand: Color(0xFF1F5FCC),
    brandSoft: Color(0xFFEAF2FF),
    accent: Color(0xFF22B8C7),
    local: Color(0xFF1E6F4E),
    hybrid: Color(0xFF8A5200),
    remote: Color(0xFF4F46C8),
    danger: Color(0xFFA33126),
    warning: Color(0xFF8A5200),
    success: Color(0xFF1E6F4E),
    onBrand: Color(0xFFFFFFFF),
    statusLocalText: Color(0xFF1E6F4E),
    statusLocalFill: Color(0xFFE9F5EF),
    statusLocalBorder: Color(0xFF6D9884),
    statusHybridText: Color(0xFF8A5200),
    statusHybridFill: Color(0xFFFFF3DE),
    statusHybridBorder: Color(0xFFAC8B54),
    statusRemoteText: Color(0xFF4F46C8),
    statusRemoteFill: Color(0xFFEEEEFF),
    statusRemoteBorder: Color(0xFF8B88C8),
    statusDangerText: Color(0xFFA33126),
    statusDangerFill: Color(0xFFFCECE9),
    statusDangerBorder: Color(0xFFB8827C),
    primaryActionText: Color(0xFFFFFFFF),
    primaryActionFill: Color(0xFF1F5FCC),
    focusRing: Color(0xFF1F5FCC),
  );

  static const dark = HarborColors(
    canvas: Color(0xFF08131F),
    surface: Color(0xFF0E1C2B),
    surfaceRaised: Color(0xFF132438),
    ink: Color(0xFFF3F7FB),
    inkMuted: Color(0xFF9FB0C1),
    border: Color(0xFF253C52),
    brand: Color(0xFF7EA9FF),
    brandSoft: Color(0xFF132E54),
    accent: Color(0xFF4BD5E1),
    local: Color(0xFF7FE0AF),
    hybrid: Color(0xFFF6C36B),
    remote: Color(0xFFC5C6FF),
    danger: Color(0xFFFFAAA0),
    warning: Color(0xFFF6C36B),
    success: Color(0xFF7FE0AF),
    onBrand: Color(0xFF07111D),
    statusLocalText: Color(0xFF7FE0AF),
    statusLocalFill: Color(0xFF153328),
    statusLocalBorder: Color(0xFF2C7A58),
    statusHybridText: Color(0xFFF6C36B),
    statusHybridFill: Color(0xFF3A2A12),
    statusHybridBorder: Color(0xFF8D682C),
    statusRemoteText: Color(0xFFC5C6FF),
    statusRemoteFill: Color(0xFF28295C),
    statusRemoteBorder: Color(0xFF5D5FB8),
    statusDangerText: Color(0xFFFFAAA0),
    statusDangerFill: Color(0xFF4A211D),
    statusDangerBorder: Color(0xFFA8564E),
    primaryActionText: Color(0xFF07111D),
    primaryActionFill: Color(0xFF7EA9FF),
    focusRing: Color(0xFF9FC0FF),
  );
}

/// Typography with Arabic line-height parity (body 14/24 ar vs 14/21 latin,
/// etc.). Arabic text must never be compressed onto Latin metrics.
@immutable
class HarborType {
  const HarborType({required this.arabic});
  final bool arabic;

  static const familyLatin = 'Inter';
  static const familyArabic = 'Noto Sans Arabic';
  static const familyMono = 'JetBrains Mono';

  String get family => arabic ? familyArabic : familyLatin;

  double get bodyLine => arabic ? 24 : 21;
  double get smallLine => arabic ? 20 : 17;
  double get captionLine => arabic ? 18 : 15;

  TextStyle bodyOf(Color color) =>
      TextStyle(fontFamily: family, fontSize: 14, height: bodyLine / 14, color: color);
  TextStyle smallOf(Color color) =>
      TextStyle(fontFamily: family, fontSize: 12, height: smallLine / 12, color: color);
  // Token weight 450 -> nearest Flutter weight w500 (FontWeight ships hundreds only).
  TextStyle captionOf(Color color) => TextStyle(
      fontFamily: family, fontSize: 11, height: captionLine / 11, fontWeight: FontWeight.w500, color: color);
  // Token weights 620/650 -> w600.
  TextStyle titleOf(Color color) => TextStyle(
      fontFamily: family, fontSize: 24, height: 30 / 24, fontWeight: FontWeight.w600, color: color);
  TextStyle h2Of(Color color) => TextStyle(
      fontFamily: family, fontSize: 18, height: 24 / 18, fontWeight: FontWeight.w600, color: color);
  TextStyle monoOf(Color color, {double size = 13}) =>
      TextStyle(fontFamily: familyMono, fontSize: size, color: color);
}

/// Spacing / radius / motion scales.
class HarborSpace {
  static const double s1 = 4, s2 = 8, s3 = 12, s4 = 16, s5 = 20, s6 = 24, s8 = 32, s10 = 40, s12 = 48, s16 = 64;
}

class HarborRadius {
  static const double sm = 8, md = 12, lg = 16, xl = 22, pill = 999;
}

/// Layout constants from 06_Design_Tokens.json `layout`.
class HarborLayout {
  static const double desktopRail = 220;
  static const double desktopRailCollapsed = 72;
  static const double desktopLens = 320;
  static const double contentMax = 1440;
  static const double workCanvasMin = 640;
  static const double lensOverlayBelow = 1280;
  static const double railCollapseBelow = 1180;
  static const double workCanvasMinAppliesAtViewport = 1024;
}

/// Theme access.
@immutable
class HarborTheme extends InheritedWidget {
  const HarborTheme({
    super.key,
    required this.colors,
    required this.text,
    required super.child,
  });

  final HarborColors colors;
  final HarborType text;

  static HarborTheme of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<HarborTheme>()!;

  @override
  bool updateShouldNotify(HarborTheme oldWidget) =>
      colors != oldWidget.colors || text.arabic != oldWidget.text.arabic;
}

/// Build the Material ThemeData from Harbor tokens (light or dark).
ThemeData harborThemeData({required bool dark, required bool arabic}) {
  final c = dark ? HarborColors.dark : HarborColors.light;
  final t = HarborType(arabic: arabic);
  final colorScheme = ColorScheme(
    brightness: dark ? Brightness.dark : Brightness.light,
    primary: c.brand,
    onPrimary: c.onBrand,
    secondary: c.accent,
    onSecondary: c.ink,
    error: c.danger,
    onError: dark ? const Color(0xFF07111D) : const Color(0xFFFFFFFF),
    surface: c.surface,
    onSurface: c.ink,
    surfaceContainerHighest: c.surfaceRaised,
    outline: c.border,
  );
  return ThemeData(
    useMaterial3: true,
    colorScheme: colorScheme,
    scaffoldBackgroundColor: c.canvas,
    fontFamily: t.family,
    visualDensity: VisualDensity.standard,
    textTheme: TextTheme(
      bodyMedium: t.bodyOf(c.ink),
      bodySmall: t.smallOf(c.inkMuted),
      labelSmall: t.captionOf(c.inkMuted),
      titleLarge: t.titleOf(c.ink),
      titleMedium: t.h2Of(c.ink),
    ),
    navigationBarTheme: NavigationBarThemeData(
      backgroundColor: c.surface,
      indicatorColor: c.brandSoft,
      iconTheme: WidgetStateProperty.resolveWith((states) => IconThemeData(
          color: states.contains(WidgetState.selected) ? c.brand : c.inkMuted)),
    ),
    focusColor: c.focusRing,
  );
}
