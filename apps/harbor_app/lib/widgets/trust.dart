import 'package:flutter/material.dart';
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../services/harbor_service.dart';

/// Display label for the workspace policy token ("LOCAL_ONLY" →
/// "LOCAL ONLY"). Tokens are technical; the surrounding copy is localized.
String policyDisplay(HarborService? service, AppLocalizations l10n) {
  final code = service?.policyCode;
  if (code == null) return l10n.trustPolicy;
  return code.replaceAll('_', ' ');
}

String executionDisplay(HarborService? service, AppLocalizations l10n) {
  final hint = service?.executionHint;
  if (hint == null) return l10n.trustExecutionOnDevice;
  return hint.replaceAll('_', ' ');
}

/// Runtime label for the Model Dock badge (policy-derived, technical).
String runtimeLabelFor(HarborService? service) {
  final code = service?.policyCode ?? 'LOCAL_ONLY';
  return code.contains('LOCAL_ONLY') ? 'LOCAL ONLY' : 'LOCAL';
}

/// Trust Pulse bound to the live service (policy + execution facts).
class TrustPulseCard extends StatelessWidget {
  const TrustPulseCard({super.key, required this.service, this.failed = false});
  final HarborService? service;
  final bool failed;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final version = service?.policyVersion;
    return TrustPulse(
      policyHeading: l10n.trustPolicyHeading,
      executionHeading: l10n.trustExecutionHeading,
      policyLabel: policyDisplay(service, l10n),
      policyDetail: version == null
          ? l10n.settingsPrivacyBody
          : l10n.trustPolicyVersion(version),
      executionLabel: failed ? 'OFFLINE' : executionDisplay(service, l10n),
      executionSemantic:
          failed ? ExecutionSemantic.danger : ExecutionSemantic.local,
    );
  }
}

/// Compact Trust chip for headers and rail footers.
class TrustChipLive extends StatelessWidget {
  const TrustChipLive({
    super.key,
    required this.onTap,
    this.iconOnly = false,
    this.failed = false,
  });
  final VoidCallback onTap;
  final bool iconOnly;
  final bool failed;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return TrustChip(
      label: failed ? 'OFFLINE' : l10n.trustChipLabel,
      semantic: failed ? ExecutionSemantic.danger : ExecutionSemantic.local,
      tooltip: failed ? l10n.coreDegradedTitle : l10n.trustChipTooltip,
      iconOnly: iconOnly,
      onTap: onTap,
    );
  }
}
