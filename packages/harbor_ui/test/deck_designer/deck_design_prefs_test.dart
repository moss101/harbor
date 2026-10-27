import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_ui/deck_designer.dart';

void main() {
  test('DeckDesignPrefs round-trips through JSON', () {
    const p = DeckDesignPrefs(
        purpose: 'pitch_deck',
        design: 'ocean',
        palette: 'mint',
        motion: 'none',
        rtl: true);
    expect(DeckDesignPrefs.fromJson(p.toJson()), p);
    expect(DeckDesignPrefs.fromJson(p.toJson()).hashCode, p.hashCode);
  });

  test('absent keys are omitted and bad types are ignored', () {
    expect(const DeckDesignPrefs().toJson(), {'rtl': false});
    final p =
        DeckDesignPrefs.fromJson({'purpose': 3, 'design': 'x', 'rtl': 'y'});
    expect(p, const DeckDesignPrefs(design: 'x'));
  });
}
