// Full-screen presenter (owner: Presenter agent).
//
// [PresentView] plays a [DeckScene]: one action ("next") first plays the
// remaining click steps of the current slide, then transitions to the next
// slide, which then auto-plays its step-0 (non-click) animations. See
// slide_player.dart for the step model.
//
// Controls
//  * keys: Right / Down / Space / Enter / PageDown next; Left / Up /
//    Backspace / PageUp previous (Left and Right swap in right-to-left
//    layouts, as do the tap halves and the swipe); Home / End first / last
//    slide; Esc exits; B black screen; W white screen; N speaker notes.
//  * tap the end half = next, the start half = previous; swipe likewise.
//  * a next/previous pressed while a step or transition is still running
//    completes that motion (and does nothing else); "previous" jumps to the
//    prior state instantly, with no reverse animation.
//  * `transition.after_ms` on the *current* slide auto-advances (the next
//    click step, or the next slide) that long after the slide has settled.
//    `transition.on_click == false` is not enforced: manual navigation always
//    works so a presenter can never be stuck.
//  * reduce motion (the `reduceMotion` argument, else
//    `MediaQuery.disableAnimations`): no transitions and no animations; every
//    slide appears at once with everything visible.
import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'slide_player.dart';
import 'slide_scene.dart';
import 'slide_transitions.dart';
import 'slide_view.dart';

String _defaultSlideOf(int index, int count) => 'Slide $index of $count';
String _defaultElapsed(String time) => 'Elapsed $time';

/// Localized strings for [PresentView]. harbor_ui has no l10n of its own, so
/// the apps pass their translations; the defaults are English.
class PresentStrings {
  const PresentStrings({
    this.slideOf = _defaultSlideOf,
    this.exit = 'Exit presentation',
    this.next = 'Next',
    this.previous = 'Previous',
    this.showNotes = 'Show speaker notes',
    this.hideNotes = 'Hide speaker notes',
    this.noNotes = 'No notes for this slide',
    this.elapsed = _defaultElapsed,
    this.blackScreen = 'Black screen',
    this.whiteScreen = 'White screen',
    this.progress = 'Presentation progress',
  });

  /// "Slide {index} of {count}" (1-based index).
  final String Function(int index, int count) slideOf;
  final String exit, next, previous;
  final String showNotes, hideNotes, noNotes;

  /// "Elapsed {time}"; [time] is `m:ss` or `h:mm:ss`.
  final String Function(String time) elapsed;
  final String blackScreen, whiteScreen, progress;
}

enum _Motion { none, step, transition }

const int _done = 1 << 30;

class PresentView extends StatefulWidget {
  const PresentView({
    super.key,
    required this.deck,
    this.startIndex = 0,
    this.reduceMotion,
    this.strings = const PresentStrings(),
    this.showNotes = false,
    this.onExit,
  });

  final DeckScene deck;

  /// 0-based slide to start on.
  final int startIndex;

  /// Overrides `MediaQuery.disableAnimations` when non-null.
  final bool? reduceMotion;
  final PresentStrings strings;

  /// Whether the speaker-notes panel starts open.
  final bool showNotes;

  /// Called on Esc / the exit button. Defaults to popping the route.
  final VoidCallback? onExit;

  /// Pushes a full-screen [PresentView] route.
  static Future<void> show(
    BuildContext context, {
    required DeckScene deck,
    int startIndex = 0,
    bool? reduceMotion,
    PresentStrings strings = const PresentStrings(),
  }) {
    return Navigator.of(context, rootNavigator: true).push<void>(
      PageRouteBuilder<void>(
        opaque: true,
        fullscreenDialog: true,
        transitionDuration: const Duration(milliseconds: 150),
        reverseTransitionDuration: const Duration(milliseconds: 150),
        pageBuilder: (_, __, ___) => PresentView(
          deck: deck,
          startIndex: startIndex,
          reduceMotion: reduceMotion,
          strings: strings,
        ),
        transitionsBuilder: (_, a, __, child) =>
            FadeTransition(opacity: a, child: child),
      ),
    );
  }

  @override
  State<PresentView> createState() => _PresentViewState();
}

