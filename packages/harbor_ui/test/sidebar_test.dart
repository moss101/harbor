import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/harbor_ui.dart';

Widget _host(Widget child, {TextDirection dir = TextDirection.ltr}) =>
    HarborTheme(
      colors: HarborColors.light,
      text: const HarborType(arabic: false),
      child: MaterialApp(
        theme: harborThemeData(dark: false, arabic: false),
        home: Directionality(
          textDirection: dir,
          child: Scaffold(body: Row(children: [child, const Spacer()])),
        ),
      ),
    );

const _destinations = [
  HarborDestination('Home', Icons.home_outlined),
  HarborDestination('Work', Icons.description_outlined),
  HarborDestination('Library', Icons.local_library_outlined),
];

void main() {
  testWidgets('open: labels, the primary action and the section show',
      (tester) async {
    var picked = -1;
    var created = false;
    await tester.pumpWidget(_host(HarborSidebar(
      destinations: _destinations,
      selectedIndex: 0,
      onSelected: (i) => picked = i,
      primaryAction: HarborSidebarAction(
          label: 'New thread',
          icon: Icons.add,
          onPressed: () => created = true),
      section: const Text('Recent'),
    )));
    expect(
        tester.getSize(find.byType(HarborSidebar)).width, HarborLayout.sidebar);
    expect(find.text('Recent'), findsOneWidget);
    await tester.tap(find.text('Library'));
    expect(picked, 2);
    await tester.tap(find.text('New thread'));
    expect(created, isTrue);
    expect(
        tester
            .widget<HarborSidebarRow>(find.byType(HarborSidebarRow).first)
            .selected,
        isTrue);
    expect(find.bySemanticsLabel('Home'), findsOneWidget);
  });

  testWidgets('collapsed: icons with tooltips, no section, toggle works',
      (tester) async {
    var toggled = false;
    await tester.pumpWidget(_host(HarborSidebar(
      destinations: _destinations,
      selectedIndex: 1,
      onSelected: (_) {},
      collapsed: true,
      onToggleCollapsed: () => toggled = true,
      toggleTooltip: 'Expand sidebar',
      section: const Text('Recent'),
    )));
    expect(tester.getSize(find.byType(HarborSidebar)).width,
        HarborLayout.sidebarCollapsed);
    expect(find.text('Work'), findsNothing);
    expect(find.text('Recent'), findsNothing);
    expect(find.byTooltip('Work'), findsOneWidget);
    await tester.tap(find.byTooltip('Expand sidebar'));
    expect(toggled, isTrue);
  });

  testWidgets('mirrors in RTL: the sidebar sits at the right edge',
      (tester) async {
    tester.view.physicalSize = const Size(800, 600);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(_host(
        HarborSidebar(
            destinations: _destinations, selectedIndex: 0, onSelected: (_) {}),
        dir: TextDirection.rtl));
    expect(tester.getRect(find.byType(HarborSidebar)).right, 800);
  });

  testWidgets('a source chip shows its number and title and is a button',
      (tester) async {
    var tapped = false;
    await tester.pumpWidget(_host(HarborSourceChip(
      index: 2,
      label: 'A very long source title that has to ellipsize in the chip.pdf',
      semanticLabel: 'Open source 2',
      onTap: () => tapped = true,
    )));
    expect(find.text('2'), findsOneWidget);
    expect(tester.getSize(find.byType(HarborSourceChip)).width,
        lessThanOrEqualTo(220));
    expect(tester.takeException(), isNull);
    await tester.tap(find.byType(HarborSourceChip));
    expect(tapped, isTrue);
    expect(find.bySemanticsLabel('Open source 2'), findsOneWidget);
  });
}
