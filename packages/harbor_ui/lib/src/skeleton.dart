import 'package:flutter/material.dart';

import 'tokens.dart';

/// A loading placeholder shaped like the content it stands for (lines of
/// text, a row of chips), instead of a spinner. One soft sheen sweeps
/// across every block together; with reduced motion the blocks hold still.
/// Decorative: screen readers hear the status line next to it, not this.
class HarborSkeleton extends StatefulWidget {
  const HarborSkeleton({super.key, required this.child});

  /// Built from [HarborSkeletonBlock]s (any layout).
  final Widget child;

  /// Text lines: full-width lines with a shorter last one.
  static Widget lines(int count,
          {double height = 12, double gap = HarborSpace.s3, Key? key}) =>
      HarborSkeleton(
        key: key,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            for (var i = 0; i < count; i++) ...[
              if (i > 0) SizedBox(height: gap),
              FractionallySizedBox(
                widthFactor:
                    i == count - 1 && count > 1 ? 0.62 : (i.isOdd ? 0.94 : 1.0),
                child: HarborSkeletonBlock(height: height),
              ),
            ],
          ],
        ),
      );

  @override
  State<HarborSkeleton> createState() => _HarborSkeletonState();
}

class _HarborSkeletonState extends State<HarborSkeleton>
    with SingleTickerProviderStateMixin {
  late final AnimationController _sheen = AnimationController(
      vsync: this, duration: const Duration(milliseconds: 1400));

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (HarborMotion.of(context).reduced) {
      _sheen.stop();
      _sheen.value = 0;
    } else if (!_sheen.isAnimating) {
      _sheen.repeat();
    }
  }

  @override
  void dispose() {
    _sheen.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final base = t.colors.skeleton;
    final glow = t.colors.ink.withValues(alpha: t.isDark ? 0.18 : 0.12);
    final rtl = Directionality.of(context) == TextDirection.rtl;
    return ExcludeSemantics(
      child: AnimatedBuilder(
        animation: _sheen,
        builder: (context, child) {
          // The sheen travels with the reading direction.
          final p = rtl ? 1 - _sheen.value : _sheen.value;
          return ShaderMask(
            blendMode: BlendMode.srcATop,
            shaderCallback: (rect) => LinearGradient(
              colors: [base, glow, base],
              stops: const [0.0, 0.5, 1.0],
              begin: Alignment(-1 - 2 + 4 * p, 0),
              end: Alignment(1 - 2 + 4 * p, 0),
            ).createShader(rect),
            child: child,
          );
        },
        child: _SkeletonColor(color: base, child: widget.child),
      ),
    );
  }
}

class _SkeletonColor extends InheritedWidget {
  const _SkeletonColor({required this.color, required super.child});
  final Color color;
  @override
  bool updateShouldNotify(_SkeletonColor old) => old.color != color;
}

/// One placeholder block inside a [HarborSkeleton].
class HarborSkeletonBlock extends StatelessWidget {
  const HarborSkeletonBlock({
    super.key,
    this.width,
    this.height = 12,
    this.radius = HarborRadius.sm,
  });
  final double? width;
  final double height;
  final double radius;

  @override
  Widget build(BuildContext context) {
    final color =
        context.dependOnInheritedWidgetOfExactType<_SkeletonColor>()?.color ??
            HarborTheme.of(context).colors.border;
    return Container(
      width: width,
      height: height,
      decoration: BoxDecoration(
          color: color, borderRadius: BorderRadius.circular(radius)),
    );
  }
}
