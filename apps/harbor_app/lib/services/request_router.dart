import 'package:harbor_domain/harbor_domain.dart';

import 'harbor_service.dart';

/// Which path a request takes, chosen by the person (the Ask / Change
/// toggle) or left to the router.
enum RequestMode { ask, change }

/// The file a request is about, when one is open (Work) or attached (Home).
final class RouteFile {
  const RouteFile({required this.name});
  final String name;
}

/// What the router recommends. Recommend-only: nothing here starts a run.
/// A [RouteSkill] is shown to the person in the skill's own run sheet,
/// where running and every approval stay explicit.
sealed class RouteDecision {
  const RouteDecision();
}

/// A question about content: answer it, grounded and cited.
final class RouteAsk extends RouteDecision {
  const RouteAsk({required this.fromCore});

  /// True when the core's decision layer chose it; false for the local
  /// fallback heuristic or a mode the person picked.
  final bool fromCore;
}

/// A task: open [skill]'s run sheet. [alternatives] are the other skills
/// the router ranked, best first, for the person to switch to.
final class RouteSkill extends RouteDecision {
  const RouteSkill(this.skill, {this.alternatives = const []});
  final SkillSummary skill;
  final List<SkillSummary> alternatives;
}

/// Why no single skill was recommended.
enum RouteNoneReason {
  /// No embedding index is open, so skills could not be ranked.
  noIndex,

  /// The core's skill router abstained (no skill fits well enough).
  noMatch,

  /// Two or more skills fit about equally; the person picks.
  ambiguous,

  /// The ranked skills cannot run here (e.g. none takes a file when one is
  /// open, or none has an executable graph).
  noRunnableSkill,
}

/// A task with no single recommended skill. [suggestions] are runnable
/// skills worth offering as choices (possibly empty).
final class RouteNone extends RouteDecision {
  const RouteNone(this.reason, {this.suggestions = const []});
  final RouteNoneReason reason;
  final List<SkillSummary> suggestions;
}

/// Routes a free-text request to Ask (grounded answer) or to a skill.
///
/// 1. A mode the person picked wins.
/// 2. Otherwise the core's decision layer (`decision.choose`) names the
///    KIND of action; `run_skill` is a task, `search_documents`,
///    `general_chat` and `recall_memory` are questions. When it abstains,
///    names another kind, or no index is open, [looksLikeQuestion] decides.
/// 3. A task is ranked by the core's skill router (`skills.suggest`). With
///    a file in context only skills that take a file qualify; without one,
///    only skills that do not require one. Without an index, skills whose
///    names share words with the request are offered as choices, never
///    picked.
///
/// The router never runs anything: it returns a [RouteDecision] for the
/// caller to SHOW.
class RequestRouter {
  RequestRouter(this.service, {this.openIndex = true});

  final HarborService service;

  /// Try to open the embedding index first when it is closed (the core's
  /// routers need it). Off in callers that already tried.
  final bool openIndex;

  /// Action kinds of `decision.choose` that are questions about content.
  static const askKinds = {'search_documents', 'general_chat', 'recall_memory'};

  Future<RouteDecision> route(
    String text, {
    RouteFile? file,
    RequestMode? mode,
    String? chatPackage,
  }) async {
    final request = text.trim();
    if (mode == RequestMode.ask) return const RouteAsk(fromCore: false);
    if (openIndex && !service.knowledgeOpen && request.isNotEmpty) {
      await service.openKnowledge();
    }
    if (mode == null) {
      final intent = await service.suggestIntent(request);
      final kind = _topKind(intent);
      if (kind != null && askKinds.contains(kind)) {
        return const RouteAsk(fromCore: true);
      }
      if (kind != 'run_skill' && looksLikeQuestion(request)) {
        return const RouteAsk(fromCore: false);
      }
    }
    return _routeSkill(request, file: file, chatPackage: chatPackage);
  }

  static String? _topKind(Map<String, dynamic>? intent) {
    if (intent == null || intent['abstained'] == true) return null;
    final ranked = (intent['ranked'] as List?) ?? const [];
    if (ranked.isEmpty || ranked.first is! Map) return null;
    return (ranked.first as Map)['id'] as String?;
  }

  /// Whether [skill] can be shown in a run sheet for this context.
  static bool fits(SkillSummary skill, {required bool hasFile}) {
    final graph = skill.graph;
    if (graph == null) return false;
    final takesFile = graph.inputProperties.any((e) => e.key == 'artifact_id');
    if (hasFile) return takesFile;
    return !graph.requiredInputs.contains('artifact_id');
  }

