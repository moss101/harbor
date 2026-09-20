import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:harbor_native/harbor_ffi.dart' as ffi;
import 'package:harbor_ui/harbor_ui.dart';

import '../l10n/app_localizations.dart';
import '../main.dart';
import '../services/harbor_service.dart';
import '../widgets/ops.dart';
import '../widgets/trust.dart';

/// Models surface (goal §23): Recommended / Library / Hugging Face /
/// Installed / Benchmark with Fit Score first, plus Import (a real local
/// GGUF install through the staged installer). Acquisition and import run
/// as cancellable background ops with live progress.
class ModelsSurface extends StatefulWidget {
  const ModelsSurface({super.key});

  @override
  State<ModelsSurface> createState() => _ModelsSurfaceState();
}

class _ModelsSurfaceState extends State<ModelsSurface>
    with SingleTickerProviderStateMixin {
  late final TabController _tabs = TabController(length: 5, vsync: this);
  bool _importing = false;

  @override
  void dispose() {
    _tabs.dispose();
    super.dispose();
  }

  /// Import a local GGUF file: package id from the file name, installed
  /// through the core's staged installer (data only, never executed).
  Future<void> _importLocal() async {
    final service = HarborServiceProvider.of(context).notifier;
    if (service == null || _importing) return;
    final l10n = AppLocalizations.of(context)!;
    final XFile? file;
    try {
      file = await openFile(acceptedTypeGroups: [
        XTypeGroup(label: l10n.fileGroupModels, extensions: const ['gguf']),
      ]);
    } catch (_) {
      return;
    }
    if (file == null || !mounted) return;
    final name = file.name;
    final packageId = name.toLowerCase().endsWith('.gguf')
        ? name.substring(0, name.length - 5)
        : name;
    setState(() => _importing = true);
    final ok = await service.installModelFromPath(
        packageId: packageId, path: file.path);
    if (!mounted) return;
    setState(() => _importing = false);
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(
      content: Text(
          ok ? l10n.importModelInstalled(packageId) : l10n.importModelFailed),
    ));
    if (ok) _tabs.animateTo(3);
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    final installed = service?.installedModels.length ?? 0;
    final wc = HarborBreakpoints.of(context);
    return Column(children: [
      HarborSurfaceHeader(
        title: l10n.surfaceModels,
        subtitle: l10n.modelsSubtitle,
        actions: [
          if (service != null && !sp.failed) ...[
            HarborPill(l10n.modelsInstalledCount(installed),
                icon: Icons.memory_outlined, brand: installed > 0),
            OutlinedButton.icon(
              onPressed: _importing ? null : _importLocal,
              icon: _importing
                  ? const SizedBox(
                      width: 14,
                      height: 14,
                      child: CircularProgressIndicator(strokeWidth: 2))
                  : const Icon(Icons.file_download_outlined, size: 16),
              label: Text(l10n.modelsImportGguf),
            ),
          ],
        ],
      ),
      TabBar(
        controller: _tabs,
        isScrollable: true,
        padding: EdgeInsets.symmetric(
            horizontal: HarborBreakpoints.gutter(wc) - HarborSpace.s3),
        tabs: [
          Tab(text: l10n.modelsRecommended),
          Tab(text: l10n.modelsLibrary),
          Tab(text: l10n.modelsHuggingFace),
          Tab(text: l10n.modelsInstalled),
          Tab(text: l10n.modelsBenchmark),
        ],
      ),
      Expanded(
        child: TabBarView(
          controller: _tabs,
          children: [
            _RecommendedTab(
              onHuggingFace: () => _tabs.animateTo(2),
              onImport: _importLocal,
            ),
            HarborEmptyState(
              icon: Icons.local_library_outlined,
              title: l10n.modelsLibrary,
              body: l10n.modelsLibraryEmpty,
            ),
            const _HfSearchView(),
            _InstalledTab(onRecommended: () => _tabs.animateTo(0)),
            HarborEmptyState(
              icon: Icons.speed_outlined,
              title: l10n.modelsBenchmark,
              body: l10n.modelsBenchmarkEmpty,
            ),
          ],
        ),
      ),
    ]);
  }
}

