import 'package:flutter/material.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';

/// The Store tab of Models: products included with Harbor (the Office
/// suite). Office is INCLUDED — the engine ships compiled in every build,
/// so its card is an activation/status view, never a download (platform
/// policy 2.5.2 and Harbor's own resource policy). Harbor's first-party
/// models are the signed catalog under Recommended; this file also holds
/// the model install / uninstall confirmations every Models tab uses:
/// downloads quote their size through the brokered preflight and require an
/// explicit confirmation before transfer (SEC-029); uninstall previews the
/// exact owned-file scope and keeps an undo window (SEC-024).
class StoreTab extends StatelessWidget {
  const StoreTab({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final state = AppStateScope.maybeOf(context);
    return HarborPage(
      maxWidth: HarborLayout.readingMax + 80,
      children: [
        Text(l10n.storeTagline, style: t.text.captionOf(t.colors.inkMuted)),
        const SizedBox(height: HarborSpace.s3),
        _OfficeCard(
            onOpenWork:
                state == null ? null : () => state.goTo(HarborSurface.work)),
      ],
    );
  }
}

/// The Harbor Office Suite card: the product ships inside this app, so
/// the honest store presentation is an INSTALLED state with the suite's
/// contents listed — never a download (platform policy 2.5.2 / Play
/// forbid downloadable native code, and the engine is compiled into the
/// binary). "Open in Work" is a secondary action; there is deliberately
/// no runtime "enable" toggle (the feature registry is a build-time
/// contract — dormant code keeps no live authority path).
class _OfficeCard extends StatefulWidget {
  const _OfficeCard({required this.onOpenWork});
  final VoidCallback? onOpenWork;

  @override
  State<_OfficeCard> createState() => _OfficeCardState();
}

class _OfficeCardState extends State<_OfficeCard> {
  bool _insideExpanded = false;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    return HarborCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(children: [
            Icon(Icons.description_outlined, color: t.colors.accent),
            const SizedBox(width: HarborSpace.s2),
            Expanded(
              child: Text(l10n.storeOfficeTitle,
                  style: t.text.bodyStrongOf(t.colors.ink)),
            ),
            HarborPill(l10n.storeOfficeInstalled, brand: true),
          ]),
          const SizedBox(height: HarborSpace.s2),
          Text(l10n.storeOfficeShips,
              style: t.text.captionOf(t.colors.inkMuted)),
          const SizedBox(height: HarborSpace.s2),
          Text(l10n.storeOfficeStatus,
              style: t.text.captionOf(t.colors.inkMuted)),
          const SizedBox(height: HarborSpace.s3),
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: TextButton.icon(
              onPressed: () =>
                  setState(() => _insideExpanded = !_insideExpanded),
              icon: Icon(
                  _insideExpanded ? Icons.expand_less : Icons.expand_more,
                  size: 18),
              label: Text(l10n.storeOfficeInside),
            ),
          ),
          if (_insideExpanded)
            Padding(
              padding: const EdgeInsetsDirectional.only(bottom: HarborSpace.s2),
              child: Column(children: [
                for (final (name, live) in [
                  (l10n.storeSuiteSheets, true),
                  (l10n.storeSuiteConvert, true),
                  (l10n.storeSuiteFormulas, true),
                  (l10n.storeSuiteDocs, false),
                  (l10n.storeSuiteSlides, false),
                  (l10n.storeSuitePdf, false),
                ])
                  Padding(
                    padding:
                        const EdgeInsets.symmetric(vertical: HarborSpace.s1),
                    child: Row(children: [
                      Expanded(
                        child: Text(name, style: t.text.smallOf(t.colors.ink)),
                      ),
                      HarborPill(live
                          ? l10n.suiteStatusLive
                          : l10n.suiteStatusPreview),
                    ]),
                  ),
              ]),
            ),
          const SizedBox(height: HarborSpace.s2),
          Text(l10n.storeOfficeAlsoAvailable,
              style: t.text.captionOf(t.colors.inkMuted)),
          const SizedBox(height: HarborSpace.s3),
          OutlinedButton.icon(
            onPressed: widget.onOpenWork,
            icon: const Icon(Icons.work_outline),
            label: Text(l10n.storeOfficeOpen),
          ),
        ],
      ),
    );
  }
}

