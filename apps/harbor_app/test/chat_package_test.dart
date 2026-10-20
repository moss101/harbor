import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/services/request_router.dart';

import 'fake_service.dart';

/// The default chat model is never an embedder (found by the live Work
/// test: bge-small sorted before the chat model and every ask failed with
/// a chat-template error).
void main() {
  FakeHarborService withRoles(Map<String, String?> roles) =>
      FakeHarborService(models: [
        for (final e in roles.entries)
          {'id': e.key, if (e.value != null) 'role': e.value}
      ]);

  FakeHarborService with_(List<String> ids) => FakeHarborService(models: [
        for (final i in ids) {'id': i}
      ]);

  test('embedders are skipped when choosing the default chat model', () {
    final s = with_([
      'bge-small-en-v1.5',
      'embeddinggemma-2',
      'multilingual-e5-small',
      'qwen2.5-1.5b-chat',
    ]);
    expect(chatPackageFor(s, null), 'qwen2.5-1.5b-chat');
    expect(chatPackageFor(s, 'gone'), 'qwen2.5-1.5b-chat');
  });

  test('only embedders installed: nothing can generate', () {
    expect(chatPackageFor(with_(['bge-m3', 'embed-12abc']), null), isNull);
  });

  test('an explicit choice is honoured, and ordinary ids are not embedders',
      () {
    final s = with_(['bge-small-en-v1.5', 'llama-3.2-1b', 'stories260k']);
    expect(chatPackageFor(s, 'stories260k'), 'stories260k');
    expect(chatPackageFor(s, null), 'llama-3.2-1b');
    for (final id in [
      'Llama-3.2-1B-Instruct',
      'qwen2.5-1.5b-chat',
      'smolvlm2-500m',
      'gemma-3-4b',
      'stories260k',
    ]) {
      expect(looksLikeEmbedder(id), isFalse, reason: id);
    }
    for (final id in [
      'bge-small-en-v1.5',
      'embeddinggemma-2',
      'multilingual-e5-small',
      'gte-base',
      'all-minilm-l6'
    ]) {
      expect(looksLikeEmbedder(id), isTrue, reason: id);
    }
  });

  test('the core role wins over the id heuristic', () {
    // An embedder with an innocuous id is skipped; a chat model whose id
    // looks like an embedder is accepted.
    final s = withRoles({
      'mystery-7b': 'embedder',
      'bge-chat-tuned': 'chat',
      'zeta': 'chat',
    });
    expect(chatPackageFor(s, null), 'bge-chat-tuned');
    expect(chatPackageFor(s, 'zeta'), 'zeta');
    // A stale preference that is an embedder is not honoured.
    expect(chatPackageFor(s, 'mystery-7b'), 'bge-chat-tuned');
    expect(chatModelsOf(s).map((m) => m['id']), ['bge-chat-tuned', 'zeta']);
  });

  test('unknown or absent role falls back to the id guard', () {
    final s = withRoles({
      'bge-small-en-v1.5': 'unknown',
      'embeddinggemma-2': null,
      'llama-3.2-1b': 'unknown',
    });
    expect(chatPackageFor(s, null), 'llama-3.2-1b');
    expect(chatPackageFor(withRoles({'bge-m3': 'unknown'}), null), isNull);
    expect(chatPackageFor(withRoles({'only': 'embedder'}), null), isNull);
  });
}