/// Recommended (production plan C2): the accepted signed catalog, offline.
/// Each entry shows its tier, quantization and context; "Check size & fit"
/// resolves the weights size through the brokered metadata read and asks
/// the core for a Fit Score; "Install" runs the real acquisition with the
/// catalog's pinned sha256. Nothing is ranked or scored from guesses.
class _RecommendedTab extends StatelessWidget {
  const _RecommendedTab({required this.onHuggingFace, required this.onImport});
  final VoidCallback onHuggingFace;
  final VoidCallback onImport;

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
    final packages = service.catalog
        .where((p) => !(p['tiers'] as List? ?? const []).contains('Test'))
        .toList()
      ..sort((a, b) {
        // Chat tiers first (Balanced/Quality/Fast), embeddings after.
        int rank(Map<String, dynamic> p) =>
            (p['tiers'] as List? ?? const []).contains('Embeddings') ? 1 : 0;
        return rank(a).compareTo(rank(b));
      });
    if (!service.catalogImported || packages.isEmpty) {
      return HarborEmptyState(
        icon: Icons.recommend_outlined,
        title: l10n.modelsRecommended,
        body: l10n.modelsRecommendedBody,
        actionLabel: l10n.modelsGoHuggingFace,
        onAction: onHuggingFace,
        secondaryActionLabel: l10n.modelsImportGguf,
        onSecondaryAction: onImport,
      );
    }
    return HarborPage(
      maxWidth: HarborLayout.readingMax + 80,
      children: [
        if (service.needsFirstModel)
          Padding(
            padding: const EdgeInsets.only(bottom: HarborSpace.s4),
            child: HarborBanner(
              key: const ValueKey('models-first-run'),
              tone: HarborBannerTone.info,
              icon: Icons.shield_outlined,
              title: l10n.modelsFirstRunTitle,
              body: l10n.modelsFirstRunBody,
            ),
          ),
        Text(l10n.modelsCatalogHeading(service.catalog.length),
            style: t.text.captionOf(t.colors.inkMuted)),
        const SizedBox(height: HarborSpace.s2),
        for (final p in packages)
          Padding(
            padding: const EdgeInsets.only(bottom: HarborSpace.s3),
            child: _CatalogCard(package: p, service: service),
          ),
        const SizedBox(height: HarborSpace.s3),
        Text(l10n.modelsCatalogFooter,
            style: t.text.smallOf(t.colors.inkMuted)),
      ],
    );
  }
}

class _CatalogCard extends StatefulWidget {
  const _CatalogCard({required this.package, required this.service});
  final Map<String, dynamic> package;
  final HarborService service;

  @override
  State<_CatalogCard> createState() => _CatalogCardState();
}

class _CatalogCardState extends State<_CatalogCard> {
  int? _weightsBytes;
  Map<String, dynamic>? _fit;
  bool _checking = false;
  bool _installing = false;
  String? _error;

  Map<String, dynamic> get p => widget.package;
  List<Map<String, dynamic>> get _files => (p['files'] as List? ?? const [])
      .cast<Map>()
      .map((m) => m.cast<String, dynamic>())
      .toList();
  bool get _installed => p['installed'] == true;

  Future<void> _checkFit() async {
    final l10n = AppLocalizations.of(context)!;
    setState(() {
      _checking = true;
      _error = null;
    });
    try {
      final listing =
          await widget.service.huggingFaceFiles(p['repo_id'] as String);
      final wanted = _files.map((f) => f['path'] as String).toSet();
      var bytes = 0;
      for (final f in listing) {
        if (wanted.contains(f['path'])) {
          bytes += (f['size'] as num? ?? 0).toInt();
        }
      }
      if (bytes == 0) {
        if (mounted) setState(() => _error = l10n.modelsCatalogSizeUnavailable);
        return;
      }
      final fit = await widget.service.fitEstimate(
        weightsBytes: bytes,
        contextTokens: (p['context_tokens'] as num? ?? 2048).toInt(),
        quantization: p['quantization'] as String? ?? 'Q4_K_M',
      );
      if (!mounted) return;
      setState(() {
        _weightsBytes = bytes;
        _fit = fit;
      });
    } finally {
      if (mounted) setState(() => _checking = false);
    }
  }

