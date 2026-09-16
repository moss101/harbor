import 'package:flutter/material.dart';

import 'foundation.dart';
import 'status.dart';
import 'tokens.dart';

/// Model Dock: the active model / provider / execution surface.
///
/// Full variant: a card with the execution badge, model id and a detail
/// line. Chip variant ([ModelDock.chip]) fits a header action row.
class ModelDock extends StatelessWidget {
  const ModelDock({
    super.key,
    required this.modelLabel,
    required this.runtimeLabel,
    required this.semantic,
    this.onTap,
    this.detail,
    this.heading,
    this.trailing,
    this.modelIsIdentifier = true,
  }) : _chip = false;

  const ModelDock.chip({
    super.key,
    required this.modelLabel,
    required this.runtimeLabel,
    required this.semantic,
    this.onTap,
    this.modelIsIdentifier = true,
  })  : detail = null,
        heading = null,
        trailing = null,
        _chip = true;

  final String modelLabel;
  final String runtimeLabel;
  final ExecutionSemantic semantic;
  final VoidCallback? onTap;
  final String? detail;
  final String? heading;
  final Widget? trailing;
  final bool modelIsIdentifier;
  final bool _chip;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final v = semanticVisual(t.colors, semantic);
    if (_chip) {
      final label = Text(modelLabel,
          overflow: TextOverflow.ellipsis,
          maxLines: 1,
          style: t.text.labelOf(t.colors.ink));
      return Semantics(
        button: onTap != null,
        label: '$runtimeLabel · $modelLabel',
        child: ExcludeSemantics(
          child: Tooltip(
            message: '$runtimeLabel · $modelLabel',
            child: Material(
              color: t.colors.surface,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(HarborRadius.pill),
                side: BorderSide(color: t.colors.border),
              ),
              clipBehavior: Clip.antiAlias,
              child: InkWell(
                onTap: onTap,
                child: ConstrainedBox(
                  constraints:
                      const BoxConstraints(minHeight: 32, maxWidth: 260),
                  child: Padding(
                    padding: const EdgeInsetsDirectional.fromSTEB(
                        HarborSpace.s2,
                        HarborSpace.s1,
                        HarborSpace.s3,
                        HarborSpace.s1),
                    child: Row(mainAxisSize: MainAxisSize.min, children: [
                      Icon(v.icon, size: 14, color: v.text),
                      const SizedBox(width: HarborSpace.s1 + 2),
                      Flexible(
                        child: modelIsIdentifier
                            ? Directionality(
                                textDirection: TextDirection.ltr, child: label)
                            : label,
                      ),
                      if (onTap != null) ...[
                        const SizedBox(width: HarborSpace.s1),
                        Icon(Icons.expand_more,
                            size: 16, color: t.colors.inkMuted),
                      ],
                    ]),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
    }
    final modelText = Text(modelLabel,
        overflow: TextOverflow.ellipsis,
        maxLines: 2,
        style: t.text.bodyStrongOf(t.colors.ink));
    return HarborCard(
      onTap: onTap,
      padding: const EdgeInsets.all(HarborSpace.s4),
      semanticLabel: '${heading ?? ''} $runtimeLabel $modelLabel'.trim(),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          Container(
            width: 40,
            height: 40,
            decoration: BoxDecoration(
              color: v.fill,
              borderRadius: BorderRadius.circular(HarborRadius.sm),
              border: Border.all(color: v.border),
            ),
            child: Icon(Icons.memory_outlined, size: 20, color: v.text),
          ),
          const SizedBox(width: HarborSpace.s3),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                if (heading != null)
                  Text(heading!, style: t.text.captionOf(t.colors.inkMuted)),
                if (modelIsIdentifier)
                  Directionality(
                      textDirection: TextDirection.ltr, child: modelText)
                else
                  modelText,
                const SizedBox(height: HarborSpace.s1 + 2),
                Wrap(
                  spacing: HarborSpace.s2,
                  runSpacing: HarborSpace.s1,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: [
                    StatusBadge(semantic: semantic, label: runtimeLabel),
                    if (detail != null)
                      Text(detail!, style: t.text.captionOf(t.colors.inkMuted)),
                  ],
                ),
              ],
            ),
          ),
          if (trailing != null) ...[
            const SizedBox(width: HarborSpace.s2),
            trailing!,
          ] else if (onTap != null)
            Icon(Icons.chevron_right, color: t.colors.inkMuted),
        ],
      ),
    );
  }
}