class _PresentViewState extends State<PresentView>
    with SingleTickerProviderStateMixin {
  late final AnimationController _ctl;
  final FocusNode _focus = FocusNode(debugLabel: 'PresentView');
  final Map<int, SlidePlayer> _players = {};

  late int _index;
  int _step = 0; // step shown on the current slide
  int _t = _done; // ms into [_step], or _done
  _Motion _motion = _Motion.none;
  int _stepDur = 1;

  // Transition snapshot.
  int _transFrom = 0, _transTo = 0;
  PlayState _transFromState = PlayState.empty;
  PlayState _transToState = PlayState.empty;

  bool _black = false, _white = false, _notes = false;
  bool _controls = true;
  int _seconds = 0;
  Timer? _clock, _auto, _hide;
  bool _started = false;

  DeckScene get _deck => widget.deck;
  int get _last => _deck.slides.length - 1;

  bool get _mobile =>
      !kIsWeb &&
      (defaultTargetPlatform == TargetPlatform.android ||
          defaultTargetPlatform == TargetPlatform.iOS);

  bool get _reduce =>
      widget.reduceMotion ?? MediaQuery.disableAnimationsOf(context);

  SlidePlayer _player(int i) => _players[i] ??= SlidePlayer(
        _deck.slides[i],
        deckWidth: _deck.width,
        deckHeight: _deck.height,
      );

  int get _steps => _deck.slides.isEmpty || _reduce ? 0 : _player(_index).steps;

  @override
  void initState() {
    super.initState();
    _index = _deck.slides.isEmpty
        ? 0
        : widget.startIndex.clamp(0, _deck.slides.length - 1);
    _notes = widget.showNotes;
    _ctl =
        AnimationController(vsync: this, duration: const Duration(seconds: 1))
          ..addListener(() {
            if (mounted && _motion != _Motion.none) setState(() {});
          })
          ..addStatusListener((s) {
            if (s == AnimationStatus.completed) _onMotionDone();
          });
    _clock = Timer.periodic(const Duration(seconds: 1), (_) {
      if (mounted) setState(() => _seconds++);
    });
    if (_mobile) {
      unawaited(
          SystemChrome.setEnabledSystemUIMode(SystemUiMode.immersiveSticky)
              .catchError((Object _) {}));
    }
    _pokeControls(rebuild: false);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (!_started) {
      _started = true;
      if (_deck.slides.isNotEmpty) _enterSlide(_index, rebuild: false);
    } else if (_reduce && _motion != _Motion.none) {
      _finishMotion();
    }
  }

  @override
  void dispose() {
    _clock?.cancel();
    _auto?.cancel();
    _hide?.cancel();
    _ctl.dispose();
    _focus.dispose();
    if (_mobile) {
      unawaited(SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge)
          .catchError((Object _) {}));
    }
    super.dispose();
  }

  // -- timeline ------------------------------------------------------------

  void _enterSlide(int i, {bool rebuild = true}) {
    _ctl.stop();
    _auto?.cancel();
    _index = i;
    _motion = _Motion.none;
    _step = 0;
    _t = _done;
    final p = _player(i);
    if (!_reduce && p.hasAutoPlay) {
      _playStep(0);
    } else {
      _settled();
    }
    if (rebuild && mounted) setState(() {});
  }

  void _playStep(int n) {
    _step = n;
    final dur = _player(_index).durationMs(n);
    if (dur <= 0) {
      _t = _done;
      _motion = _Motion.none;
      _settled();
      return;
    }
    _t = 0;
    _stepDur = dur;
    _motion = _Motion.step;
    _ctl.duration = Duration(milliseconds: dur);
    _ctl.forward(from: 0);
  }

  void _startTransition(int to) {
    final tr = _deck.slides[to].transition;
    if (_reduce || tr == null) {
      _enterSlide(to);
      return;
    }
    _transFrom = _index;
    _transTo = to;
    _transFromState =
        _reduce ? PlayState.empty : _player(_index).stateAt(_step, _currentT());
    _transToState = _player(to).stateAt(-1, 0);
    _motion = _Motion.transition;
    _ctl.duration = Duration(milliseconds: tr.durMs < 1 ? 1 : tr.durMs);
    _ctl.forward(from: 0);
    setState(() {});
  }

  int _currentT() => _motion == _Motion.step
      ? (_ctl.value * _stepDur).round().clamp(0, _stepDur)
      : _t;

  void _onMotionDone() {
    switch (_motion) {
      case _Motion.step:
        _t = _done;
        _motion = _Motion.none;
        _settled();
        if (mounted) setState(() {});
      case _Motion.transition:
        _enterSlide(_transTo);
      case _Motion.none:
        break;
    }
  }

  void _finishMotion() {
    switch (_motion) {
      case _Motion.step:
        _ctl.stop();
        _t = _done;
        _motion = _Motion.none;
        _settled();
      case _Motion.transition:
        _ctl.stop();
        _enterSlide(_transTo);
      case _Motion.none:
        break;
    }
    if (mounted) setState(() {});
  }

  /// A slide (and its motion) has settled: arm the auto-advance, if any.
  void _settled() {
    _auto?.cancel();
    final ms =
        _deck.slides.isEmpty ? null : _deck.slides[_index].transition?.afterMs;
    final atEnd = _index >= _last && _step >= _steps;
    if (ms == null || atEnd) return;
    _auto = Timer(Duration(milliseconds: ms), () {
      if (mounted && _motion == _Motion.none) _next();
    });
  }

  // -- navigation ----------------------------------------------------------

  void _unblank() {
    _black = false;
    _white = false;
  }

  void _next() {
    if (_deck.slides.isEmpty) return;
    _auto?.cancel();
    if (_black || _white) {
      setState(_unblank);
      return;
    }
    if (_motion != _Motion.none) {
      _finishMotion();
      return;
    }
    if (_step < _steps) {
      _playStep(_step + 1);
      if (mounted) setState(() {});
    } else if (_index < _last) {
      _startTransition(_index + 1);
    }
  }

  void _previous() {
    if (_deck.slides.isEmpty) return;
    _auto?.cancel();
    if (_black || _white) {
      setState(_unblank);
      return;
    }
    if (_motion != _Motion.none) {
      // Abandon the motion where it is: a transition stays on its outgoing
      // slide, a step lands on its end state.
      _ctl.stop();
      _t = _done;
      _motion = _Motion.none;
    }
    if (_step > 0) {
      _step -= 1;
      _t = _done;
    } else if (_index > 0) {
      _index -= 1;
      _step = _steps;
      _t = _done;
    }
    _settled();
    setState(() {});
  }

  void _goTo(int i) {
    if (_deck.slides.isEmpty) return;
    _unblank();
    _enterSlide(i.clamp(0, _last));
  }

  void _exit() {
    final cb = widget.onExit;
    if (cb != null) {
      cb();
    } else {
      Navigator.of(context).maybePop();
    }
  }

  // -- chrome --------------------------------------------------------------

  void _pokeControls({bool rebuild = true}) {
    _hide?.cancel();
    _hide = Timer(const Duration(seconds: 3), () {
      if (mounted && _controls) setState(() => _controls = false);
    });
    if (!_controls) {
      _controls = true;
      if (rebuild && mounted) setState(() {});
    }
  }

  KeyEventResult _onKey(FocusNode node, KeyEvent e) {
    if (e is! KeyDownEvent) return KeyEventResult.ignored;
    _pokeControls();
    final rtl = Directionality.of(context) == TextDirection.rtl;
    final k = e.logicalKey;
    if (k == LogicalKeyboardKey.escape) {
      _exit();
    } else if (k ==
            (rtl
                ? LogicalKeyboardKey.arrowLeft
                : LogicalKeyboardKey.arrowRight) ||
        k == LogicalKeyboardKey.arrowDown ||
        k == LogicalKeyboardKey.space ||
        k == LogicalKeyboardKey.enter ||
        k == LogicalKeyboardKey.pageDown) {
      _next();
    } else if (k ==
            (rtl
                ? LogicalKeyboardKey.arrowRight
                : LogicalKeyboardKey.arrowLeft) ||
        k == LogicalKeyboardKey.arrowUp ||
        k == LogicalKeyboardKey.backspace ||
        k == LogicalKeyboardKey.pageUp) {
      _previous();
    } else if (k == LogicalKeyboardKey.home) {
      _goTo(0);
    } else if (k == LogicalKeyboardKey.end) {
      _goTo(_last);
    } else if (k == LogicalKeyboardKey.keyB) {
      setState(() {
        _white = false;
        _black = !_black;
      });
    } else if (k == LogicalKeyboardKey.keyW) {
      setState(() {
        _black = false;
        _white = !_white;
      });
    } else if (k == LogicalKeyboardKey.keyN) {
      setState(() => _notes = !_notes);
    } else {
      return KeyEventResult.ignored;
    }
    return KeyEventResult.handled;
  }

  void _tap(TapUpDetails d, double width, bool rtl) {
    _pokeControls();
    final endHalf =
        rtl ? d.localPosition.dx < width / 2 : d.localPosition.dx >= width / 2;
    if (endHalf) {
      _next();
    } else {
      _previous();
    }
  }

  void _swipe(DragEndDetails d, bool rtl) {
    final v = d.primaryVelocity ?? 0;
    if (v.abs() < 300) return;
    // Swiping towards the start edge goes forward.
    final towardsStart = rtl ? v > 0 : v < 0;
    if (towardsStart) {
      _next();
    } else {
      _previous();
    }
  }

  void _act(VoidCallback f) {
    f();
    _focus.requestFocus();
  }

  static String _fmt(int s) {
    final h = s ~/ 3600;
    final m = (s % 3600) ~/ 60;
    final ss = (s % 60).toString().padLeft(2, '0');
    return h > 0 ? '$h:${m.toString().padLeft(2, '0')}:$ss' : '$m:$ss';
  }

  // -- build ---------------------------------------------------------------

  Widget _slideArea(BuildContext context) {
    if (_deck.slides.isEmpty) return const SizedBox.shrink();
    final ar = _deck.width / _deck.height;
    return LayoutBuilder(builder: (context, c) {
      var w = c.maxWidth;
      var h = w / ar;
      if (h > c.maxHeight) {
        h = c.maxHeight;
        w = h * ar;
      }
      final Widget content;
      if (_motion == _Motion.transition) {
        content = TransitionView(
          from: _deck.slides[_transFrom],
          to: _deck.slides[_transTo],
          deckWidth: _deck.width,
          deckHeight: _deck.height,
          transition: _deck.slides[_transTo].transition,
          progress: Curves.easeInOut.transform(_ctl.value),
          fromState: _transFromState,
          toState: _transToState,
        );
      } else {
        final st = _reduce
            ? PlayState.empty
            : _player(_index).stateAt(_step, _currentT());
        content = SlideView(
          scene: _deck.slides[_index],
          deckWidth: _deck.width,
          deckHeight: _deck.height,
          effects: st.effects,
          hidden: st.hidden,
          hiddenParas: st.hiddenParas,
        );
      }
      return Center(child: SizedBox(width: w, height: h, child: content));
    });
  }

  Widget _notesPanel(BuildContext context) {
    final s = widget.strings;
    final notes = _deck.slides.isEmpty ? null : _deck.slides[_index].notes;
    final has = notes != null && notes.trim().isNotEmpty;
    return Container(
      color: const Color(0xFF16181D),
      padding: const EdgeInsets.all(16),
      child: SingleChildScrollView(
        child: Text(
          has ? notes : s.noNotes,
          style: TextStyle(
            color: has ? const Color(0xFFF2F4F8) : const Color(0xFF9AA3B2),
            fontSize: 18,
            height: 1.4,
          ),
        ),
      ),
    );
  }

  Widget _controlButton({
    required IconData icon,
    required String tooltip,
    required VoidCallback? onPressed,
  }) {
    return IconButton(
      icon: Icon(icon),
      tooltip: tooltip,
      color: Colors.white,
      disabledColor: Colors.white30,
      style: IconButton.styleFrom(
        backgroundColor: const Color(0x66000000),
        minimumSize: const Size(48, 48),
      ),
      onPressed: onPressed == null ? null : () => _act(onPressed),
    );
  }

  Widget _controlsOverlay(BuildContext context) {
    final s = widget.strings;
    final n = _deck.slides.length;
    final label = s.slideOf(n == 0 ? 0 : _index + 1, n);
    const textStyle = TextStyle(color: Colors.white, fontSize: 13);
    final visible = _controls || MediaQuery.accessibleNavigationOf(context);
    return Positioned.fill(
      child: SafeArea(
        child: Focus(
          canRequestFocus: false,
          skipTraversal: true,
          onFocusChange: (f) {
            if (f) _pokeControls();
          },
          child: IgnorePointer(
            ignoring: !visible,
            child: AnimatedOpacity(
              opacity: visible ? 1 : 0,
              alwaysIncludeSemantics: true,
              duration:
                  _reduce ? Duration.zero : const Duration(milliseconds: 200),
              child: Padding(
                padding: const EdgeInsets.all(8),
                child: Column(
                  children: [
                    Row(children: [
                      _controlButton(
                        icon: Icons.close,
                        tooltip: s.exit,
                        onPressed: _exit,
                      ),
                      const Spacer(),
                      _controlButton(
                        icon: _notes
                            ? Icons.speaker_notes_off_outlined
                            : Icons.speaker_notes_outlined,
                        tooltip: _notes ? s.hideNotes : s.showNotes,
                        onPressed: () => setState(() => _notes = !_notes),
                      ),
                    ]),
                    const Spacer(),
                    Row(children: [
                      _controlButton(
                        icon: Icons.chevron_left,
                        tooltip: s.previous,
                        onPressed: _previous,
                      ),
                      Expanded(
                        child: Center(
                          child: FittedBox(
                            fit: BoxFit.scaleDown,
                            child: Container(
                              padding: const EdgeInsets.symmetric(
                                  horizontal: 12, vertical: 6),
                              decoration: BoxDecoration(
                                color: const Color(0x66000000),
                                borderRadius: BorderRadius.circular(16),
                              ),
                              child: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  // The live region announces only the slide
                                  // change, never the ticking timer.
                                  Semantics(
                                    container: true,
                                    liveRegion: true,
                                    label: label,
                                    excludeSemantics: true,
                                    child: Text(label, style: textStyle),
                                  ),
                                  Semantics(
                                    label: s.elapsed(_fmt(_seconds)),
                                    excludeSemantics: true,
                                    child: Text(' · ${_fmt(_seconds)}',
                                        style: textStyle),
                                  ),
                                ],
                              ),
                            ),
                          ),
                        ),
                      ),
                      _controlButton(
                        icon: Icons.chevron_right,
                        tooltip: s.next,
                        onPressed: _next,
                      ),
                    ]),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.strings;
    final rtl = Directionality.of(context) == TextDirection.rtl;
    final n = _deck.slides.length;
    final progress = n == 0 ? 0.0 : (_index + 1) / n;

    final stage = LayoutBuilder(builder: (context, c) {
      final slide = _slideArea(context);
      if (!_notes) return slide;
      if (c.maxWidth >= 700) {
        return Row(children: [
          Expanded(flex: 3, child: slide),
          SizedBox(width: 320, child: _notesPanel(context)),
        ]);
      }
      return Column(children: [
        Expanded(child: slide),
        SizedBox(height: 150, child: _notesPanel(context)),
      ]);
    });

    return Material(
      color: Colors.black,
      child: Focus(
        focusNode: _focus,
        autofocus: true,
        onKeyEvent: _onKey,
        child: LayoutBuilder(builder: (context, c) {
          return MouseRegion(
            onHover: (_) => _pokeControls(),
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTapUp: (d) => _tap(d, c.maxWidth, rtl),
              onHorizontalDragEnd: (d) => _swipe(d, rtl),
              child: Stack(
                fit: StackFit.expand,
                children: [
                  Positioned.fill(child: stage),
                  Positioned(
                    left: 0,
                    right: 0,
                    bottom: 0,
                    child: Semantics(
                      label: s.progress,
                      value: s.slideOf(n == 0 ? 0 : _index + 1, n),
                      child: ExcludeSemantics(
                        child: LinearProgressIndicator(
                          value: progress,
                          minHeight: 3,
                          backgroundColor: const Color(0x33FFFFFF),
                          color: Colors.white70,
                        ),
                      ),
                    ),
                  ),
                  _controlsOverlay(context),
                  if (_black || _white)
                    Positioned.fill(
                      child: Semantics(
                        container: true,
                        label: _black ? s.blackScreen : s.whiteScreen,
                        child: ColoredBox(
                          color: _black ? Colors.black : Colors.white,
                        ),
                      ),
                    ),
                ],
              ),
            ),
          );
        }),
      ),
    );
  }
}
