import 'package:flutter/cupertino.dart' show CupertinoPageTransitionsBuilder;
import 'package:flutter/foundation.dart' show defaultTargetPlatform;
import 'package:flutter/material.dart';

/// Harbor Current 2 semantic color roles (06_Design_Tokens.json).
///
/// Values are the normative tokens verbatim. Anything derived (hover,
/// pressed, scrims) is computed from a role at use time so the audited
/// text/surface pairs stay the only pairs that carry text.
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

  bool get isDark => canvas.computeLuminance() < 0.5;

  /// Hover wash over a surface (never carries text of its own).
  Color get hover => ink.withValues(alpha: isDark ? 0.08 : 0.05);

  /// Pressed wash over a surface.
  Color get pressed => ink.withValues(alpha: isDark ? 0.14 : 0.09);

  /// Modal scrim behind sheets, dialogs and the transient Lens.
  Color get scrim => const Color(0xFF08131F).withValues(alpha: 0.48);

  /// Hairline separators inside cards (softer than [border]).
  Color get borderSubtle => border.withValues(alpha: 0.6);

  /// Skeleton/placeholder fill while data loads.
  Color get skeleton => ink.withValues(alpha: isDark ? 0.10 : 0.06);

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
///
/// Families follow the authority (§6): Inter / Noto Sans Arabic /
/// JetBrains Mono when bundled, otherwise the platform-equivalent family
/// through an explicit fallback chain — so monospace identifiers, hashes
/// and formulas always render in a real monospace face on every platform.
@immutable
class HarborType {
  const HarborType({required this.arabic});
  final bool arabic;

  static const familyLatin = 'Inter';
  static const familyArabic = 'Noto Sans Arabic';
  static const familyMono = 'JetBrains Mono';

  /// Platform-equivalent UI faces (Apple, Android, Windows, Linux).
  static const latinFallback = <String>[
    'SF Pro Text',
    '.SF UI Text',
    'Roboto',
    'Segoe UI',
    'Helvetica Neue',
    'Arial',
    'sans-serif',
  ];
  static const arabicFallback = <String>[
    'SF Arabic',
    'Geeza Pro',
    'Noto Naskh Arabic',
    'Segoe UI',
    'Roboto',
    'sans-serif',
  ];
  static const monoFallback = <String>[
    'SF Mono',
    'Menlo',
    'Roboto Mono',
    'Droid Sans Mono',
    'Consolas',
    'Courier New',
    'monospace',
  ];

  String get family => arabic ? familyArabic : familyLatin;
  List<String> get familyFallback => arabic ? arabicFallback : latinFallback;

  double get bodyLine => arabic ? 24 : 21;
  double get smallLine => arabic ? 20 : 17;
  double get captionLine => arabic ? 18 : 15;

  TextStyle _style({
    required double size,
    required double line,
    required double weight,
    required Color color,
    double? letterSpacing,
  }) {
    return TextStyle(
      fontFamily: family,
      fontFamilyFallback: familyFallback,
      fontSize: size,
      height: line / size,
      color: color,
      letterSpacing: letterSpacing,
      // Token weights are honored exactly on variable faces (650/620/450)
      // and rounded to the nearest static face otherwise.
      fontWeight: _nearestWeight(weight),
      fontVariations: [FontVariation.weight(weight)],
      leadingDistribution: TextLeadingDistribution.even,
    );
  }

  static FontWeight _nearestWeight(double w) {
    final index = ((w / 100).round() - 1).clamp(0, 8);
    return FontWeight.values[index];
  }

  /// 32/38 w650 — the Home headline on desktop.
  TextStyle displayOf(Color color) => _style(
      size: 32, line: 38, weight: 650, color: color, letterSpacing: -0.4);

  /// 24/30 w650 — surface titles.
  TextStyle titleOf(Color color) => _style(
      size: 24, line: 30, weight: 650, color: color, letterSpacing: -0.2);

  /// 18/24 w620 — section titles and sheet headings.
  TextStyle h2Of(Color color) =>
      _style(size: 18, line: 24, weight: 620, color: color);

  /// 14/21 (ar 24) w400 — body copy.
  TextStyle bodyOf(Color color) =>
      _style(size: 14, line: bodyLine, weight: 400, color: color);

  /// 14/21 (ar 24) w600 — emphasized body / list titles.
  TextStyle bodyStrongOf(Color color) =>
      _style(size: 14, line: bodyLine, weight: 600, color: color);

  /// 12/17 (ar 20) w400 — secondary lines.
  TextStyle smallOf(Color color) =>
      _style(size: 12, line: smallLine, weight: 400, color: color);