  Future<void> _install() async {
    final l10n = AppLocalizations.of(context)!;
    setState(() {
      _installing = true;
      _error = null;
    });
    try {
      await widget.service.acquireModelHf(
        packageId: p['id'] as String,
        repoId: p['repo_id'] as String,
        files: [
          for (final f in _files)
            {
              'path': f['path'] as String,
              'role': f['role'] as String? ?? 'weights',
              'sha256': f['sha256'] as String? ?? '',
            },
        ],
      );
    } on ffi.HarborCoreException catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e.message.toLowerCase().contains('cancel')
            ? l10n.modelInstallCancelled
            : l10n.modelInstallFailed;
      });
    } finally {
      if (mounted) setState(() => _installing = false);
    }
  }

  static String _gb(int bytes) =>
      '${(bytes / (1 << 30)).toStringAsFixed(2)} GB';

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final tiers = (p['tiers'] as List? ?? const []).cast<String>();
    final fit = _fit;
    return HarborCard(
      key: ValueKey('catalog-${p['id']}'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Wrap(
            spacing: HarborSpace.s2,
            runSpacing: HarborSpace.s1,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Text(p['id'] as String, style: t.text.bodyStrongOf(t.colors.ink)),
              for (final tier in tiers)
                StatusBadge(semantic: ExecutionSemantic.local, label: tier),
              if (_installed)
                StatusBadge(
                    semantic: ExecutionSemantic.local,
                    label: l10n.modelsInstalledBadge),
            ],
          ),
          const SizedBox(height: HarborSpace.s2),
          Text(
            [
              p['repo_id'] as String,
              if (p['quantization'] != null) p['quantization'] as String,
              if (p['context_tokens'] != null)
                l10n.modelsContextTokens((p['context_tokens'] as num).toInt()),
              if (p['license'] != null) p['license'] as String,
              if (_weightsBytes != null) _gb(_weightsBytes!),
            ].join(' · '),
            style: t.text.smallOf(t.colors.inkMuted),
          ),
          if (fit != null) ...[
            const SizedBox(height: HarborSpace.s2),
            FitScoreBadge(
              band: FitBandLabel.parse(fit['band'] as String? ?? ''),
              reasons: (fit['reasons'] as List? ?? const []).cast<String>(),
            ),
          ],
          if (_error != null) ...[
            const SizedBox(height: HarborSpace.s2),
            Text(_error!, style: t.text.smallOf(t.colors.statusDangerText)),
          ],
          const SizedBox(height: HarborSpace.s3),
          Wrap(
            alignment: WrapAlignment.end,
            spacing: HarborSpace.s2,
            runSpacing: HarborSpace.s2,
            children: [
              OutlinedButton.icon(
                key: ValueKey('catalog-fit-${p['id']}'),
                onPressed: _checking || _installing ? null : _checkFit,
                icon: const Icon(Icons.speed_outlined, size: 18),
                label: Text(_checking
                    ? l10n.modelsFitComputing
                    : l10n.modelsCatalogCheckFit),
              ),
              FilledButton.icon(
                key: ValueKey('catalog-install-${p['id']}'),
                onPressed: _installed || _installing ? null : _install,
                icon: const Icon(Icons.download_outlined, size: 18),
                label: Text(_installed
                    ? l10n.modelsInstalledBadge
                    : (_installing
                        ? l10n.opAcquireRunning
                        : l10n.modelsCatalogInstall)),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _InstalledTab extends StatelessWidget {
  const _InstalledTab({required this.onRecommended});
  final VoidCallback onRecommended;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (sp.failed || service == null) {
      return HarborErrorState(
          title: l10n.coreDegradedTitle, message: l10n.coreNotLoadedModels);
    }
    final models = service.installedModels;
    if (models.isEmpty) {
      return HarborEmptyState(
        icon: Icons.memory_outlined,
        title: l10n.modelsInstalled,
        body: l10n.modelsInstalledEmpty,
        actionLabel: l10n.modelsRecommended,
        // Real navigation: the Recommended tab is one tap away.
        onAction: onRecommended,
      );
    }
    final state = AppStateScope.maybeOf(context);
    final chosen = state?.chatModel ?? models.first['id'];
    return HarborPage(
      padding: EdgeInsets.all(
          HarborBreakpoints.gutter(HarborBreakpoints.of(context))),
      children: [
        for (final m in models)
          Padding(
            padding: const EdgeInsets.only(bottom: HarborSpace.s3),
            child: _InstalledCard(
              model: m,
              inUse: m['id'] == chosen,
              runtimeLabel: runtimeLabelFor(service),
              onUse: state == null
                  ? null
                  : () => state.setChatModel(m['id'] as String),
            ),
          ),
      ],
    );
  }
}

class _InstalledCard extends StatelessWidget {
  const _InstalledCard({
    required this.model,
    required this.inUse,
    required this.runtimeLabel,
    required this.onUse,
  });
  final Map<String, dynamic> model;
  final bool inUse;
  final String runtimeLabel;
  final VoidCallback? onUse;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final id = model['id'] as String;
    final bytes = (model['total_bytes'] as num?)?.toInt() ?? 0;
    final files = (model['files'] as num?)?.toInt() ?? 0;
    final runtime = model['runtime'] as String? ?? '';
    final mb = bytes ~/ (1024 * 1024);
    final size = mb >= 1024 ? '${(mb / 1024).toStringAsFixed(1)} GB' : '$mb MB';
    return HarborCard(
      selected: inUse,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Container(
              width: 40,
              height: 40,
              decoration: BoxDecoration(
                color: t.colors.brandSoft,
                borderRadius: BorderRadius.circular(HarborRadius.sm),
              ),
              child:
                  Icon(Icons.memory_outlined, size: 20, color: t.colors.brand),
            ),
            const SizedBox(width: HarborSpace.s3),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  HarborIdentifier(id, size: 14, maxLines: 2),
                  const SizedBox(height: HarborSpace.s1),
                  Text(l10n.filesSizeRuntime(files, mb, runtime),
                      style: t.text.captionOf(t.colors.inkMuted)),
                ],
              ),
            ),
            if (inUse)
              StatusBadge(
                semantic: ExecutionSemantic.local,
                icon: Icons.check_circle_outline,
                label: l10n.modelsInUse,
              ),
          ]),
          const SizedBox(height: HarborSpace.s3),
          Wrap(
            spacing: HarborSpace.s2,
            runSpacing: HarborSpace.s2,
            children: [
              HarborPill('${l10n.modelsSize} $size',
                  icon: Icons.storage_outlined),
              HarborPill('${l10n.modelsFiles} $files',
                  icon: Icons.folder_outlined),
              HarborPill(runtime, icon: Icons.settings_suggest_outlined),
              StatusBadge(
                  semantic: ExecutionSemantic.local, label: runtimeLabel),
            ],
          ),
          const SizedBox(height: HarborSpace.s3),
          Divider(height: 1, color: t.colors.borderSubtle),
          const SizedBox(height: HarborSpace.s3),
          Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Expanded(child: _FitBadge(packageId: id)),
            if (!inUse && onUse != null)
              TextButton.icon(
                onPressed: onUse,
                icon: const Icon(Icons.question_answer_outlined, size: 16),
                label: Text(l10n.modelsUseForAsk),
              ),
          ]),
        ],
      ),
    );
  }
}

