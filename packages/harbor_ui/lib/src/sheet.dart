import 'package:flutter/material.dart';

import 'tokens.dart';

/// Harbor Sheet: the approval/review surface (goal §16). Explains target,
/// scope, base version and consequence before a protected action. The
/// approve/deny actions stay reachable at 320px and 200% text (§4).
class HarborSheet extends StatelessWidget {
  const HarborSheet({
    super.key,
    required this.title,
    required this.explanation,
    required this.diffSummary,
    required this.onApprove,
    required this.onDeny,
    this.approveLabel = 'Approve',
    this.denyLabel = 'Deny',
    this.icon = Icons.verified_user_outlined,
  });

  final String title;
  final String explanation;
  final Widget diffSummary;
  final VoidCallback onApprove;
  final VoidCallback onDeny;
  final String approveLabel;
  final String denyLabel;
  final IconData icon;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Semantics(
      container: true,
      child: Container(
        padding: const EdgeInsets.all(HarborSpace.s5),
        decoration: ShapeDecoration(
          color: t.colors.surface,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(HarborRadius.lg),
            side: BorderSide(color: t.colors.border),
          ),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Container(
                width: 36,
                height: 36,
                decoration: BoxDecoration(
                  color: t.colors.brandSoft,
                  borderRadius: BorderRadius.circular(HarborRadius.sm),
                ),
                child: Icon(icon, size: 20, color: t.colors.brand),
              ),
              const SizedBox(width: HarborSpace.s3),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(title, style: t.text.h2Of(t.colors.ink)),
                    const SizedBox(height: HarborSpace.s1),
                    Text(explanation, style: t.text.smallOf(t.colors.inkMuted)),
                  ],
                ),
              ),
            ]),
            const SizedBox(height: HarborSpace.s4),
            diffSummary,
            const SizedBox(height: HarborSpace.s5),
            Wrap(
              alignment: WrapAlignment.end,
              spacing: HarborSpace.s2,
              runSpacing: HarborSpace.s2,
              children: [
                OutlinedButton(onPressed: onDeny, child: Text(denyLabel)),
                FilledButton(onPressed: onApprove, child: Text(approveLabel)),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
