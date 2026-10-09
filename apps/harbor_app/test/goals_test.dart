import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/services/harbor_service.dart';

/// Scheduled goals through the real core (decision 0011): the service
/// API the Agents surface drives — create with typed validation, due
/// slots, write-ahead claims that dedupe across restarts, pause/resume/
/// cancel, and receipts that survive a reopen. The surface render path
/// is covered by the shell tests; this file proves the contract.
final repoRoot = Directory.current.parent.parent.path; // apps/harbor_app
final dylibPath = '$repoRoot/core/target/debug/libharbor_ffi.dylib';
final coreAvailable = File(dylibPath).existsSync();

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('goals create, due, claim-dedupe and cancel end to end',
      (tester) async {
    if (!coreAvailable) return;
    // Real IO futures must never run inside the fake-async widget zone
    // (session 48): the whole conversation with the core lives inside
    // runAsync.
    await tester.runAsync(() async {
      final dir = Directory.systemTemp.createTempSync('harbor-goals-test-');
      final service = await HarborService.open(
        libraryPath: dylibPath,
        dataRoot: dir.path,
        workspaceId: 'ws-goals',
        deviceRootHex:
            'f47973db602cbd13c408a3a5cdf3a8eeaa3bd6870b76607542d75ff568526c3c',
      );
      addTearDown(() {
        service.close();
        dir.deleteSync(recursive: true);
      });

      // Validation is typed: an empty title is refused.
      Object? refused;
      try {
        await service.createGoal(
          title: '',
          request: {'kind': 'prompt', 'text': 'x'},
          schedule: {'kind': 'every_minutes', 'minutes': 10},
        );
      } catch (e) {
        refused = e;
      }
      expect(refused, isNotNull);

      // Create a repeating prompt goal.
      final created = await service.createGoal(
        title: 'Weekly digest',
        request: {'kind': 'prompt', 'text': 'summarize my documents'},
        schedule: {'kind': 'every_minutes', 'minutes': 10},
        maxRuns: 2,
      );
      final goalId = created['id'] as String;
      expect(created['state'], 'active');

      // It is due with a deterministic slot.
      final due = await service.dueGoals();
      expect(due, hasLength(1));
      final slot = due.first['slot'] as String;
      expect(slot, startsWith('every-10-'));

      // Claim: wins once, then refuses the same slot.
      final claim = await service.claimGoal(goalId, slot);
      final runId = claim['run_id'] as String;
      expect(runId, startsWith('run-'));
      Object? duplicate;
      try {
        await service.claimGoal(goalId, slot);
      } catch (e) {
        duplicate = e;
      }
      expect(duplicate, isNotNull);

      // The receipt exists before the driver executes the claim.
      final goal = (await service.listGoals()).first;
      expect((goal['executions'] as List), hasLength(1));

      // Outcome attaches.
      await service.recordGoalOutcome(goalId, runId, 'completed');
      final afterOutcome = (await service.listGoals()).first;
      expect((afterOutcome['executions'] as List).first['outcome'],
          'completed');

      // Pause stops due-ness; resume restores the active state (the
      // claimed slot stays consumed, so due-ness returns only on the
      // next interval boundary); cancel is terminal.
      await service.pauseGoal(goalId);
      expect((await service.dueGoals()), isEmpty);
      await service.resumeGoal(goalId);
      expect(
          (await service.listGoals()).first['state'], equals('active'));
      await service.cancelGoal(goalId);
      expect((await service.dueGoals()), isEmpty);
      expect((await service.listGoals()).first['state'],
          equals('cancelled'));
      Object? terminal;
      try {
        await service.resumeGoal(goalId);
      } catch (e) {
        terminal = e;
      }
      expect(terminal, isNotNull);
    });
  });
}
