import 'package:flutter_test/flutter_test.dart';
import 'package:harbor_app/main.dart';

/// Android back walks back through visited surfaces and leaves the app
/// only from Home (upgrade plan A10: it used to exit from every surface).
void main() {
  test('back returns through visited surfaces to Home, then exits', () {
    final s = AppState();
    expect(s.atRoot, isTrue);
    s.goTo(HarborSurface.models);
    s.goTo(HarborSurface.settings);
    expect(s.atRoot, isFalse);
    expect(s.goBack(), isTrue);
    expect(s.surface, HarborSurface.models);
    expect(s.goBack(), isTrue);
    expect(s.surface, HarborSurface.home);
    expect(s.atRoot, isTrue);
    expect(s.goBack(), isFalse);
  });

  test('a surface reached from the palette backs out to Home', () {
    final s = AppState();
    s.requestOpenFile();
    expect(s.surface, HarborSurface.work);
    expect(s.goBack(), isTrue);
    expect(s.surface, HarborSurface.home);
    expect(s.atRoot, isTrue);
  });

  test('revisiting a surface does not loop the history', () {
    final s = AppState();
    s.goTo(HarborSurface.models);
    s.goTo(HarborSurface.settings);
    s.goTo(HarborSurface.models);
    expect(s.goBack(), isTrue);
    expect(s.surface, HarborSurface.settings);
    expect(s.goBack(), isTrue);
    expect(s.surface, HarborSurface.home);
  });
}