/// SEC-024 uninstall of an installed package: preview the exact owned-file
/// scope (blocked while in use), commit on confirmation, and offer Undo
/// for the trash window.
Future<void> uninstallModel(
    BuildContext context, HarborService service, String id) async {
  final l10n = AppLocalizations.of(context)!;
  try {
    final preview = await service.uninstallPreview(id);
    if (!context.mounted) return;
    final ok = await showUninstallConfirmDialog(context, preview);
    if (!ok) return;
    final result =
        await service.uninstallCommit(id, preview['scope_digest'] as String);
    if (!context.mounted) return;
    final trashEntry = result['trash_entry'] as String?;
    final messenger = ScaffoldMessenger.of(context);
    messenger.showSnackBar(SnackBar(
      content: Text(l10n.storeUninstallDone(id)),
      action: trashEntry == null
          ? null
          : SnackBarAction(
              label: l10n.storeUndo,
              onPressed: () async {
                try {
                  await service.uninstallRestore(trashEntry);
                } on ffi.HarborCoreException {
                  messenger.showSnackBar(
                      SnackBar(content: Text(l10n.storeUndoFailed)));
                }
              },
            ),
    ));
  } on ffi.HarborCoreException catch (e) {
    if (!context.mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('${l10n.storeUninstallFailed}: ${e.message}')));
  }
}

String _gb(int bytes) => '${(bytes / (1 << 30)).toStringAsFixed(2)} GB';

/// SEC-029 confirmation: shows the brokered quote next to real free
/// space; refuses to proceed when the preflight says it does not fit.
Future<bool> showAcquireConfirmDialog(
    BuildContext context, Map<String, dynamic> quote) async {
  final l10n = AppLocalizations.of(context)!;
  final t = HarborTheme.of(context);
  final fits = quote['fits'] == true;
  final result = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(l10n.storeAcquireTitle),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(l10n.storeAcquireBody(
            _gb((quote['quoted_bytes'] as num? ?? 0).toInt()),
            _gb((quote['available_bytes'] as num? ?? 0).toInt()),
          )),
          if (!fits) ...[
            const SizedBox(height: HarborSpace.s2),
            Text(l10n.storeAcquireNoFit,
                style: t.text.captionOf(t.colors.danger)),
          ],
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: Text(MaterialLocalizations.of(context).cancelButtonLabel),
        ),
        FilledButton(
          onPressed: fits ? () => Navigator.of(context).pop(true) : null,
          child: Text(l10n.storeAcquireConfirm),
        ),
      ],
    ),
  );
  return result == true;
}

/// SEC-024 deletion preview dialog: the exact owned-file scope, the bytes,
/// and any in-use blockers (which disable the confirm button).
Future<bool> showUninstallConfirmDialog(
    BuildContext context, Map<String, dynamic> preview) async {
  final l10n = AppLocalizations.of(context)!;
  final t = HarborTheme.of(context);
  final inUse = preview['in_use'] == true;
  final reasons = (preview['in_use_reasons'] as List? ?? const [])
      .cast<String>()
      .join(', ');
  final result = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(
          l10n.storeUninstallTitle(preview['package_id'] as String? ?? '')),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(l10n.storeUninstallBody(
            _gb((preview['manifest_bytes_total'] as num? ?? 0).toInt()),
            (preview['files'] as List? ?? const []).length,
          )),
          if (inUse) ...[
            const SizedBox(height: HarborSpace.s2),
            Text(l10n.storeUninstallInUse(reasons),
                style: t.text.captionOf(t.colors.danger)),
          ],
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: Text(MaterialLocalizations.of(context).cancelButtonLabel),
        ),
        FilledButton(
          style: FilledButton.styleFrom(
            backgroundColor: t.colors.danger,
          ),
          onPressed: inUse ? null : () => Navigator.of(context).pop(true),
          child: Text(l10n.storeUninstall),
        ),
      ],
    ),
  );
  return result == true;
}