/// Fit Score for one installed model, computed on the worker isolate.
class _FitBadge extends StatefulWidget {
  const _FitBadge({required this.packageId});
  final String packageId;

  @override
  State<_FitBadge> createState() => _FitBadgeState();
}

class _FitBadgeState extends State<_FitBadge> {
  Map<String, dynamic>? _fit;
  bool _requested = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (!_requested) {
      _requested = true;
      final service = HarborServiceProvider.of(context).notifier;
      service?.fitScore(widget.packageId).then((fit) {
        if (mounted) setState(() => _fit = fit);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final t = HarborTheme.of(context);
    final fit = _fit;
    if (fit == null) {
      return Row(mainAxisSize: MainAxisSize.min, children: [
        const SizedBox(
            width: 12,
            height: 12,
            child: CircularProgressIndicator(strokeWidth: 2)),
        const SizedBox(width: HarborSpace.s2),
        Text(l10n.modelsFitComputing,
            style: t.text.captionOf(t.colors.inkMuted)),
      ]);
    }
    final band = FitBandLabel.parse(fit['band'] as String? ?? '');
    final label = switch (band) {
      FitBand.excellent => l10n.fitLabelExcellent,
      FitBand.good => l10n.fitLabelGood,
      FitBand.limited => l10n.fitLabelLimited,
      FitBand.tooLarge => l10n.fitLabelTooLarge,
      FitBand.unsupported => l10n.fitLabelUnsupported,
    };
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Text(l10n.fitScore, style: t.text.captionOf(t.colors.inkMuted)),
      const SizedBox(height: HarborSpace.s1),
      FitScoreBadge(
        band: band,
        label: label,
        reasons: (fit['reasons'] as List? ?? const []).cast<String>(),
      ),
    ]);
  }
}

