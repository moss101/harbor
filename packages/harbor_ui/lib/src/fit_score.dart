import 'package:flutter/material.dart';

import 'status.dart';
import 'tokens.dart';

/// Fit Score band rendering with the five user-language bands (goal §6).
enum FitBand { excellent, good, limited, tooLarge, unsupported }

extension FitBandLabel on FitBand {
  String get label => switch (this) {
        FitBand.excellent => 'Excellent',
        FitBand.good => 'Good',
        FitBand.limited => 'Limited',
        FitBand.tooLarge => 'Too large',
        FitBand.unsupported => 'Unsupported',
      };

  ExecutionSemantic get semantic => switch (this) {
        FitBand.excellent || FitBand.good => ExecutionSemantic.local,
        FitBand.limited => ExecutionSemantic.hybrid,
        FitBand.tooLarge || FitBand.unsupported => ExecutionSemantic.danger,
      };

  IconData get icon => switch (this) {
        FitBand.excellent => Icons.speed,
        FitBand.good => Icons.check_circle_outline,
        FitBand.limited => Icons.warning_amber_outlined,
        FitBand.tooLarge => Icons.storage_outlined,
        FitBand.unsupported => Icons.block_outlined,
      };

  static FitBand parse(String band) => switch (band.toLowerCase()) {
        'excellent' => FitBand.excellent,
        'good' => FitBand.good,
        'limited' => FitBand.limited,
        'unsupported' => FitBand.unsupported,
        _ => FitBand.tooLarge,
      };
}

class FitScoreBadge extends StatelessWidget {
  const FitScoreBadge({
    super.key,
    required this.band,
    this.reasons = const [],
    this.label,
    this.showReasons = true,
  });

  final FitBand band;
  final List<String> reasons;

  /// Localized band label (defaults to the English band name).
  final String? label;
  final bool showReasons;

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        StatusBadge(
          semantic: band.semantic,
          icon: band.icon,
          label: label ?? band.label,
          tooltip: reasons.join('\n'),
        ),
        if (showReasons)
          for (final r in reasons)
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Text('• $r', style: t.text.captionOf(t.colors.inkMuted)),
            ),
      ],
    );
  }
}