  /// 12/17 (ar 20) w600 — labels on chips, tabs, badges.
  TextStyle labelOf(Color color) =>
      _style(size: 12, line: smallLine, weight: 600, color: color);

  /// 11/15 (ar 18) w450 — metadata, never critical approval copy.
  TextStyle captionOf(Color color) =>
      _style(size: 11, line: captionLine, weight: 450, color: color);

  /// Monospace for identifiers, hashes, formulas and code. Callers wrap
  /// technical strings in an LTR [Directionality] (see HarborIdentifier).
  TextStyle monoOf(Color color, {double size = 13, double weight = 400}) =>
      TextStyle(
        fontFamily: familyMono,
        fontFamilyFallback: monoFallback,
        fontSize: size,
        height: 1.45,
        color: color,
        fontWeight: _nearestWeight(weight),
        fontVariations: [FontVariation.weight(weight)],
        fontFeatures: const [FontFeature.tabularFigures()],
      );
}

/// Spacing scale (06_Design_Tokens.json `space`).
class HarborSpace {
  static const double s1 = 4,
      s2 = 8,
      s3 = 12,
      s4 = 16,
      s5 = 20,
      s6 = 24,
      s8 = 32,
      s10 = 40,
      s12 = 48,
      s16 = 64;
}

class HarborRadius {
  static const double sm = 8, md = 12, lg = 16, xl = 22, pill = 999;
}

/// Stroke widths (`stroke` tokens).
class HarborStroke {
  static const double hairline = 1, focus = 2;
}

/// Motion scale (`motion` tokens). All durations collapse to zero when the
/// platform requests reduced motion (accessibility §15) — see [of].
@immutable
class HarborMotion {
  const HarborMotion._(this.reduced);

  /// Whether spatial/continuous animation is disabled for this build.
  final bool reduced;

  static const Duration instantMs = Duration(milliseconds: 80);
  static const Duration fastMs = Duration(milliseconds: 120);
  static const Duration normalMs = Duration(milliseconds: 180);
  static const Duration slowMs = Duration(milliseconds: 260);
  static const Duration panelMs = Duration(milliseconds: 320);

  /// cubic-bezier(.2,.8,.2,1)
  static const Curve easing = Cubic(0.2, 0.8, 0.2, 1);

  static HarborMotion of(BuildContext context) =>
      HarborMotion._(MediaQuery.maybeDisableAnimationsOf(context) ?? false);

  Duration get instant => reduced ? Duration.zero : instantMs;
  Duration get fast => reduced ? Duration.zero : fastMs;
  Duration get normal => reduced ? Duration.zero : normalMs;
  Duration get slow => reduced ? Duration.zero : slowMs;
  Duration get panel => reduced ? Duration.zero : panelMs;
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
  static const double compactMinimumViewport = 320;
  static const double mobileGutter = 16;
  static const double tabletGutter = 24;
  static const double desktopGutter = 28;

  /// Reading-width cap for prose (documents, answers, settings).
  static const double readingMax = 760;

  /// Minimum touch target on mobile / pointer target on desktop.
  static const double touchTarget = 44;
  static const double pointerTarget = 32;

  /// Compact top bar and bottom bar heights.
  static const double topBar = 56;
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

  bool get isDark => colors.isDark;

  static HarborTheme of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<HarborTheme>()!;

  static HarborTheme? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<HarborTheme>();

  @override
  bool updateShouldNotify(HarborTheme oldWidget) =>
      colors != oldWidget.colors || text.arabic != oldWidget.text.arabic;
}

