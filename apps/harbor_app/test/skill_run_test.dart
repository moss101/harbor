import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/surfaces/skill_run.dart';

/// The approval sheet's rules (decision 0008), without a core: what an
/// edit and a newly created file may each offer.
void main() {
  const hash =
      'aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11';

  test('a created file commits with nothing attached and never overwrites', () {
    final a = ApprovalActions.of(
      {
        'effect_class': 'artifact.commit',
        'proposed_output_hash': hash,
        'creates': true,
        'suggested_name': 'October budget.xlsx',
      },
      attachedName: null,
      attachedHasRealPath: false,
      pickerReturnsUsersFile: true,
    );
    expect(a.creates, isTrue);
    expect(a.canCommit, isTrue);
    expect(a.canOverwrite, isFalse);
    expect(a.saveName, 'October budget.xlsx');
  });

  test('a created file with no suggested name still has one to save under', () {
    final a = ApprovalActions.of(
      {
        'effect_class': 'artifact.commit',
        'proposed_output_hash': hash,
        'creates': true,
      },
      attachedName: null,
      attachedHasRealPath: false,
      pickerReturnsUsersFile: false,
    );
    expect(a.canCommit, isTrue);
    expect(a.saveName, 'Harbor');
  });

  test('an edit needs its original attached and names the copy after it', () {
    final approval = {
      'effect_class': 'artifact.commit',
      'proposed_output_hash': hash,
    };
    final none = ApprovalActions.of(approval,
        attachedName: null,
        attachedHasRealPath: false,
        pickerReturnsUsersFile: true);
    expect(none.canCommit, isFalse);
    expect(none.saveName, isNull);
    final picked = ApprovalActions.of(approval,
        attachedName: 'report.docx',
        attachedHasRealPath: true,
        pickerReturnsUsersFile: true);
    expect(picked.canCommit, isTrue);
    expect(picked.canOverwrite, isTrue);
    expect(picked.saveName, 'report (Harbor).docx');
    // Mobile pickers hand back a copy: Overwrite would write the copy.
    final mobile = ApprovalActions.of(approval,
        attachedName: 'report.docx',
        attachedHasRealPath: true,
        pickerReturnsUsersFile: false);
    expect(mobile.canOverwrite, isFalse);
  });

  test('a proposal with no output hash, or not a commit, cannot be saved', () {
    expect(
        ApprovalActions.of({'effect_class': 'artifact.commit', 'creates': true},
                attachedName: null,
                attachedHasRealPath: false,
                pickerReturnsUsersFile: true)
            .canCommit,
        isFalse);
    expect(
        ApprovalActions.of({
          'effect_class': 'connector.send',
          'proposed_output_hash': hash
        },
                attachedName: 'x.docx',
                attachedHasRealPath: true,
                pickerReturnsUsersFile: true)
            .canCommit,
        isFalse);
    expect(suggestedCopyName('notes'), 'notes (Harbor)');
  });
}