/// Hugging Face discovery + acquisition through the brokered core path.
/// The query is acquisition metadata only; downloaded packages are hashed
/// and installed via the staged installer — as a real background op with
/// live progress and cancel (no fake "Installed" flips).
class _HfSearchView extends StatefulWidget {
  const _HfSearchView();

  @override
  State<_HfSearchView> createState() => _HfSearchViewState();
}

class _HfSearchViewState extends State<_HfSearchView> {
  final _controller = TextEditingController();
  List<Map<String, dynamic>>? _results;
  bool _searching = false;
  bool _searchFailed = false;
  final Set<String> _installedIds = {};
  String? _installError;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _search() async {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null || _controller.text.trim().isEmpty) return;
    setState(() => _searching = true);
    final results = await service.searchHuggingFace(_controller.text.trim());
    if (!mounted) return;
    setState(() {
      _results = results;
      _searching = false;
      _searchFailed = results == null;
    });
  }

  /// Real acquisition: resolve the repo's GGUF files through the broker,
  /// then run the cancellable acquire op. Progress renders from the
  /// service's kind-tracked snapshot.
  Future<void> _install(String repoId) async {
    final sp = HarborServiceProvider.of(context);
    final service = sp.notifier;
    if (service == null || !mounted) return;
    final l10n = AppLocalizations.of(context)!;
    setState(() => _installError = null);
    final files = await service.huggingFaceFiles(repoId);
    if (!mounted) return;
    if (files.isEmpty) {
      setState(() => _installError = l10n.modelInstallFailed);
      return;
    }
    try {
      final result = await service.acquireModelHf(
        packageId: repoId.split('/').last,
        repoId: repoId,
        files: [
          for (final f in files)
            {'path': f['path'] as String, 'role': 'weights', 'sha256': ''},
        ],
      );
      if (!mounted) return;
      setState(() {
        if (result['installed'] != null) {
          _installedIds.add(repoId);
        } else {
          _installError = l10n.modelInstallFailed;
        }
      });
    } on ffi.HarborCoreException catch (e) {
      if (!mounted) return;
      setState(() {
        _installError = e.message.toLowerCase().contains('cancel')
            ? l10n.modelInstallCancelled
            : l10n.modelInstallFailed;
      });
    }
  }

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
    final acquiring = service.kindProgress['acquire'];
    final gutter = HarborBreakpoints.gutter(HarborBreakpoints.of(context));
    final results = _results;
    return Column(children: [
      Padding(
        padding:
            EdgeInsets.fromLTRB(gutter, HarborSpace.s4, gutter, HarborSpace.s2),
        child: TextField(
          controller: _controller,
          onSubmitted: (_) => _search(),
          textInputAction: TextInputAction.search,
          decoration: InputDecoration(
            hintText: l10n.modelsHfSearchHint,
            prefixIcon: const Icon(Icons.search),
            suffixIcon: IconButton(
              tooltip: l10n.askSearchTooltip,
              onPressed: _searching ? null : _search,
              icon: const Icon(Icons.arrow_forward),
            ),
          ),
        ),
      ),
      if (acquiring != null)
        Padding(
          padding: EdgeInsets.fromLTRB(
              gutter, HarborSpace.s2, gutter, HarborSpace.s2),
          child: OpProgressCard(progress: acquiring, service: service),
        ),
      if (_installError != null)
        Padding(
          padding: EdgeInsets.fromLTRB(
              gutter, HarborSpace.s2, gutter, HarborSpace.s2),
          child: HarborBanner(
            tone: HarborBannerTone.danger,
            dense: true,
            title: _installError!,
          ),
        ),
      Expanded(
        child: _searching
            ? HarborLoadingState(label: l10n.askSearchTooltip)
            : results == null
                ? HarborEmptyState(
                    icon: Icons.travel_explore_outlined,
                    title: l10n.modelsHuggingFace,
                    body: l10n.modelsHfEmpty,
                  )
                : results.isEmpty
                    ? HarborEmptyState(
                        icon: _searchFailed
                            ? Icons.cloud_off_outlined
                            : Icons.search_off_outlined,
                        title: l10n.modelsHuggingFace,
                        body: _searchFailed
                            ? l10n.modelsHfSearchFailed
                            : l10n.askNoEvidenceTitle,
                        actionLabel:
                            _searchFailed ? l10n.askSearchTooltip : null,
                        onAction: _searchFailed ? _search : null,
                      )
                    : ListView.builder(
                        padding: EdgeInsets.fromLTRB(
                            gutter, HarborSpace.s2, gutter, HarborSpace.s8),
                        itemCount: results.length,
                        itemBuilder: (context, i) {
                          final m = results[i];
                          final id = m['id'] as String;
                          final installed = _installedIds.contains(id);
                          return Padding(
                            padding:
                                const EdgeInsets.only(bottom: HarborSpace.s2),
                            child: HarborCard(
                              padding: const EdgeInsets.all(HarborSpace.s3),
                              child: Row(children: [
                                Expanded(
                                  child: Column(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      HarborIdentifier(id,
                                          size: 13, maxLines: 2),
                                      const SizedBox(height: HarborSpace.s1),
                                      Wrap(
                                        spacing: HarborSpace.s2,
                                        runSpacing: HarborSpace.s1,
                                        children: [
                                          HarborPill(
                                              l10n.modelsDownloads(
                                                  (m['downloads'] as num?)
                                                          ?.toInt() ??
                                                      0),
                                              icon: Icons.download_outlined),
                                          HarborPill(
                                              l10n.modelsLikes(
                                                  (m['likes'] as num?)
                                                          ?.toInt() ??
                                                      0),
                                              icon: Icons.favorite_border),
                                        ],
                                      ),
                                    ],
                                  ),
                                ),
                                const SizedBox(width: HarborSpace.s3),
                                if (installed)
                                  StatusBadge(
                                      semantic: ExecutionSemantic.local,
                                      label: l10n.statusInstalled)
                                else
                                  FilledButton(
                                    onPressed: acquiring != null
                                        ? null
                                        : () => _install(id),
                                    child: Text(l10n.modelInstallAction),
                                  ),
                              ]),
                            ),
                          );
                        },
                      ),
      ),
      if (results != null && results.isNotEmpty)
        Padding(
          padding: EdgeInsets.fromLTRB(gutter, 0, gutter, HarborSpace.s3),
          child: Text(l10n.modelsHfEmpty,
              style: t.text.captionOf(t.colors.inkMuted)),
        ),
    ]);
  }
}
