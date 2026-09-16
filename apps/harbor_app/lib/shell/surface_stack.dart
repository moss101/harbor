import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

/// Keeps every visited surface alive (Ask conversations, search results
/// and filters survive navigation) and cross-fades between them with the
/// token motion scale. Inactive surfaces are offstage, unfocusable,
/// non-interactive and hidden from assistive technology.
class SurfaceStack extends StatefulWidget {
  const SurfaceStack({
    super.key,
    required this.index,
    required this.count,
    required this.builder,
  });

  final int index;
  final int count;
  final Widget Function(int index) builder;

  @override
  State<SurfaceStack> createState() => _SurfaceStackState();
}

class _SurfaceStackState extends State<SurfaceStack> {
  final Set<int> _visited = {};

  @override
  Widget build(BuildContext context) {
    _visited.add(widget.index);
    return Stack(
      fit: StackFit.expand,
      children: [
        for (var i = 0; i < widget.count; i++)
          if (_visited.contains(i))
            _SurfacePane(
              key: ValueKey(i),
              active: i == widget.index,
              child: widget.builder(i),
            ),
      ],
    );
  }
}

class _SurfacePane extends StatefulWidget {
  const _SurfacePane({super.key, required this.active, required this.child});
  final bool active;
  final Widget child;

  @override
  State<_SurfacePane> createState() => _SurfacePaneState();
}

class _SurfacePaneState extends State<_SurfacePane> {
  late bool _mounted = widget.active;

  @override
  void didUpdateWidget(covariant _SurfacePane oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.active && !_mounted) _mounted = true;
  }

  @override
  Widget build(BuildContext context) {
    final motion = HarborMotion.of(context);
    final active = widget.active;
    // The fade itself must keep ticking while the pane retires, so the
    // TickerMode that pauses the surface's own animations sits inside it.
    return Offstage(
      offstage: !_mounted,
      child: ExcludeFocus(
        excluding: !active,
        child: ExcludeSemantics(
          excluding: !active,
          child: IgnorePointer(
            ignoring: !active,
            child: AnimatedOpacity(
              opacity: active ? 1 : 0,
              duration: motion.normal,
              curve: HarborMotion.easing,
              onEnd: () {
                if (!widget.active && mounted) {
                  setState(() => _mounted = false);
                }
              },
              child: AnimatedSlide(
                offset: active ? Offset.zero : const Offset(0, 0.01),
                duration: motion.normal,
                curve: HarborMotion.easing,
                child: TickerMode(enabled: active, child: widget.child),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
