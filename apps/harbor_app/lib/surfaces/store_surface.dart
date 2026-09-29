import 'package:flutter/material.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';
import '../widgets/ops.dart';

/// Store surface: included products (Harbor Office) and first-party model
/// management. Office is INCLUDED — the engine ships compiled in every
/// build, so the store's office card is an activation/status view, never a
/// download (platform policy 2.5.2 and Harbor's own resource policy).
/// Model downloads quote their size through the brokered preflight and
/// require an explicit confirmation before transfer (SEC-029); uninstall
/// previews the exact owned-file scope and keeps an undo window (SEC-024).
class StoreSurface extends StatelessWidget {
  const StoreSurface({super.key, required this.state});
  final AppState state;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (sp.failed || service == null) {
      return HarborErrorState(
          title: l10n.coreDegradedTitle, message: l10n.coreNotLoadedModels);
    }
    return HarborPage(
      maxWidth: HarborLayout.readingMax + 80,
      children: [
        _OfficeCard(onOpenWork: () => state.goTo(HarborSurface.work)),
        const SizedBox(height: HarborSpace.s5),
        _HarborModelsSection(service: service),
        const SizedBox(height: HarborSpace.s5),
        _InstalledSection(service: service),
        if (service.kindProgress['acquire'] != null) ...[
          const SizedBox(height: HarborSpace.s3),
          OpProgressCard(
            progress: service.kindProgress['acquire']!,
            service: service,
          ),
        ],
        const SizedBox(height: HarborSpace.s3),
        Text(l10n.storeManageHint, style: t.text.smallOf(t.colors.inkMuted)),
      ],
    );
  }
}

/// The Harbor Office card: the product ships inside this app. The card is
/// informational until the office activation gates pass; it never offers a
/// download, and there is deliberately no runtime "enable" toggle (the
/// feature registry is a build-time contract — dormant code keeps no live
/// authority path).
class _OfficeCard extends StatelessWidget {
  const _OfficeCard({required this.onOpenWork});
  final VoidCallback onOpenWork;

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
            HarborPill(l10n.storeOfficeIncluded),
          ]),
          const SizedBox(height: HarborSpace.s2),
          Text(l10n.storeOfficeStatus,
              style: t.text.captionOf(t.colors.inkMuted)),
          const SizedBox(height: HarborSpace.s3),
          OutlinedButton.icon(
            onPressed: onOpenWork,
            icon: const Icon(Icons.work_outline),
            label: Text(l10n.storeOfficeOpen),
          ),
        ],
      ),
    );
  }
}

/// First-party models: the accepted signed catalog. Each entry quotes the
/// real transfer through [showAcquireConfirmDialog] before acquiring; the
/// core refuses any transfer larger than what the user confirmed.
class _HarborModelsSection extends StatelessWidget {
  const _HarborModelsSection({required this.service});
  final HarborService service;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    if (!service.catalogImported || service.catalog.isEmpty) {
      return HarborEmptyState(
        icon: Icons.storefront_outlined,
        title: l10n.surfaceStore,
        body: l10n.storeEmpty,
      );
    }
    final packages = service.catalog
        .where((p) => !(p['tiers'] as List? ?? const []).contains('Test'))
        .toList()
      ..sort((a, b) {
        int first(Map p) => p['publisher'] == 'harbor' ? 0 : 1;
        return first(a).compareTo(first(b));
      });
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        HarborSectionHeader(title: l10n.storeModelsHeading(packages.length)),
        const SizedBox(height: HarborSpace.s2),
        Text(l10n.storeModelsBody, style: t.text.captionOf(t.colors.inkMuted)),
        const SizedBox(height: HarborSpace.s3),
        for (final p in packages)
          Padding(
            padding: const EdgeInsets.only(bottom: HarborSpace.s3),
            child: _StoreModelCard(package: p, service: service),
          ),
      ],
    );
  }
}

class _StoreModelCard extends StatefulWidget {
  const _StoreModelCard({required this.package, required this.service});
  final Map<String, dynamic> package;
  final HarborService service;

  @override
  State<_StoreModelCard> createState() => _StoreModelCardState();
}

class _StoreModelCardState extends State<_StoreModelCard> {
  bool _busy = false;
  String? _error;

  Map<String, dynamic> get p => widget.package;
  bool get _installed => p['installed'] == true;

  List<Map<String, String>> get _files => [
        for (final f in (p['files'] as List? ?? const []).cast<Map>())
          {
            'path': f['path'] as String,
            'role': f['role'] as String? ?? 'weights',
            'sha256': f['sha256'] as String? ?? '',
          }
      ];

