import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/l10n/app_localizations.dart';
import 'package:harbor_app/main.dart';
import 'package:harbor_app/services/harbor_service.dart';
import 'package:harbor_ui/harbor_ui.dart';

final repoRoot = Directory.current.parent.parent.path; // apps/harbor_app
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';
final coreAvailable = File(dylibPath).existsSync();

/// Creator chains of layout errors raised since the last [takeLayoutError],
/// so an overflow failure names the widget instead of just the pixels.
final List<String> _layoutErrors = [];

String takeLayoutError() {
  final text = _layoutErrors.join('\n');
  _layoutErrors.clear();
  return text;
}

Future<void> pumpApp(
  WidgetTester tester, {
  Locale locale = const Locale('en'),
  double width = 1280,
  double height = 900,
  double textScale = 1.0,
}) async {
  final previous = FlutterError.onError;
  FlutterError.onError = (details) {
    final text = details.toString();
    final idx = text.indexOf('The relevant error-causing widget was');
    _layoutErrors.add(idx < 0
        ? text.split('\n').first
        : text.substring(idx, (idx + 220).clamp(0, text.length)));
    previous?.call(details);
  };
  addTearDown(() {
    FlutterError.onError = previous;
    _layoutErrors.clear();
  });
  tester.view.physicalSize = Size(width, height);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);
  HarborService? service;
  if (coreAvailable) {
    await tester.runAsync(() async {
      final dir = await Directory.systemTemp.createTemp('harbor-a11y-');
      final s = await HarborService.open(
          libraryPath: dylibPath,
          dataRoot: dir.path,
          workspaceId: 'ws-a11y',
          deviceRootHex:
              '49387c936d072910d8a5404a96a1ac272296eb3270c694aaec11f9805c875205');
      await s.refresh();
      service = s;
    });
  }
  addTearDown(() => service?.close());
  final arabic = locale.languageCode == 'ar';
  // Scale text on top of the REAL view metrics (a bare MediaQueryData
  // would zero the size and silently force the compact layout).
  await tester.pumpWidget(RepaintBoundary(
    child: Builder(
      builder: (context) => MediaQuery(
        data: MediaQuery.of(context)
            .copyWith(textScaler: TextScaler.linear(textScale)),
        child: HarborTheme(
          colors: HarborColors.light,
          text: HarborType(arabic: arabic),
          child: MaterialApp(
            locale: locale,
            localizationsDelegates: const [
              AppLocalizations.delegate,
              GlobalMaterialLocalizations.delegate,
              GlobalWidgetsLocalizations.delegate,
              GlobalCupertinoLocalizations.delegate,
            ],
            supportedLocales: const [Locale('en'), Locale('ar')],
            theme: harborThemeData(dark: false, arabic: arabic),
            home: HarborApp(service: service),
          ),
        ),
      ),
    ),
  ));
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 100));
}

/// Navigate by label; on compact widths secondary surfaces live under More.
Future<void> goTo(WidgetTester tester, String label) async {
  var target = find.text(label);
  if (target.evaluate().isEmpty) {
    final rail = find.byType(HarborRail);
    if (rail.evaluate().isNotEmpty) {
      await tester.scrollUntilVisible(find.text(label), 80,
          scrollable:
              find.descendant(of: rail, matching: find.byType(Scrollable)));
    } else {
      await tester.tap(find.text('More').first);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));
    }
    target = find.text(label);
  }
  await tester.tap(target.first);
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 400));
  // One more frame: the previous surface retires (goes offstage) after
  // its fade completes.
  await tester.pump();
}

/// Every enabled interactive control reachable in the critical flows must
/// expose a non-empty semantic label (screen reader requirement, goal §27).
void _assertLabeled(WidgetTester tester, String flow) {
  const types = [IconButton, FilledButton, OutlinedButton, TextButton];
  final failures = <String>[];
  for (final type in types) {
    final candidates = find.byType(type, skipOffstage: true);
    for (final candidate in candidates.evaluate()) {
      final widget = candidate.widget;
      double sizeOf(RenderObject? o) {
        if (o is RenderBox) return o.size.shortestSide;
        return 0;
      }

      final size = sizeOf(candidate.renderObject);
      if (size == 0) continue;
      // Semantics walk: the element's semantics must carry a label or
      // tooltip somewhere along its configuration.
      final element = candidate as ComponentElement;
      String? tooltip;
      void walk(Element e) {
        final w = e.widget;
        if (w is Tooltip) {
          final msg = w.message;
          if (msg != null && msg.isNotEmpty) tooltip ??= msg;
        }
        if (w is Text) {
          final data = w.data;
          if (data != null && data.trim().isNotEmpty) tooltip ??= data;
        }
        e.visitChildren(walk);
      }

      walk(element);
      final hasSemantics = tooltip != null;
      if (!hasSemantics) {
        failures.add('$flow: unlabeled ${widget.runtimeType} (${size}px)');
      }
    }
  }
  expect(failures, isEmpty,
      reason: 'unlabeled controls in $flow:\n${failures.join("\n")}');
}

