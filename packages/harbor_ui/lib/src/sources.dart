import 'package:flutter/material.dart';

import 'tokens.dart';

/// A compact source chip under an answer: a small number badge and a
/// short title ("1 · report.docx", "2 · Page 3"). The full source list
/// (quotes, scores) opens from the row, so the answer stays readable.
class HarborSourceChip extends StatelessWidget {
  const HarborSourceChip({
    super.key,
    required this.index,
    required this.label,
    this.onTap,
    this.semanticLabel,
  });

  /// 1-based position, matching the answer's [n] markers.
  final int index;

  /// Source title or locator (data, shown as is).
  final String label;
  final VoidCallback? onTap;

  /// What a screen reader announces for the tap; defaults to [label].
  final String? semanticLabel;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final c = t.colors;
    final chip = Container(
      constraints: const BoxConstraints(maxWidth: 220, minHeight: 32),
      padding: const EdgeInsetsDirectional.fromSTEB(
          HarborSpace.s1, HarborSpace.s1, HarborSpace.s3, HarborSpace.s1),
      decoration: ShapeDecoration(
        color: c.surface,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(HarborRadius.pill),
          side: BorderSide(color: c.border),
        ),
      ),
      child: Row(mainAxisSize: MainAxisSize.min, children: [
        Container(
          width: 22,
          height: 22,
          alignment: Alignment.center,
          decoration: BoxDecoration(color: c.brandSoft, shape: BoxShape.circle),
          child: Text('$index',
              style: t.text.captionOf(c.ink).copyWith(
                  fontWeight: FontWeight.w600, fontSize: 11, height: 1)),
        ),
        const SizedBox(width: HarborSpace.s2),
        Flexible(
          child: Text(label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: t.text.smallOf(c.ink)),
        ),
      ]),
    );
    if (onTap == null) return chip;
    return Semantics(
      button: true,
      label: semanticLabel ?? label,
      child: ExcludeSemantics(
        child: Material(
          color: Colors.transparent,
          shape: const StadiumBorder(),
          child: InkWell(
            customBorder: const StadiumBorder(),
            hoverColor: c.hover,
            onTap: onTap,
            child: chip,
          ),
        ),
      ),
    );
  }
}

/// The small heading over an answer's parts ("Sources", "Answer").
class HarborAnswerSectionLabel extends StatelessWidget {
  const HarborAnswerSectionLabel(
      {super.key, required this.icon, required this.label, this.trailing});
  final IconData icon;
  final String label;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Row(children: [
      Icon(icon, size: 18, color: t.colors.ink),
      const SizedBox(width: HarborSpace.s2),
      Expanded(
        child: Text(label,
            style: t.text.bodyStrongOf(t.colors.ink).copyWith(fontSize: 15)),
      ),
      if (trailing != null) trailing!,
    ]);
  }
}