  Future<void> _install() async {
    final l10n = AppLocalizations.of(context)!;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final quote = await widget.service.acquirePreflight(
        repoId: p['repo_id'] as String,
        files: _files,
      );
      if (!mounted) return;
      final confirmed = await showAcquireConfirmDialog(context, quote);
      if (!mounted || !confirmed) {
        setState(() => _busy = false);
        return;
      }
      await widget.service.acquireModelHf(
        packageId: p['id'] as String,
        repoId: p['repo_id'] as String,
        files: _files,
        confirmedTotalBytes: (quote['quoted_bytes'] as num? ?? 0).toInt(),
      );
    } on ffi.HarborCoreException catch (e) {
      if (!mounted) return;
      setState(() => _error = e.message.toLowerCase().contains('cancel')
          ? l10n.modelInstallCancelled
          : l10n.modelInstallFailed);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    final tiers = (p['tiers'] as List? ?? const []).cast<String>();
    return HarborCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(children: [
            Expanded(
              child: Text(p['id'] as String? ?? '',
                  style: t.text.bodyStrongOf(t.colors.ink)),
            ),
            if (p['publisher'] == 'harbor')
              const Padding(
                padding: EdgeInsetsDirectional.only(end: HarborSpace.s2),
                child: HarborPill('Harbor', brand: true),
              ),
            if (_installed)
              HarborPill(AppLocalizations.of(context)!.modelsInstalledBadge),
          ]),
          const SizedBox(height: HarborSpace.s1),
          Text(
            [
              p['quantization'] as String? ?? 'Q4_K_M',
              AppLocalizations.of(context)!.modelsContextTokens(
                  (p['context_tokens'] as num? ?? 2048).toInt()),
              ...tiers,
            ].join(' · '),
            style: t.text.captionOf(t.colors.inkMuted),
          ),
          if (_error != null) ...[
            const SizedBox(height: HarborSpace.s2),
            Text(_error!, style: t.text.captionOf(t.colors.danger)),
          ],
          const SizedBox(height: HarborSpace.s3),
          FilledButton.icon(
            onPressed: _busy || _installed ? null : _install,
            icon: const Icon(Icons.download_outlined),
            label: Text(AppLocalizations.of(context)!.modelInstallAction),
          ),
        ],
      ),
    );
  }
}

/// Installed packages with the SEC-024 uninstall flow: preview the exact
/// scope, block when in use, keep a 72-hour undo window.
class _InstalledSection extends StatelessWidget {
  const _InstalledSection({required this.service});
  final HarborService service;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    if (service.installedModels.isEmpty) {
      return const SizedBox.shrink();
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        HarborSectionHeader(title: l10n.storeInstalledHeading),
        const SizedBox(height: HarborSpace.s2),
        for (final m in service.installedModels)
          Padding(
            padding: const EdgeInsets.only(bottom: HarborSpace.s3),
            child: _InstalledCard(model: m, service: service),
          ),
      ],
    );
  }
}

class _InstalledCard extends StatelessWidget {
  const _InstalledCard({required this.model, required this.service});
  final Map<String, dynamic> model;
  final HarborService service;

  Future<void> _uninstall(BuildContext context) async {
    final l10n = AppLocalizations.of(context)!;
    final id = model['id'] as String? ?? '';
    try {
      final preview = await service.uninstallPreview(id);
      if (!context.mounted) return;
      final ok = await showUninstallConfirmDialog(context, preview);
      if (!ok) return;
      final result = await service.uninstallCommit(
          id, preview['scope_digest'] as String);
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
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('${l10n.storeUninstallFailed}: ${e.message}')));
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = HarborTheme.of(context);
    return HarborCard(
      child: Row(children: [
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(model['id'] as String? ?? '',
                  style: t.text.bodyStrongOf(t.colors.ink)),
              const SizedBox(height: HarborSpace.s1),
              Text(
                '${_gb((model['total_bytes'] as num? ?? 0).toInt())} · '
                '${model['files']} files · ${model['runtime']}',
                style: t.text.captionOf(t.colors.inkMuted),
              ),
            ],
          ),
        ),
        OutlinedButton.icon(
          style: OutlinedButton.styleFrom(
            foregroundColor: t.colors.danger,
          ),
          onPressed: () => _uninstall(context),
          icon: const Icon(Icons.delete_outline),
          label: Text(AppLocalizations.of(context)!.storeUninstall),
        ),
      ]),
    );
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
  final reasons =
      (preview['in_use_reasons'] as List? ?? const []).cast<String>().join(', ');
  final result = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(l10n.storeUninstallTitle(preview['package_id'] as String? ?? '')),
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
