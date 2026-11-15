import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/harbor_ui.dart';

/// Navigate the answer-first shell to the surface labelled [label]: a
/// sidebar row for Home · Work · Library, the footer's More menu for
/// Models · Skills · Activity · Settings. On phones the sidebar lives in a
/// drawer, which is opened first.
Future<void> navigateTo(WidgetTester tester, String label) async {
  Future<void> settle() async {
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 400));
    await tester.pump();
  }

  if (find.byType(HarborSidebar).evaluate().isEmpty) {
    await tester.tap(find.byIcon(Icons.menu).first);
    await settle();
  }
  final row = find.descendant(
      of: find.byType(HarborSidebar),
      matching: find
          .byWidgetPredicate((w) => w is HarborSidebarRow && w.label == label));
  if (row.evaluate().isNotEmpty) {
    await tester.tap(row.first);
  } else {
    await tester.tap(find.byKey(const ValueKey('shell-more')).first);
    await settle();
    await tester.tap(find.widgetWithText(MenuItemButton, label).last);
  }
  await settle();
  // The phone drawer closes after the menu does; one more beat for it and
  // for the previous surface to retire (go offstage) after its fade.
  await settle();
}
