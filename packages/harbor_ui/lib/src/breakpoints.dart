import 'package:flutter/widgets.dart';

import 'tokens.dart';

/// Responsive contract (UI authority §4/§20, 06_Design_Tokens breakpoints).
///
/// compact     <=599   bottom navigation, Lens is a modal sheet
/// medium      600-1023 compact navigation, Lens transient
/// expanded    1024-1179 72px collapsed rail, Lens overlay/drawer,
///                     Work Canvas keeps >= 640 (desktop-only rule)
/// wide-rail   1180-1279 rail may expand toward 220 only when the canvas
///                     stays viable; Lens transient
/// full        1280+   220px rail + up to 320px Lens, canvas >= 640
enum HarborWindowClass { compact, medium, expanded, wideRail, full }

class HarborBreakpoints {
  static HarborWindowClass classify(double width) {
    if (width <= 599) return HarborWindowClass.compact;
    if (width <= 1023) return HarborWindowClass.medium;
    if (width <= 1179) return HarborWindowClass.expanded;
    if (width <= 1279) return HarborWindowClass.wideRail;
    return HarborWindowClass.full;
  }

  static HarborWindowClass of(BuildContext context) =>
      classify(MediaQuery.sizeOf(context).width);

  /// Rail width for the window class (0 = no rail).
  static double railWidth(HarborWindowClass c) => switch (c) {
        HarborWindowClass.compact || HarborWindowClass.medium => 0,
        HarborWindowClass.expanded => HarborLayout.desktopRailCollapsed,
        _ => HarborLayout.desktopRail,
      };

  /// Lens is a persistent side panel only at 1280+; below it is transient.
  static bool lensIsPersistent(HarborWindowClass c) =>
      c == HarborWindowClass.full;

  /// Work Canvas minimum applies only from 1024 up (desktop rule); compact
  /// editing uses viewport-sized controls instead.
  static bool enforceWorkCanvasMin(HarborWindowClass c) {
    switch (c) {
      case HarborWindowClass.compact:
      case HarborWindowClass.medium:
        return false;
      case HarborWindowClass.expanded:
      case HarborWindowClass.wideRail:
      case HarborWindowClass.full:
        return true;
    }
  }

  /// Residual canvas width after rail (and persistent lens) at `width`.
  static double workCanvasWidth(HarborWindowClass c, double width) {
    var remaining = width - railWidth(c);
    if (lensIsPersistent(c)) remaining -= HarborLayout.desktopLens;
    return remaining;
  }

  /// Page gutter per class (`layout.mobileGutter/tabletGutter/desktopGutter`).
  static double gutter(HarborWindowClass c) => switch (c) {
        HarborWindowClass.compact => HarborLayout.mobileGutter,
        HarborWindowClass.medium => HarborLayout.tabletGutter,
        _ => HarborLayout.desktopGutter,
      };

  /// Compact = phone-class: bottom navigation, sheets instead of panels.
  static bool isCompact(HarborWindowClass c) => c == HarborWindowClass.compact;

  /// A side rail (compact or full) is present from 1024 up; the medium
  /// class uses the platform's compact side pattern (icon + label rail).
  static bool hasRail(HarborWindowClass c) => c != HarborWindowClass.compact;

  /// Whether the rail shows text labels beside icons (220px rail).
  static bool railIsExtended(HarborWindowClass c) =>
      railWidth(c) >= HarborLayout.desktopRail;

  /// Two-column surface layouts (main + aside) need a canvas of at least
  /// this width; below it sections stack.
  static const double twoColumnMinCanvas = 900;
}
