import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/slides.dart';

SlideScene slide(
  int index,
  String bg, {
  SceneTransition? transition,
  List<SceneAnimation> animations = const [],
  String? notes,
}) =>
    SlideScene(
      index: index,
      background: ScenePaint(kind: PaintKind.solid, c: bg, a: 1),
      transition: transition,
      animations: animations,
      notes: notes,
    );

SceneAnimation fade(int id,
        {String trigger = 'click', int dur = 300, String phase = 'entrance'}) =>
    SceneAnimation(
      shapeId: id,
      effect: 'fade',
      phase: phase,
      trigger: trigger,
      durMs: dur,
    );

DeckScene deck3({int? afterMs}) => DeckScene(width: 16, height: 9, slides: [
      slide(1, '#FF0000',
          notes: 'Notes one',
          transition: afterMs == null
              ? null
              : SceneTransition(kind: 'fade', durMs: 100, afterMs: afterMs),
          animations: [fade(1), fade(2)]),
      slide(2, '#00FF00',
          transition: const SceneTransition(kind: 'push', dir: 'l', durMs: 400),
          animations: [fade(3, trigger: 'with_prev')]),
      slide(3, '#0000FF',
          transition: const SceneTransition(kind: 'fade', durMs: 300)),
    ]);

Future<void> host(
  WidgetTester tester,
  DeckScene deck, {
  int start = 0,
  bool? reduce,
  VoidCallback? onExit,
  TextDirection dir = TextDirection.ltr,
  Size size = const Size(800, 450),
  PresentStrings strings = const PresentStrings(),
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(MaterialApp(
    home: Directionality(
      textDirection: dir,
      child: PresentView(
        deck: deck,
        startIndex: start,
        reduceMotion: reduce,
        onExit: onExit,
        strings: strings,
      ),
    ),
  ));
  await tester.pump();
}

Future<void> press(WidgetTester tester, LogicalKeyboardKey k) async {
  await tester.sendKeyEvent(k);
  await tester.pump();
}

Future<void> settle(WidgetTester tester, [int ms = 1000]) async {
  await tester.pump(Duration(milliseconds: ms));
}

SlideView view(WidgetTester tester) =>
    tester.widget<SlideView>(find.byType(SlideView));

int shown(WidgetTester tester) => view(tester).scene.index;

Future<void> unmount(WidgetTester tester) =>
    tester.pumpWidget(const SizedBox.shrink());

void main() {
  testWidgets('starts on the first slide with its entrances hidden',
      (tester) async {
    await host(tester, deck3());
    expect(find.text('Slide 1 of 3'), findsOneWidget);
    expect(shown(tester), 1);
    expect(view(tester).hidden, {1, 2});
    await unmount(tester);
  });

  testWidgets('startIndex opens at that slide', (tester) async {
    await host(tester, deck3(), start: 2);
    expect(find.text('Slide 3 of 3'), findsOneWidget);
    expect(shown(tester), 3);
    await unmount(tester);
  });

  testWidgets(
      'next plays click steps, then transitions, then auto-plays step 0',
      (tester) async {
    await host(tester, deck3());
    await press(tester, LogicalKeyboardKey.space); // click 1
    await settle(tester, 100);
    expect(view(tester).hidden, {2}); // 1 started
    await settle(tester, 400);
    expect(view(tester).effects, isEmpty);
    expect(view(tester).hidden, {2});

    await press(tester, LogicalKeyboardKey.arrowRight); // click 2
    await settle(tester, 500);
    expect(view(tester).hidden, isEmpty);
    expect(find.byType(TransitionView), findsNothing);

    await press(tester, LogicalKeyboardKey.pageDown); // transition
    expect(find.byType(TransitionView), findsOneWidget);
    expect(find.text('Slide 1 of 3'), findsOneWidget); // not yet entered
    await settle(tester, 200);
    expect(find.byType(TransitionView), findsOneWidget);
    await settle(tester, 300);
    expect(find.byType(TransitionView), findsNothing);
    expect(find.text('Slide 2 of 3'), findsOneWidget);
    expect(shown(tester), 2);
    // Slide 2's with_prev animation auto-plays: faded in part-way...
    expect(view(tester).effects.containsKey(3), isTrue);
    await settle(tester, 400);
    expect(view(tester).effects, isEmpty);
    expect(view(tester).hidden, isEmpty);
    await unmount(tester);
  });

  testWidgets('the incoming slide is painted with its entrances hidden',
      (tester) async {
    await host(tester, deck3(), start: 0);
    await press(tester, LogicalKeyboardKey.end);
    await press(tester, LogicalKeyboardKey.home);
    // Run to the end of slide 1.
    for (var i = 0; i < 2; i++) {
      await press(tester, LogicalKeyboardKey.space);
      await settle(tester, 400);
    }
    await press(tester, LogicalKeyboardKey.space);
    final tv = tester.widget<TransitionView>(find.byType(TransitionView));
    expect(tv.toState.hidden, {3});
    expect(tv.fromState.hidden, isEmpty);
    await unmount(tester);
  });

  testWidgets('next during a running step completes it without advancing',
      (tester) async {
    await host(tester, deck3());
    await press(tester, LogicalKeyboardKey.space);
    await settle(tester, 50);
    await press(tester, LogicalKeyboardKey.space); // completes click 1
    expect(view(tester).hidden, {2});
    expect(view(tester).effects, isEmpty);
    await unmount(tester);
  });

  testWidgets('previous steps back, then to the previous slide\'s end',
      (tester) async {
    await host(tester, deck3(), start: 1, reduce: true);
    await press(tester, LogicalKeyboardKey.arrowLeft);
    expect(shown(tester), 1);
    await press(tester, LogicalKeyboardKey.arrowLeft); // nothing before it
    expect(shown(tester), 1);
    await unmount(tester);

    await host(tester, deck3());
    await press(tester, LogicalKeyboardKey.space);
    await settle(tester, 400);
    await press(tester, LogicalKeyboardKey.space);
    await settle(tester, 400);
    await press(tester, LogicalKeyboardKey.space); // -> slide 2
    await settle(tester, 1500);
    expect(shown(tester), 2);
    await press(tester, LogicalKeyboardKey.pageUp);
    expect(shown(tester), 1);
    expect(view(tester).hidden, isEmpty); // end state: both steps played
    await press(tester, LogicalKeyboardKey.arrowUp); // back one step
    expect(view(tester).hidden, {2});
    await unmount(tester);
  });

  testWidgets('home, end and escape', (tester) async {
    var exited = 0;
    await host(tester, deck3(), onExit: () => exited++);
    await press(tester, LogicalKeyboardKey.end);
    expect(find.text('Slide 3 of 3'), findsOneWidget);
    await press(tester, LogicalKeyboardKey.home);
    expect(find.text('Slide 1 of 3'), findsOneWidget);
    await press(tester, LogicalKeyboardKey.escape);
    expect(exited, 1);
    await tester.tap(find.byTooltip('Exit presentation'));
    await tester.pump();
    expect(exited, 2);
    await unmount(tester);
  });

  testWidgets('B and W blank the screen; any navigation restores it',
      (tester) async {
    final handle = tester.ensureSemantics();
    await host(tester, deck3(), reduce: true);
    expect(find.bySemanticsLabel('Black screen'), findsNothing);
    await press(tester, LogicalKeyboardKey.keyB);
    expect(find.bySemanticsLabel('Black screen'), findsOneWidget);
    await press(tester, LogicalKeyboardKey.keyB);
    expect(find.bySemanticsLabel('Black screen'), findsNothing);
    await press(tester, LogicalKeyboardKey.keyW);
    expect(find.bySemanticsLabel('White screen'), findsOneWidget);
    await press(tester, LogicalKeyboardKey.keyB); // switches to black
    expect(find.bySemanticsLabel('White screen'), findsNothing);
    expect(find.bySemanticsLabel('Black screen'), findsOneWidget);
    await press(tester, LogicalKeyboardKey.space); // restores, no advance
    expect(find.bySemanticsLabel('Black screen'), findsNothing);
    expect(shown(tester), 1);
    await unmount(tester);
    handle.dispose();
  });

  testWidgets('N toggles the speaker notes', (tester) async {
    await host(tester, deck3(), reduce: true);
    expect(find.text('Notes one'), findsNothing);
    await press(tester, LogicalKeyboardKey.keyN);
    expect(find.text('Notes one'), findsOneWidget);
    expect(find.byTooltip('Hide speaker notes'), findsOneWidget);
    await press(tester, LogicalKeyboardKey.space);
    expect(find.text('No notes for this slide'), findsOneWidget);
    await press(tester, LogicalKeyboardKey.keyN);
    expect(find.text('No notes for this slide'), findsNothing);
    expect(find.byTooltip('Show speaker notes'), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('notes also fit a phone-sized screen', (tester) async {
    await host(tester, deck3(), reduce: true, size: const Size(360, 640));
    await press(tester, LogicalKeyboardKey.keyN);
    expect(find.text('Notes one'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await unmount(tester);
  });

  testWidgets('reduce motion: no transition, everything visible',
      (tester) async {
    await host(tester, deck3(), reduce: true);
    expect(view(tester).hidden, isEmpty);
    expect(view(tester).effects, isEmpty);
    await press(tester, LogicalKeyboardKey.space);
    // The click steps are skipped and the slide changes at once.
    expect(find.byType(TransitionView), findsNothing);
    expect(shown(tester), 2);
    expect(view(tester).effects, isEmpty);
    expect(view(tester).hidden, isEmpty);
    await unmount(tester);
  });

  testWidgets('MediaQuery.disableAnimations also reduces motion',
      (tester) async {
    tester.view.physicalSize = const Size(800, 450);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(MaterialApp(
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(context).copyWith(disableAnimations: true),
        child: child!,
      ),
      home: PresentView(deck: deck3()),
    ));
    await tester.pump();
    await press(tester, LogicalKeyboardKey.space);
    expect(shown(tester), 2);
    expect(find.byType(TransitionView), findsNothing);
    await unmount(tester);
  });

  testWidgets('tapping the right half advances, the left half goes back',
      (tester) async {
    await host(tester, deck3(), reduce: true);
    await tester.tapAt(const Offset(700, 200));
    await tester.pump();
    expect(shown(tester), 2);
    await tester.tapAt(const Offset(100, 200));
    await tester.pump();
    expect(shown(tester), 1);
    await unmount(tester);
  });

  testWidgets('swiping towards the start goes forward', (tester) async {
    await host(tester, deck3(), reduce: true);
    await tester.fling(find.byType(PresentView), const Offset(-300, 0), 1500);
    await tester.pump();
    expect(shown(tester), 2);
    await tester.fling(find.byType(PresentView), const Offset(300, 0), 1500);
    await tester.pump();
    expect(shown(tester), 1);
    await unmount(tester);
  });

  testWidgets('right-to-left mirrors arrows and tap halves', (tester) async {
    await host(tester, deck3(), reduce: true, dir: TextDirection.rtl);
    await press(tester, LogicalKeyboardKey.arrowLeft);
    expect(shown(tester), 2);
    await press(tester, LogicalKeyboardKey.arrowRight);
    expect(shown(tester), 1);
    await tester.tapAt(const Offset(100, 200)); // end half in RTL
    await tester.pump();
    expect(shown(tester), 2);
    await unmount(tester);
  });

  testWidgets('after_ms auto-advances', (tester) async {
    await host(tester, deck3(afterMs: 1000));
    // Slide 1 has two click steps; the timer plays them one by one, then
    // moves on (fade 100 ms, then slide 2's own motion).
    Future<void> fire(int ms) async {
      await tester.pump(Duration(milliseconds: ms));
      await tester.pump(); // first frame of whatever the timer started
    }

    await fire(1000); // click 1 starts
    expect(view(tester).hidden, {2});
    await fire(310); // ...and finishes
    expect(view(tester).hidden, {2});
    await fire(1000); // click 2 starts
    await fire(310);
    expect(shown(tester), 1);
    expect(view(tester).hidden, isEmpty);
    await fire(1000); // transition starts
    expect(find.byType(TransitionView), findsOneWidget);
    await fire(500);
    expect(shown(tester), 2);
    await unmount(tester);
  });

  testWidgets('announces "Slide N of M" through a live region', (tester) async {
    final handle = tester.ensureSemantics();
    await host(tester, deck3(), reduce: true);
    expect(
      tester.getSemantics(find.bySemanticsLabel('Slide 1 of 3')),
      matchesSemantics(label: 'Slide 1 of 3', isLiveRegion: true),
    );
    await press(tester, LogicalKeyboardKey.space);
    expect(find.bySemanticsLabel('Slide 2 of 3'), findsOneWidget);
    await unmount(tester);
    handle.dispose();
  });

  testWidgets('controls are labelled, focusable buttons', (tester) async {
    await host(tester, deck3(), reduce: true);
    for (final tip in [
      'Exit presentation',
      'Previous',
      'Next',
      'Show speaker notes',
    ]) {
      expect(find.byTooltip(tip), findsOneWidget, reason: tip);
    }
    await tester.tap(find.byTooltip('Next'));
    await tester.pump();
    expect(shown(tester), 2);
    await tester.tap(find.byTooltip('Previous'));
    await tester.pump();
    expect(shown(tester), 1);
    await unmount(tester);
  });

  testWidgets('custom strings are used', (tester) async {
    await host(
      tester,
      deck3(),
      reduce: true,
      strings: PresentStrings(
        slideOf: (i, n) => 'Folie $i von $n',
        exit: 'Beenden',
        next: 'Weiter',
      ),
    );
    expect(find.text('Folie 1 von 3'), findsOneWidget);
    expect(find.byTooltip('Beenden'), findsOneWidget);
    expect(find.byTooltip('Weiter'), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('an empty deck shows no slide and does not throw',
      (tester) async {
    await host(tester, const DeckScene(width: 16, height: 9, slides: []));
    await press(tester, LogicalKeyboardKey.space);
    await press(tester, LogicalKeyboardKey.arrowLeft);
    expect(find.byType(SlideView), findsNothing);
    expect(tester.takeException(), isNull);
    await unmount(tester);
  });

  testWidgets('phone width: controls fit', (tester) async {
    await host(tester, deck3(), size: const Size(360, 640));
    expect(tester.takeException(), isNull);
    await unmount(tester);
  });
}