/// Build the Material ThemeData from Harbor tokens (light or dark).
///
/// Every stock Material widget the app touches (buttons, chips, inputs,
/// tabs, cards, sheets, dialogs, navigation) is themed here so surfaces
/// never restyle widgets ad hoc. Desktop platforms use a denser
/// visual density; touch platforms keep the 44px target (tokens
/// `accessibility.minimumTouchTarget`).
ThemeData harborThemeData({
  required bool dark,
  required bool arabic,
  TargetPlatform? platform,
}) {
  final c = dark ? HarborColors.dark : HarborColors.light;
  final t = HarborType(arabic: arabic);
  final colorScheme = ColorScheme(
    brightness: dark ? Brightness.dark : Brightness.light,
    primary: c.brand,
    onPrimary: c.onBrand,
    primaryContainer: c.brandSoft,
    onPrimaryContainer: c.ink,
    secondary: c.accent,
    onSecondary: c.onBrand,
    secondaryContainer: c.brandSoft,
    onSecondaryContainer: c.ink,
    tertiary: c.local,
    onTertiary: c.onBrand,
    error: c.danger,
    onError: c.onBrand,
    errorContainer: c.statusDangerFill,
    onErrorContainer: c.statusDangerText,
    surface: c.surface,
    onSurface: c.ink,
    onSurfaceVariant: c.inkMuted,
    surfaceContainerLowest: c.surface,
    surfaceContainerLow: c.surface,
    surfaceContainer: c.surfaceRaised,
    surfaceContainerHigh: c.surfaceRaised,
    surfaceContainerHighest: c.surfaceRaised,
    surfaceDim: c.canvas,
    surfaceBright: c.surface,
    inverseSurface: c.ink,
    onInverseSurface: c.canvas,
    outline: c.border,
    outlineVariant: c.borderSubtle,
    shadow: const Color(0xFF000000),
    scrim: c.scrim,
  );
  final resolvedPlatform = platform ?? defaultTargetPlatform;
  final desktop = switch (resolvedPlatform) {
    TargetPlatform.macOS ||
    TargetPlatform.windows ||
    TargetPlatform.linux =>
      true,
    _ => false,
  };
  final shapeSm = RoundedRectangleBorder(
    borderRadius: BorderRadius.circular(HarborRadius.sm),
  );
  final shapeMd = RoundedRectangleBorder(
    borderRadius: BorderRadius.circular(HarborRadius.md),
  );
  final shapeLg = RoundedRectangleBorder(
    borderRadius: BorderRadius.circular(HarborRadius.lg),
  );
  final minTarget = desktop
      ? const Size(
          HarborLayout.pointerTarget + 4, HarborLayout.pointerTarget + 4)
      : const Size(HarborLayout.touchTarget, HarborLayout.touchTarget);
  final buttonPadding = EdgeInsets.symmetric(
    horizontal: HarborSpace.s4,
    vertical: desktop ? HarborSpace.s2 : HarborSpace.s3,
  );
  final labelStyle = t.labelOf(c.ink).copyWith(fontSize: 13);

  return ThemeData(
    useMaterial3: true,
    platform: resolvedPlatform,
    colorScheme: colorScheme,
    scaffoldBackgroundColor: c.canvas,
    canvasColor: c.surface,
    fontFamily: t.family,
    fontFamilyFallback: t.familyFallback,
    visualDensity: VisualDensity.standard,
    splashFactory: desktop ? NoSplash.splashFactory : InkSparkle.splashFactory,
    hoverColor: c.hover,
    highlightColor: c.pressed,
    splashColor: c.pressed,
    focusColor: c.focusRing.withValues(alpha: 0.24),
    dividerColor: c.border,
    shadowColor: const Color(0xFF000000),
    textTheme: TextTheme(
      displaySmall: t.displayOf(c.ink),
      headlineSmall: t.titleOf(c.ink),
      titleLarge: t.titleOf(c.ink),
      titleMedium: t.h2Of(c.ink),
      titleSmall: t.bodyStrongOf(c.ink),
      bodyLarge: t.bodyOf(c.ink),
      bodyMedium: t.bodyOf(c.ink),
      bodySmall: t.smallOf(c.inkMuted),
      labelLarge: labelStyle,
      labelMedium: t.labelOf(c.ink),
      labelSmall: t.captionOf(c.inkMuted),
    ),
    iconTheme: IconThemeData(color: c.inkMuted, size: 20),
    primaryIconTheme: IconThemeData(color: c.brand, size: 20),
    appBarTheme: AppBarTheme(
      backgroundColor: c.surface,
      foregroundColor: c.ink,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      scrolledUnderElevation: 0,
      centerTitle: false,
      titleTextStyle: t.h2Of(c.ink),
      toolbarHeight: HarborLayout.topBar,
      shape: Border(bottom: BorderSide(color: c.border)),
    ),
    cardTheme: CardThemeData(
      color: c.surface,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      margin: EdgeInsets.zero,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(HarborRadius.md),
        side: BorderSide(color: c.border),
      ),
    ),
    dividerTheme: DividerThemeData(color: c.border, thickness: 1, space: 1),
    listTileTheme: ListTileThemeData(
      iconColor: c.inkMuted,
      textColor: c.ink,
      titleTextStyle: t.bodyStrongOf(c.ink),
      subtitleTextStyle: t.smallOf(c.inkMuted),
      minVerticalPadding: HarborSpace.s2,
      contentPadding: const EdgeInsets.symmetric(
          horizontal: HarborSpace.s4, vertical: HarborSpace.s1),
      shape: shapeSm,
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        backgroundColor: c.primaryActionFill,
        foregroundColor: c.primaryActionText,
        disabledBackgroundColor: c.ink.withValues(alpha: 0.10),
        disabledForegroundColor: c.inkMuted,
        minimumSize: minTarget,
        padding: buttonPadding,
        shape: shapeSm,
        textStyle: labelStyle,
        elevation: 0,
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        foregroundColor: c.ink,
        minimumSize: minTarget,
        padding: buttonPadding,
        shape: shapeSm,
        side: BorderSide(color: c.border),
        textStyle: labelStyle,
      ),
    ),
    textButtonTheme: TextButtonThemeData(
      style: TextButton.styleFrom(
        foregroundColor: c.brand,
        minimumSize: minTarget,
        padding: buttonPadding,
        shape: shapeSm,
        textStyle: labelStyle,
      ),
    ),
    iconButtonTheme: IconButtonThemeData(
      style: IconButton.styleFrom(
        foregroundColor: c.inkMuted,
        minimumSize: minTarget,
        shape: shapeSm,
      ),
    ),
    segmentedButtonTheme: SegmentedButtonThemeData(
      style: SegmentedButton.styleFrom(
        backgroundColor: c.surface,
        foregroundColor: c.ink,
        selectedBackgroundColor: c.brandSoft,
        selectedForegroundColor: c.ink,
        side: BorderSide(color: c.border),
        textStyle: labelStyle,
        minimumSize: minTarget,
        padding: const EdgeInsets.symmetric(horizontal: HarborSpace.s3),
      ),
    ),
    chipTheme: ChipThemeData(
      backgroundColor: c.surface,
      selectedColor: c.brandSoft,
      disabledColor: c.surface,
      side: BorderSide(color: c.border),
      shape: shapeSm,
      labelStyle: t.labelOf(c.ink).copyWith(fontSize: 13),
      secondaryLabelStyle: t.labelOf(c.ink).copyWith(fontSize: 13),
      iconTheme: IconThemeData(color: c.brand, size: 16),
      padding: EdgeInsets.symmetric(
          horizontal: HarborSpace.s3,
          vertical: desktop ? HarborSpace.s2 : HarborSpace.s3),
      showCheckmark: false,
      elevation: 0,
      pressElevation: 0,
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: c.surface,
      hintStyle: t.bodyOf(c.inkMuted),
      labelStyle: t.smallOf(c.inkMuted),
      helperStyle: t.captionOf(c.inkMuted),
      errorStyle: t.captionOf(c.statusDangerText),
      contentPadding: const EdgeInsets.symmetric(
          horizontal: HarborSpace.s3, vertical: HarborSpace.s3),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(HarborRadius.sm),
        borderSide: BorderSide(color: c.border),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(HarborRadius.sm),
        borderSide: BorderSide(color: c.border),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(HarborRadius.sm),
        borderSide: BorderSide(color: c.focusRing, width: HarborStroke.focus),
      ),
      errorBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(HarborRadius.sm),
        borderSide: BorderSide(color: c.danger),
      ),
    ),
    tabBarTheme: TabBarThemeData(
      labelColor: c.brand,
      unselectedLabelColor: c.inkMuted,
      labelStyle: labelStyle,
      unselectedLabelStyle: labelStyle,
      indicatorColor: c.brand,
      indicatorSize: TabBarIndicatorSize.label,
      dividerColor: c.border,
      overlayColor: WidgetStatePropertyAll(c.hover),
      tabAlignment: TabAlignment.start,
    ),
    navigationBarTheme: NavigationBarThemeData(
      backgroundColor: c.surface,
      surfaceTintColor: Colors.transparent,
      indicatorColor: c.brandSoft,
      elevation: 0,
      height: 72,
      labelTextStyle: WidgetStateProperty.resolveWith((states) => t.captionOf(
          states.contains(WidgetState.selected) ? c.brand : c.inkMuted)),
      iconTheme: WidgetStateProperty.resolveWith((states) => IconThemeData(
          size: 22,
          color: states.contains(WidgetState.selected) ? c.brand : c.inkMuted)),
    ),
    navigationRailTheme: NavigationRailThemeData(
      backgroundColor: c.surface,
      indicatorColor: c.brandSoft,
      selectedIconTheme: IconThemeData(color: c.brand, size: 22),
      unselectedIconTheme: IconThemeData(color: c.inkMuted, size: 22),
      selectedLabelTextStyle: t.captionOf(c.brand),
      unselectedLabelTextStyle: t.captionOf(c.inkMuted),
    ),
    bottomSheetTheme: BottomSheetThemeData(
      backgroundColor: c.surface,
      surfaceTintColor: Colors.transparent,
      modalBackgroundColor: c.surface,
      modalBarrierColor: c.scrim,
      dragHandleColor: c.border,
      showDragHandle: true,
      shape: const RoundedRectangleBorder(
        borderRadius:
            BorderRadius.vertical(top: Radius.circular(HarborRadius.xl)),
      ),
      clipBehavior: Clip.antiAlias,
    ),
    dialogTheme: DialogThemeData(
      backgroundColor: c.surface,
      surfaceTintColor: Colors.transparent,
      barrierColor: c.scrim,
      shape: shapeLg,
      titleTextStyle: t.h2Of(c.ink),
      contentTextStyle: t.bodyOf(c.ink),
    ),
    snackBarTheme: SnackBarThemeData(
      backgroundColor: c.ink,
      contentTextStyle: t.bodyOf(c.canvas),
      // Snackbars sit on `ink`, so the action uses the opposite theme's
      // blue (light focus-ring on navy, brand blue on near-white).
      actionTextColor:
          dark ? HarborColors.light.brand : HarborColors.dark.focusRing,
      behavior: SnackBarBehavior.floating,
      shape: shapeSm,
      insetPadding: const EdgeInsets.all(HarborSpace.s4),
    ),
    tooltipTheme: TooltipThemeData(
      decoration: ShapeDecoration(color: c.ink, shape: shapeSm),
      textStyle: t.smallOf(c.canvas),
      padding: const EdgeInsets.symmetric(
          horizontal: HarborSpace.s3, vertical: HarborSpace.s2),
      waitDuration: const Duration(milliseconds: 400),
    ),
    progressIndicatorTheme: ProgressIndicatorThemeData(
      color: c.brand,
      linearTrackColor: c.brandSoft,
      circularTrackColor: c.brandSoft,
      linearMinHeight: 6,
      borderRadius: BorderRadius.circular(HarborRadius.pill),
    ),
    dropdownMenuTheme: DropdownMenuThemeData(
      textStyle: t.bodyOf(c.ink),
      menuStyle: MenuStyle(
        backgroundColor: WidgetStatePropertyAll(c.surface),
        surfaceTintColor: const WidgetStatePropertyAll(Colors.transparent),
        shape: WidgetStatePropertyAll(shapeMd),
        side: WidgetStatePropertyAll(BorderSide(color: c.border)),
      ),
    ),
    menuTheme: MenuThemeData(
      style: MenuStyle(
        backgroundColor: WidgetStatePropertyAll(c.surface),
        surfaceTintColor: const WidgetStatePropertyAll(Colors.transparent),
        shape: WidgetStatePropertyAll(shapeMd),
        side: WidgetStatePropertyAll(BorderSide(color: c.border)),
      ),
    ),
    popupMenuTheme: PopupMenuThemeData(
      color: c.surface,
      surfaceTintColor: Colors.transparent,
      shape: shapeMd,
      textStyle: t.bodyOf(c.ink),
    ),
    radioTheme: RadioThemeData(
      fillColor: WidgetStateProperty.resolveWith((states) =>
          states.contains(WidgetState.selected) ? c.brand : c.inkMuted),
    ),
    switchTheme: SwitchThemeData(
      thumbColor: WidgetStateProperty.resolveWith((states) =>
          states.contains(WidgetState.selected) ? c.onBrand : c.inkMuted),
      trackColor: WidgetStateProperty.resolveWith((states) =>
          states.contains(WidgetState.selected) ? c.brand : c.surfaceRaised),
      trackOutlineColor: WidgetStatePropertyAll(c.border),
    ),
    checkboxTheme: CheckboxThemeData(
      fillColor: WidgetStateProperty.resolveWith((states) =>
          states.contains(WidgetState.selected) ? c.brand : Colors.transparent),
      checkColor: WidgetStatePropertyAll(c.onBrand),
      side: BorderSide(color: c.inkMuted, width: 1.5),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(4)),
    ),
    scrollbarTheme: ScrollbarThemeData(
      thumbColor: WidgetStatePropertyAll(c.inkMuted.withValues(alpha: 0.45)),
      radius: const Radius.circular(HarborRadius.pill),
      thickness: const WidgetStatePropertyAll(6),
    ),
    pageTransitionsTheme: const PageTransitionsTheme(builders: {
      TargetPlatform.iOS: CupertinoPageTransitionsBuilder(),
      TargetPlatform.android: PredictiveBackPageTransitionsBuilder(),
      TargetPlatform.macOS: FadeForwardsPageTransitionsBuilder(),
      TargetPlatform.windows: FadeForwardsPageTransitionsBuilder(),
      TargetPlatform.linux: FadeForwardsPageTransitionsBuilder(),
    }),
    extensions: const [],
  );
}