  Future<RouteDecision> _routeSkill(String request,
      {RouteFile? file, String? chatPackage}) async {
    final hasFile = file != null;
    final byId = {for (final s in service.skills) s.id: s};
    final result =
        await service.suggestSkills(request, chatPackage: chatPackage);
    if (result == null) {
      return RouteNone(RouteNoneReason.noIndex,
          suggestions:
              lexicalMatches(request, service.skills, hasFile: hasFile));
    }
    final ranked = [
      for (final r
          in ((result['ranked'] as List?) ?? const []).whereType<Map>())
        if (byId[r['skill_id']] case final SkillSummary s) s,
    ];
    final usable = [
      for (final s in ranked)
        if (fits(s, hasFile: hasFile)) s,
    ];
    if (result['abstained'] == true) {
      return RouteNone(
          result['reason'] == 'ambiguous'
              ? RouteNoneReason.ambiguous
              : RouteNoneReason.noMatch,
          suggestions: usable.take(4).toList());
    }
    if (usable.isEmpty) {
      return RouteNone(RouteNoneReason.noRunnableSkill,
          suggestions:
              lexicalMatches(request, service.skills, hasFile: hasFile));
    }
    return RouteSkill(usable.first,
        alternatives: usable.skip(1).take(3).toList());
  }

  // Openers of a question. Modal verbs ("can you…", "would you…") are left
  // out: they open requests to DO something as often as questions. The
  // end is a lookahead, not \b, which never matches after Arabic letters.
  static final _interrogative = RegExp(
    r"^(what|whats|what's|who|whom"
    r'|whose|when|where|why|how|which|is|are|'
    r'was|were|does|did|has|have|explain|tell me|'
    r'ما|ماذا|من|متى|أين|اين|لماذا|كيف|هل|كم|أي|اي|لم)(?=[\s,،:]|$)',
    caseSensitive: false,
    unicode: true,
  );

  /// The fallback heuristic: a question mark (Latin or Arabic) or a
  /// leading interrogative word makes a request a question.
  static bool looksLikeQuestion(String text) {
    final t = text.trim();
    if (t.isEmpty) return false;
    if (t.endsWith('?') || t.endsWith('؟')) return true;
    return _interrogative.hasMatch(t);
  }

  /// Runnable skills sharing at least one meaningful word with [request]
  /// (title, id or description), best overlap first, at most four. Shown
  /// as choices only — word overlap is too weak to pick a skill.
  static List<SkillSummary> lexicalMatches(
      String request, List<SkillSummary> skills,
      {required bool hasFile}) {
    final words = _words(request);
    if (words.isEmpty) return const [];
    final scored = <(int, SkillSummary)>[];
    for (final s in skills) {
      if (!fits(s, hasFile: hasFile)) continue;
      final name = _words('${s.title} ${s.id.replaceAll('-', ' ')}');
      final about = _words(s.description);
      final score = 3 * words.intersection(name).length +
          words.intersection(about).length;
      if (score > 0) scored.add((score, s));
    }
    scored.sort((a, b) => b.$1.compareTo(a.$1));
    return [for (final (_, s) in scored.take(4)) s];
  }

  static const _stop = {
    'the',
    'and',
    'for',
    'with',
    'this',
    'that',
    'from',
    'into',
    'about',
    'your',
    'you',
    'please',
    'make',
    'file',
    'document',
    'can',
    'will',
  };

  static Set<String> _words(String text) => {
        for (final w in text
            .toLowerCase()
            .split(RegExp(r'[^\p{L}\p{N}]+', unicode: true)))
          if (w.length >= 3 && !_stop.contains(w))
            w.length > 4 && w.endsWith('s') ? w.substring(0, w.length - 1) : w,
      };
}

/// The chat model a request runs with: the person's choice when it is still
/// installed, else the first installed model, else null (nothing can
/// generate: the caller shows the install-a-model guidance).
String? chatPackageFor(HarborService service, String? preferred) {
  final installed = [
    for (final m in service.installedModels)
      if (m['id'] is String) m['id'] as String
  ];
  if (installed.isEmpty) return null;
  return preferred != null && installed.contains(preferred)
      ? preferred
      : installed.first;
}

/// Files a skill can take as its artifact and the canvas can read: the
/// Office formats and PDF.
bool isOfficeName(String name) {
  final lower = name.toLowerCase();
  return lower.endsWith('.docx') ||
      lower.endsWith('.xlsx') ||
      lower.endsWith('.pptx') ||
      lower.endsWith('.pdf');
}