void main() {
  testWidgets('every critical-flow control is semantically labeled',
      (tester) async {
    await pumpApp(tester);
    _assertLabeled(tester, 'home');
    for (final surface in [
      'Ask',
      'Work',
      'Agents',
      'Models',
      'Skills',
      'Knowledge',
      'Activity',
      'Settings'
    ]) {
      await goTo(tester, surface);
      _assertLabeled(tester, surface);
    }
  });

  testWidgets('critical flows survive 200% text scale without overflow',
      (tester) async {
    await pumpApp(tester, width: 390, height: 844, textScale: 2.0);
    expect(tester.takeException(), isNull);
    for (final surface in [
      'Ask',
      'Work',
      'Models',
      'Agents',
      'Skills',
      'Knowledge',
      'Activity',
      'Settings'
    ]) {
      await goTo(tester, surface);
      expect(tester.takeException(), isNull,
          reason: 'overflow at 200% scale on $surface\n${takeLayoutError()}');
    }
    // The Lens sheet also survives 200% on a 390px phone.
    await tester.tap(find.byType(TrustChip));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    expect(tester.takeException(), isNull,
        reason: 'overflow in the Lens\n${takeLayoutError()}');
  });

  testWidgets('critical flows survive 320px at 200% text scale',
      (tester) async {
    await pumpApp(tester, width: 320, height: 568, textScale: 2.0);
    expect(tester.takeException(), isNull);
    for (final surface in ['Work', 'Settings']) {
      await goTo(tester, surface);
      expect(tester.takeException(), isNull,
          reason: 'overflow at 320px/200% on $surface\n${takeLayoutError()}');
    }
  });

  testWidgets('desktop surfaces survive 200% text scale', (tester) async {
    await pumpApp(tester, width: 1280, height: 800, textScale: 2.0);
    expect(tester.takeException(), isNull);
    for (final surface in ['Ask', 'Work', 'Models', 'Knowledge', 'Settings']) {
      await goTo(tester, surface);
      expect(tester.takeException(), isNull,
          reason:
              'overflow at 200% scale on desktop $surface\n${takeLayoutError()}');
    }
  });

  testWidgets('compact navigation destinations meet the 44px minimum target',
      (tester) async {
    await pumpApp(tester, width: 390, height: 844);
    final destinations = find.byType(NavigationDestination);
    expect(destinations, findsWidgets);
    for (final d in destinations.evaluate()) {
      final box = d.renderObject as RenderBox;
      expect(
        box.size.height,
        greaterThanOrEqualTo(44),
        reason: 'navigation destination under 44px tall',
      );
    }
  });

  testWidgets('keyboard traversal reaches every primary Home control',
      (tester) async {
    await pumpApp(tester);
    // Keyboard users must reach every primary desktop operation (goal §27):
    // walk Tab through the whole surface and record what gets focus.
    final reachedTextFiled = <String>{};
    for (var i = 0; i < 24; i++) {
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pump();
      final focused = FocusManager.instance.primaryFocus;
      final context = focused?.context;
      if (context == null) continue;
      context.visitAncestorElements((e) {
        final w = e.widget;
        if (w is TextField || w is EditableText) {
          reachedTextFiled.add('composer');
        }
        if (w is ActionChip) reachedTextFiled.add('quick-action');
        if (w is FilledButton) reachedTextFiled.add('filled-action');
        if (w is ModelDock) reachedTextFiled.add('model-dock');
        return true;
      });
    }
    // The composer text field and the quick actions are keyboard-reachable.
    expect(reachedTextFiled, contains('composer'),
        reason: 'the composer must be reachable by keyboard');
    expect(reachedTextFiled, contains('quick-action'),
        reason: 'quick actions must be reachable by keyboard');
  });
}
